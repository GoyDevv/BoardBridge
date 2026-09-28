# Graphics backends

The bridge does not render the game. Minecraft brings its own GL or Vulkan
calls; what the bridge owns is the *environment* those calls need: a window, a
context, a thread it can be current on, and a present operation.
`graphics::GraphicsBackend` is that seam:

```rust
fn initialize(&self) -> Result<()>;
fn bind_window(&self, window: OwnedNativeWindow, requested: SurfaceSize) -> Result<SurfaceSize>;
fn unbind_window(&self) -> Result<()>;
fn make_current(&self, role: ThreadRole) -> Result<()>;
fn release_current(&self) -> Result<()>;
fn present(&self) -> Result<()>;
fn renderer_info(&self) -> Result<RendererInfo>;
fn stats(&self) -> GraphicsStats;
fn set_swap_interval(&self, interval: i32) -> Result<()>;
fn shutdown(&self);                       // plus name/kind/status/describe/window_size
```

Contract, in full: `initialize` may be called once; `bind_window`/`unbind_window`
come only from the bridge thread (the runtime's FIFO command queue);
`make_current`/`release_current`/`present` may come from the bridge thread *or*
the game thread, never concurrently for the same binding; `shutdown` releases
everything and is idempotent; and an implementation must never release an
`ANativeWindow` while an `EGLSurface` created from it exists.

## OpenGL ES via EGL — implemented

`boardbridge/src/egl/` + `boardbridge/src/graphics/gles.rs`.

### Context creation

`Context::create_best` walks a fallback chain, logging every refusal, so a driver
that lacks the newest level degrades instead of failing:

| Step | Request | EGL attributes |
|---|---|---|
| 1 | ES 3.2 | `EGL_CONTEXT_MAJOR_VERSION=3`, `EGL_CONTEXT_MINOR_VERSION=2` |
| 2 | ES 3.1 | 3 / 1 |
| 3 | ES 3.0 | 3 / 0 |
| 4 | ES 3 (client version) | `EGL_CONTEXT_CLIENT_VERSION=3` — for drivers that reject the modern attribute list |

The step that succeeded is reported in `RendererInfo.context_request` and shown
by `runSelfTest()`/`getStatus()` (for example `ES 3.2 via OpenGL ES 3.2`).

### Config selection

`ConfigRequest::launcher()` asks for RGBA8888, depth 24, stencil 8,
`EGL_WINDOW_BIT | EGL_PBUFFER_BIT` and `EGL_OPENGL_ES3_BIT`. The **pbuffer bit is
not optional**: the bridge makes the context current against an offscreen pbuffer
whenever no window is bound, which is what keeps the game's textures, shaders and
VAOs alive across rotation and backgrounding (`preserve_context = true`, the
default). Reduced-depth/stencil variants and an opaque (no-alpha) variant exist
for drivers that expose no matching config.

### EGL lifetime: two failures that look identical

`EGL_BAD_SURFACE` (`0x300d`) from `eglMakeCurrent` on a *non-null* `EGLSurface`
has two quite different causes. Both were real here, and both produced a black
screen, so the distinction is worth keeping.

**1. The handle was already destroyed — by us.** `WindowSurface::create` returned
the surface it had just created through struct update syntax
(`WindowSurface { size, ..surface }`). Every field that syntax takes from the
source is `Copy` (two raw handles and a `SurfaceSize`), so it *copied* them out and
left the source fully initialised; `Drop` then ran `eglDestroySurface` on the
handle being returned. The result was a caller holding a non-null `EGLSurface`
that EGL no longer recognised: `eglCreateWindowSurface` succeeded, and
`eglQuerySurface` and `eglMakeCurrent` four milliseconds later failed with
`0x300d` — on the emulator *and* on a phone, at the first frame. Nothing about
the surface lifecycle could have fixed it: the failure was in the return value.
The fix is to write the size into the struct (`surface.size = size`) rather than
copy the struct, and `tools/host_check_android.sh` now fails CI if any of
`WindowSurface`, `PbufferSurface`, `Context`, `Display`, `WindowBinding` or
`OwnedNativeWindow` is ever built with struct update syntax again. `egl/surface.rs`
carries the long-form explanation next to the code.

**2. Another EGL user terminated the shared display.** The default EGL display is
**process-wide**. libEGL keeps every `EGLSurface` and `EGLContext` made from it in
a per-display object table, and any other EGL user in the process (the activity's
own HWUI render thread, a library, a game's own GL setup) may call `eglTerminate`
on that display. When that happens our handles stay non-null but stop resolving:
every later call on them fails with `EGL_BAD_SURFACE`, including
`eglDestroySurface`. This has not been observed in a log yet — it is the failure
the design below defends against, and it is why the checks list an *unrecovered*
`0x300d` as the thing to report (the recovery is legitimate; a busy log line is
not a broken bridge).

Two rules follow from cause 2, and both are implemented:

1. **EGL is created late.** Nothing in `GlesBackend::initialize` touches EGL: the
display, config, context and offscreen surface are created by `ensure_display` on
the **first window bind**, so the display, its context and the surface that is
made current are only ever a few instructions apart. (The first Rust rewrite
created EGL in `createRuntime` — inside `Activity.onCreate`, a second or more
before the `SurfaceView` had a surface — which is exactly the window in which the
handles could be invalidated.) The former C++ core created EGL inside the render
thread that starts on `surfaceCreated`; this restores that ordering.

2. **A failed `eglMakeCurrent` is a state problem, not a frame problem.**
`GlesBackend::make_current` treats it as "the display is gone" and recovers in
place, bounded by `MAX_EGL_REBUILDS` (3) per binding generation:

   * **cheap path** — recreate only the `EGLSurface` from the `ANativeWindow` the
     binding still owns, keeping the display and the context. libEGL validates the
     *context* before the surface and reports `EGL_BAD_CONTEXT` when that is the
     missing object, so `EGL_BAD_SURFACE` means the context survived: keeping it is
     what keeps the game's textures, shaders and VAOs alive, which is the promise
     `preserve_context` makes;
   * **full rebuild** — otherwise forget every handle *without* calling back into
     EGL (`disarm`, which prevents use-after-free inside libEGL) and build a fresh
     display, context and surface.

   `WindowSurface`/`PbufferSurface`/`Context`/`Display` all expose `disarm()` for
   this; the retry happens inside the same `make_current` call, so the frame is
   drawn rather than dropped. If the rebuild also fails the binding is revoked, the
   lifecycle machine is moved to `NO_SURFACE` (the game is told through the input
   queue), and the render loop parks on its command queue instead of spinning — a
   new `surfaceCreated` (rotation, relaunch) starts a clean generation.

A failing frame is also paced (`FAILED_FRAME_FPS`) and logged at most once per
`ATTACH_LOG_INTERVAL` (5 s): an uncapped loop once emitted 129 000 warnings in
19 seconds, which filled logcat's ring buffer and pushed every earlier diagnostic
line — including the evidence for the failure itself — out of it.

### Window binding and the revoke/drain fence

`WindowBinding` owns, in declaration order: the `EGLSurface`, then the
`ANativeWindow`. Rust drops fields in order, so the surface is *always* destroyed
before the window reference is released — the single most important property in
this file, and the reason the struct's field order carries a comment saying so.

Teardown (`retire`) is not a `sleep`:

1. mark the binding **revoked** — no new `present` can start, in-flight ones are
   counted and rejected (`rejected_presents`);
2. wait, with a deadline (`drain_timeout_ms`, 250 ms), for in-flight presents to
   finish and for the owning thread to release the context;
3. if that succeeds: `eglDestroySurface` → `ANativeWindow_release`;
4. if it does not: **defer** — the binding is moved to a deferred list, the
   window reference stays with the backend, and the release is retried when the
   owning thread detaches. Worst case: a delayed release, never a dangling
   pointer.

`eglSwapBuffers` runs with the backend lock released (the only call that can
block for a whole vsync), guarded by an `InFlight` RAII counter, so a long frame
cannot block the UI thread's `surfaceDestroyed` beyond its fence and cannot
corrupt the counters if it unwinds.

### Diagnostics

`getStatus()` prints the backend counters (`presents=`, `failures=`,
`rejected=`, `generations=`, `deferred=`, `in_flight=`, `bound=`, `revoked=`,
`owner=`), the EGL display/config description, and the GL strings.
`runSelfTest()` reads back a real pixel; see [STATUS.md](STATUS.md).

## Vulkan — interface only

`boardbridge/src/graphics/vulkan.rs`. Nothing there works, and nothing pretends
to: `initialize()` returns `Error::BackendUnavailable`, every other operation
fails or returns an empty value, and `status()` reports
`InterfaceOnly { remaining }` so `getStatus()` and the launcher UI can say
"Vulkan: not implemented" out loud. Selecting it is *not* fatal to the APK: the
bridge thread logs the failure, stops, and `getStatus()` reports
`thread_running=false` plus the error.

Exact remaining work (also in the module header):

1. **Loader** — `dlopen("libvulkan.so.1")` with a `libvulkan.so` fallback,
   resolve `vkGetInstanceProcAddr`, build the dispatch table. Android's loader
   differs from a desktop one: the ICD is loaded by the platform.
2. **Instance/device** — `VkApplicationInfo` with `apiVersion` negotiated from
   `vkEnumerateInstanceVersion`, one graphics+present queue family,
   `VK_KHR_swapchain` (or `VK_ANDROID_external_memory_android_hardware_buffer` if
   the game renders into an external image).
3. **Surface** — `vkCreateAndroidSurfaceKHR` from the *same* `ANativeWindow` the
   EGL path owns; the swapchain must be recreated on every `bind_window`
   (rotation invalidates it), with `oldSwapchain` where supported, and
   `VK_ERROR_OUT_OF_DATE_KHR` / `VK_SUBOPTIMAL_KHR` handled on present.
4. **Threading** — the same owner/fence protocol the GLES backend implements: a
   Vulkan queue is externally synchronised, so "one owner at a time" becomes "one
   submitting thread at a time".
5. **LWJGL compatibility** — the extensions Minecraft asks for must be advertised
   as available; the bridge supplies the surface/instance environment, exactly as
   it supplies EGL for the GL path.
6. **Diagnostics** — either a `None`-mode-only Vulkan backend or a small
   compute-to-image clear for the self-test.

Why the seam exists now rather than later: Minecraft Java Edition is migrating
its renderer to Vulkan, and Android is the one platform where a launcher chooses
the swapchain instead of inheriting a desktop one. With the seam in place, that
work is a new `impl GraphicsBackend`, not a rewrite of the lifecycle, input and
JNI layers.

## Adding a backend

1. Implement `GraphicsBackend` in a new module under `boardbridge/src/graphics/`.
2. Add a `RendererKind` variant and decode it in `RendererKind::from_jni` (and in
   `NativeBridge.Renderer` on the Kotlin side).
3. Return `BackendStatus::InterfaceOnly { remaining }` until it really works —
   `getStatus()`, `runSelfTest()` and the honesty ledger in
   [STATUS.md](STATUS.md) all read that value, so an unfinished backend cannot be
   mistaken for a working one.
4. Wire it in `graphics::create` and, if it needs a different present model
   (Vulkan does), extend `set_swap_interval`'s meaning there rather than
   inventing a second trait.
5. Add tests for the pure parts (decoding, stats, status reporting) and a CI
   check for the parts that need a device.
