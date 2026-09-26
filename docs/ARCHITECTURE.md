# Architecture

BoardBridge is a bridge, not a renderer. Minecraft brings its own GL/Vulkan
calls, its own shaders and its own world; what it cannot bring on Android is the
platform plumbing: a window that survives rotation, an EGL context that can be
made current on the game's thread, and input in the shape the game expects
(SDL3 or GLFW), not in the shape Android delivers.

Three layers, one direction of trust: **Kotlin reports, Rust decides.**

```
┌──────────────────────────────────────┐
│ app/ — Kotlin shell                  │  SurfaceHolder callbacks, MotionEvent/
│  BridgeSurfaceView, BridgeRuntime    │  KeyEvent/IME, activity lifecycle
└───────────────┬──────────────────────┘
                │ JNI (NativeBridge, ABI v2, primitive args + one return code)
┌───────────────▼──────────────────────┐
│ boardbridge/ — Rust core             │  lifecycle machine, command queue,
│  runtime, lifecycle, input, graphics │  EGL, window ownership, input translation
└───────────────┬──────────────────────┘
                │ EGL / ANativeWindow / (future) libSDL3.so / libglfw.so
┌───────────────▼──────────────────────┐
│ Android platform                     │  BufferQueue, SurfaceFlinger, input stack
└──────────────────────────────────────┘
```

## Module map

| Module | Responsibility | Portable to a host build? |
|---|---|---|
| `error` | error type and the stable negative codes Kotlin sees | yes |
| `log` | logcat (stderr off-device) with level filtering | yes |
| `lifecycle` | `NO_SURFACE` … `STOPPED` state machine; pure logic | yes (unit-tested) |
| `input` | SDL-shaped events, bounded queue, Android→SDL keymap, generated SDL/AOSP tables | yes (unit-tested) |
| `runtime::config` | everything Kotlin may configure | yes (unit-tested) |
| `runtime::control` | bridge thread, command queue, fences, surface epochs | no |
| `runtime::registry` | the process-wide `Runtime` handle the JNI layer talks to | no |
| `android` | `ANativeWindow` ownership, JNI→event adapters | no |
| `egl` | `EGLDisplay` / `EGLConfig` / `EGLContext` / `EGLSurface` in Rust | no |
| `graphics` | the backend trait, the GLES implementation, the Vulkan interface | no |
| `render` | the bridge's own diagnostic renderer (solid clear / triangle) | no |
| `platform` | who supplies windows and input: Android native, SDL3, GLFW | no |
| `jni` | the exported entry points Kotlin calls | no |

The `#[cfg(target_os = "android")]` gate in `lib.rs` is what makes
`cargo test` on a plain Linux/macOS runner meaningful: the pure modules above are
compiled and tested there, while the platform modules are compiled only when
cross-compiled for Android (CI does that in the `apk` job).

## Ownership rules

These four rules are the whole design; everything else follows from them.

1. **The `ANativeWindow` reference is owned by exactly one Rust value at a
   time.** `OwnedNativeWindow` acquires it on the Kotlin/UI thread
   (`ANativeWindow_fromSurface`) and never clones it. It travels through the
   command queue into the graphics backend, and the backend drops it — after
   destroying the `EGLSurface` that was created from it — in `unbind_window`,
   in `shutdown`, or immediately if the bind failed. See
   `boardbridge/src/android/surface.rs`.
2. **The UI thread never waits for EGL work, except at `surfaceDestroyed`,
   and even then only for a bounded fence.** `surfaceCreated` and
   `surfaceChanged` queue a command and return; `surfaceDestroyed` waits up to
   `detach_timeout_ms` (250 ms) for the bridge thread to acknowledge, and if the
   fence expires the release still completes asynchronously — the window stays
   valid for Rust until it does, because Rust holds the reference, and Android
   cannot free the buffer before that reference is released.
3. **Only one thread owns the EGL context at a time**, and who owns it is
   explicit (`ThreadRole::Bridge` or `ThreadRole::Game`). The game thread
   attaches with `attachGameThread()`; the bridge thread releases the context
   before it acknowledges the switch, so an attach never races the internal
   loop.
