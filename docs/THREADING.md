# Threading and the surface lifecycle

The two ways this kind of bridge breaks are, in order of frequency:

1. **Touching an `ANativeWindow` after Android reclaimed it.** Symptom: a crash
   in a driver with no useful stack, only during rotation or app switching.
2. **Making EGL calls on the wrong thread, or on two threads at once.**
   Symptom: random frames, hangs in `eglSwapBuffers`, or a context that "goes
   away" when the game starts rendering.

Everything below exists to make those two impossible rather than unlikely. There
is no `sleep()` on any path, and no timeout is used as a correctness mechanism.

## Threads

| Thread | Created by | Owns | May call | Must never |
|---|---|---|---|---|
| Android UI | Android | `Surface` callbacks, input callbacks | `createRuntime`, `surface*`, `onPause`/`onResume`, all `send*` input calls, diagnostics | block for more than the surface fence (≤ 250 ms); call EGL |
| bridge (`BoardBridge`) | `Runtime::start` | EGL lifecycle, window binding, the diagnostic loop | `bind_window`, `unbind_window`, `initialize`, `shutdown`, `present`, `make_current` | touch a released window; spin when there is nothing to do |
| game (Minecraft) | the launcher | its own GL work and world state | `attachGameThread`, `swapBuffers`, `detachGameThread`, `poll_event`, diagnostics | present after `ERR_SURFACE_REVOKED`; attach while another thread owns the context |
| any | — | — | `getStatus`, `getRendererInfo`, `setRenderMode`, `send*` | `createRuntime` twice |

`InputEvent` producers (the UI thread) and the consumer (bridge thread or game
thread) are decoupled by the bounded queue in `boardbridge/src/input/queue.rs`:
capacity 4096 by default, push never blocks, and an overflow drops the *oldest*
event and counts it (`dropped=` in `getStatus`). A burst can therefore never
stall a frame.

## The lifecycle state machine

`boardbridge/src/lifecycle/` is pure logic (no Android calls), unit-tested on the
CI runner (`cargo test`).

It tracks **two independent facts**, because Android delivers them
independently — `onPause()` and `surfaceDestroyed()` can arrive in either order
depending on whether the user backgrounded the app, rotated the device, resized
a multi-window frame or folded a device:

- the surface binding: `NO_SURFACE` → `SURFACE_PENDING` → `SURFACE_ACTIVE` →
  `SURFACE_DESTROY_PENDING` → `NO_SURFACE`, plus the terminal `STOPPING` and
  `STOPPED`;
- the activity state: `RESUMED` / `PAUSED`.

`reported_state()` composes them into exactly seven names for logs and
diagnostics — `NO_SURFACE`, `SURFACE_PENDING`, `SURFACE_ACTIVE`,
`SURFACE_DESTROY_PENDING`, `PAUSED`, `STOPPING`, `STOPPED` — where `PAUSED` wins
whenever the activity is paused and the runtime is alive, because that single
fact is what a reader of logcat actually needs.

Transitions, as implemented (`machine.rs::on`):

| Event | From | To | Notes |
|---|---|---|---|
| `SurfaceCreated` | `NO_SURFACE`, `SURFACE_ACTIVE`, `SURFACE_DESTROY_PENDING` | `SURFACE_PENDING` | accepted while the previous binding is still being retired (rotation); `epoch` increments |
| `SurfaceBound` | `SURFACE_PENDING` | `SURFACE_ACTIVE` | reported by the bridge thread after EGL succeeded |
| `SurfaceBindFailed` | `SURFACE_PENDING` | `NO_SURFACE` | the backend already released the window |
| `SurfaceChanged` | `PENDING`, `ACTIVE` | unchanged | size only |
| `SurfaceDestroyed` | `SURFACE_ACTIVE` | `SURFACE_DESTROY_PENDING` | the binding is retired asynchronously |
| `SurfaceDestroyed` | `SURFACE_PENDING` | `NO_SURFACE` | a bind that never completed is dropped |
| `SurfaceUnbound` | `ACTIVE`, `DESTROY_PENDING` | `NO_SURFACE` | reported by the bridge thread |
| `Pause` / `Resume` | any live state | same state | activity flag only; the surface usually survives `onPause` |
| `StopRequested` | any live state | `STOPPING` | `surfaceCreated` afterwards is **refused**, and the caller must release the window |
| `Stopped` | `STOPPING` | `STOPPED` | the bridge thread exited |