4. **Input is translated once, at the boundary.** A key event enters the queue
   as a real `SDL_Scancode`/`SDL_Keycode` with `SDL_KMOD_*` bits, not as an
   Android keycode with an SDL name sprayed on later. The game — or a future
   SDL3/GLFW backend — reads the queue, not Android.

## The JNI contract

Kotlin calls `NativeBridge`, whose methods are a 1:1 mirror of
`boardbridge/src/jni/native_bridge.rs`. The contract is deliberately dull:

- **No object handles.** The runtime is a process-wide singleton
  (`runtime::registry`); Kotlin never passes a pointer or receives one.
- **Primitive arguments only**, plus `Surface` (turned into an `ANativeWindow`
  immediately) and `String` (for IME text).
- **One `Int` return code per call**, `0` for success and a stable negative
  value for failure (`error.rs` → `NativeBridge.ERR_*`). String-returning
  diagnostics (`getRendererInfo`, `getStatus`, `runSelfTest`) return `""` rather
  than throwing.
- **Panics cannot cross the boundary.** Every entry point runs inside
  `helpers::guarded`, and release builds set `panic = "abort"`, so a panic is
  either contained (debug) or fatal and visible (release) — never undefined
  behaviour in the JVM.
- **`ABI_VERSION`** is bumped whenever this surface changes, and reported by
  `getStatus()`, so a stale `.so` in an APK is obvious in logcat.

Input crosses JNI in batches: `sendTouchBatch` carries one `MotionEvent`'s
pointers as five parallel arrays (ids, normalized phases, x, y, pressure) with a
`count`, so a five-finger move costs one call, not five. The JNI layer clamps
`count` at 32 and rejects anything malformed with `ERR_INVALID_ARGUMENT`.

## Two loops, one surface

| | Internal loop (`LoopMode::Internal`, default) | Inverted loop (`LoopMode::Inverted`) |
|---|---|---|
| Who draws | the bridge thread (`render::DiagnosticRenderer`) | the game (Minecraft), on its own thread |
| Who presents | the bridge thread | the game, via `swapBuffers()` |
| Who gets the context | bridge thread | game thread, after `attachGameThread()` |
| Surface loss reaches the game as | not applicable | a queued `LifecycleNotice::SurfaceRevoked` |
| Used by | the demo app, `runSelfTest()`, CI | a launcher running the real game |

Both modes share the same surface/EGL ownership rules, which is why the CI
render test is meaningful evidence for the game path: the demo runs the internal
loop over the identical lifecycle machine, EGL backend and input queue that the
inverted loop uses.

The switch is not cosmetic: `set_loop_mode` queues a command, and the bridge
thread releases the EGL context *before* acknowledging it, so a game thread that
attaches afterwards is never refused (`ERR_SURFACE_BUSY`/ownership conflict).

## Graphics: a seam, not an abstraction for its own sake

`graphics::GraphicsBackend` describes only what the bridge must own on the
game's behalf: bind a window, make a context current on some thread, present,
report GL strings and counters, shut down. GLES implements it today; Vulkan is
an interface that fails fast and names its remaining work. Adding a backend does
not touch the lifecycle, the input layer or the JNI shell — see
[GRAPHICS.md](GRAPHICS.md).

## Platform: what the game calls

`platform::PlatformBackend` is the seam for the *game-facing* APIs: the
implemented `android` backend (Kotlin pushes events straight into the queue),
and the interface-only `sdl3` and `glfw_compat` backends. Those two exist now so
that the migration from GLFW to SDL3 in Minecraft (26.3+) is a new
implementation behind a trait, not a rewrite. What is already done for both —
and not faked — is the hard part: the Android→SDL translation. See
[SDL3.md](SDL3.md) and [GLFW_COMPAT.md](GLFW_COMPAT.md).

## Diagnostics

Three entry points, all safe to call at any time from any thread:

- `getRendererInfo()` → `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…` (empty
  until the context exists).
- `getStatus()` → one line: abi, thread state, lifecycle summary, loop mode,
  graphics counters, input-queue counters, runtime counters, EGL config,
  renderer strings, and every platform backend with its status.
- `runSelfTest()` → a multi-line audit: what was bound, which context level
  succeeded, what the queue saw, and what is not implemented.

Everything the bridge does is observable through these three calls plus logcat;
that is deliberate, because on-device debugging is the expensive kind.