Every event returns an `Outcome`: `Applied`, `Ignored` (benign duplicate or
out-of-order callback) or `Refused` (the caller must release a resource). This is
why a duplicate `surfaceCreated` from a misbehaving OEM cannot leak an
`ANativeWindow`: the refusal carries the requirement to drop the reference, and
`Registry`/`Runtime` do exactly that.

## The surface fences

`surfaceDestroyed` is the only call that waits, and it waits on a *command id*,
not on a duration guess:

```
UI thread                                  bridge thread
─────────                                  ─────────────
lifecycle: ACTIVE → DESTROY_PENDING
queue Detach (FIFO, always after any Attach)
wait ≤ detach_timeout_ms (250 ms)  ────▶   on Detach:
on command completion:  return             1. mark the binding revoked
                                           2. wait ≤ drain_timeout_ms for
                                              in-flight presents + the owner
                                              ├─ released → destroy EGLSurface,
                                              │             release ANativeWindow
                                              └─ still held → defer: keep the
                                                 window reference, retry when
                                                 that thread releases the context
                                           3. lifecycle: → NO_SURFACE
                                           4. queue LifecycleNotice::SurfaceRevoked
```

If the wait expires, the UI thread returns anyway — logging a warning and
counting `detach_timeouts` — and the release still completes on the bridge
thread. Android cannot free the buffer before that, because Rust holds the
`ANativeWindow` reference (rule 1 in [ARCHITECTURE.md](ARCHITECTURE.md)). The
fence is a latency optimisation for the UI thread, never a safety mechanism.

## Rotation: epochs, not luck

Android can deliver `surfaceCreated` for the new window *before* the old window's
`surfaceDestroyed` completes. Two mechanisms keep that safe:

- **FIFO command queue.** Detach commands are queued in arrival order, so "retire
  the old binding" is always processed before "bind the new one", regardless of
  how many rotations arrive at once.
- **Surface epochs.** Every accepted `surfaceCreated` gets a monotonic epoch; the
  bridge thread drops any `Attach` whose epoch is lower than the newest queued
  epoch (`dropped_attaches=` in diagnostics). Stale windows are therefore released
  immediately instead of being bound and immediately torn down.

Both are exercised by unit tests (`cargo test` in `lifecycle` and `runtime`), and
the emulator workflow drives HOME + relaunch to produce a second real binding.

## EGL ownership

EGL requires a context to be current on at most one thread at a time. The GLES
backend enforces it with an explicit owner:

- `make_current(role)` records `ThreadId` + `ThreadRole`; a second thread asking
  while another owns the context gets `ERR_SURFACE_BUSY`.
- `present()` refuses to present unless the calling thread *is* the owner
  (`ERR_INVALID_STATE`), and refuses outright if the binding was revoked
  (`ERR_SURFACE_REVOKED`, counted as `rejected_presents`).
- The window target is used when a binding is live; otherwise the backend makes
  the context current against a **pbuffer**, which is what lets the game's GL
  objects survive a rotation when `preserve_context` is on (the default).
- `eglSwapBuffers` — the only call that can wait for vsync — runs with the
  backend lock released, guarded by an `InFlight` RAII counter so the counters
  stay correct even if the call unwinds.

`set_loop_mode` hands the context over: the bridge thread releases the context
*before* acknowledging the command, so a game thread that attaches right after
the switch is never refused.

## What "no sleep and hope" replaced

The previous C++ demo made `surfaceDestroyed` clear a flag, `sleep(2s)`-style
wait for the render thread to notice, and then return — a design that both stalls
the UI thread for two seconds and *still* races if the render thread was inside a
driver call. The Rust bridge replaces it with: a command id, a bounded wait on a
condition variable, a revoke flag that rejects in-flight work, a drain with a
deadline, and a deferred-release path that cannot dangle. See
[MIGRATION.md](MIGRATION.md) for the before/after mapping.
