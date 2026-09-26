# Migration: C++ demo → Rust bridge

This document is for two readers: someone who knew the previous C++/Kotlin
version of this repository, and a launcher author adopting the new Kotlin API.

## What changed, in one table

| Concern | Before (C++ demo) | Now (Rust core) |
|---|---|---|
| Native language | C++17, `app/src/main/cpp` | Rust 2021, `boardbridge/` |
| Build | `externalNativeBuild` + CMake + libc++ | `cargo ndk` invoked by Gradle, no CMake |
| JNI surface | 11 methods, `Boolean` returns for `attach`/`swap` | 25 methods, `Int` status codes everywhere, `ABI_VERSION = 2` |
| Runtime handle | `static std::unique_ptr<RenderThread>` guarded by a mutex, ad-hoc error handling | `registry::Runtime` (one per process) + `Error` with stable negative codes |
| Surface destroy | flag + `sleep(2s)`-style wait in `clearWindow()` | command id, lifecycle machine, 250 ms fence, revoke + drain + deferred-release |
| EGL context chain | ES 3.2 → 3.0 → legacy | ES 3.2 → 3.1 → 3.0 → ES 3 client-version |
| Loop | render thread only; "inverted mode" flag with `sleep` | `LoopMode::{Internal, Inverted}` with explicit context hand-over |
| Input | `onTouch(pointerId, action, x, y, time)` per pointer; Android keycodes passed through | `sendTouchBatch(...)` per `MotionEvent`; **Android → SDL translation** (`SDL_Scancode`, `SDL_KMOD_*`, gamepad axes/buttons) |
| Text input | not supported | `sendText` (IME commits) |
| Gamepads | not supported | `sendGamepadAxis` + gamepad button triage by device class |
| Diagnostics | `getRendererInfo()` only | `getRendererInfo()`, `getStatus()`, `runSelfTest()` |
| Kotlin package | `com.boardbridge.egl` | `com.boardbridge.bridge` |
| Connectivity | diagnostics only | plus `platform::{sdl3, glfw_compat}` and `graphics::vulkan` interfaces |

## Module mapping

| Old C++ file | New Rust |
|---|---|
| `native_bridge.cpp` | `jni/native_bridge.rs` (entry points), `android/surface.rs` (`ANativeWindow` ownership), `android/input.rs` (argument → event) |
| `render_thread.{h,cpp}` | `runtime/control.rs` (thread, command queue, fences), `graphics/gles.rs` (GL state), `render/diagnostics.rs` (solid/triangle) |
| `egl_core.{h,cpp}` | `egl/{ffi,display,context,surface}.rs` |
| `input_queue.{h,cpp}` | `input/{event,queue,keymap}.rs` + `input/sdl_tables.rs` (generated) |
| `log.h` | `log.rs` (logcat with level filtering) |
| `CMakeLists.txt` | `boardbridge/Cargo.toml` + the `cargoBuildBoardBridge` Gradle task |

## Entry-point mapping

| Old Kotlin (`com.boardbridge.egl`) | New Kotlin (`com.boardbridge.bridge`) |
|---|---|
| `NativeBridge.onSurfaceCreated(surface)` | `BridgeRuntime.surfaceCreated(surface)` → `NativeBridge.surfaceCreated(surface)` |
| `onSurfaceChanged(w, h)` | `surfaceChanged(w, h, format)` |
| `onSurfaceDestroyed()` | `surfaceDestroyed()` |
| `onResume()` / `onPause()` | same names (now returning codes) |
| `onDestroy()` | `destroyRuntime()` |
| `attachCurrentThread(): Boolean` | `attachGameThread(): Int` (+ `detachGameThread()`) |
| `swap()` | `swapBuffers()` |
| `setInvertedMode(boolean)` | `setRenderLoopMode(NativeBridge.LoopMode.INVERTED)` |
| `setRenderMode(...)` (internal to the demo) | `setRenderMode(NativeBridge.RenderMode.*)` |
| `onTouch(pointerId, action, x, y, time)` | `sendTouchBatch(ids, phases, xs, ys, pressures, count, time)` |
| `onKey(keyCode, action, unicodeChar, time)` | `sendKey(keyCode, pressed, repeat, unicodeChar, deviceKind, deviceId, time)` |
| — | `sendText`, `sendMouseMotion`, `sendMouseButton`, `sendMouseWheel`, `sendGamepadAxis` |
| `getRendererInfo(): String` | same, plus `getStatus()` and `runSelfTest()` |
| — | `createRuntime(renderer, loopMode, flags, diagnosticMode, targetFps)` |

Runtime creation is no longer implicit: the runtime must be created before any
surface callback arrives (the demo does it in `onCreate`). `NO_SURFACE` is a
valid starting state.

## The three behavioural fixes worth knowing

1. **`surfaceDestroyed` no longer blocks the UI thread for two seconds.** It
   waits, at most 250 ms, for a *command id* to complete, and the release
   completes on the bridge thread if the fence expires. Safety does not depend on
   the wait: the window reference is a Rust value, released only after the
   `EGLSurface` created from it is destroyed. See [THREADING.md](THREADING.md).
2. **Input is translated once, correctly.** The old code handed Android keycodes
   to the game and let the game's GLFW/LWJGL layer guess. Now a key event is
   queued with a real `SDL_Scancode`/`SDL_Keycode` and `SDL_KMOD_*` bits, and
   gamepad buttons are distinguished from keyboard keys by device class. The log
   line `key DOWN code=29 scancode=A` is the proof.
3. **Failures are named.** Where the old code returned a `bool` or logged at
   debug level, every call now returns a code from `error.rs`, and Kotlin maps it
   to a name plus the lifecycle state. `ERR_INVALID_STATE` in logcat is now a
   diagnosis, not a mystery.

## Adopting it in a launcher

1. **Copy the shell, not the demo.** `NativeBridge.kt` + `BridgeRuntime.kt` are
   the reusable half; `MainActivity.kt` and the solid/triangle diagnostic modes
   are demo/CI scaffolding.
2. **Start the runtime early** (in `onCreate`) and stop it in `onDestroy`; the
   surface may arrive before or after either, in any order, and the state machine
   handles it.
3. **Keep the surface callbacks exactly as `BridgeSurfaceView` implements them.**
   They are the contract; do not add logic between Android and the bridge except
   logging.
4. **For Minecraft (internal loop off):**
   ```kotlin
   BridgeRuntime.start(BridgeRuntime.Config(
       renderer = NativeBridge.Renderer.AUTO,
       loopMode = NativeBridge.LoopMode.INVERTED,
       diagnosticMode = NativeBridge.RenderMode.NONE,  // not your UI, not your clear colour
       preserveContext = true,                          // keep the game's GL objects across rotation
   ))
   ```
   then, on the game's own thread after a surface exists:
   `attachGameThread()` → render → `swapBuffers()` → `detachGameThread()` on
   teardown. Never from the UI thread.
5. **Handle `ERR_SURFACE_REVOKED`**: it means the surface went away while the
   game was presenting. Stop presenting, wait for the next `surfaceCreated`
   (`getStatus()` reports the state), and re-attach. Do not retry in a loop.
6. **Watch `getStatus()` in your bug reports.** It prints the ABI version, the
   lifecycle state, both counter sets, the EGL config and every backend's status
   on one line — the difference between a guess and a diagnosis.

## What is deliberately *not* here

- No JVM management (`libjvm.so`, `java.home`, classpath): the launcher owns it.
- No downloads/auth/instances/UI: a separate `launcher-core`.
- No renderer translation (gl4es/Zink/ANGLE): the bridge hands over a real EGL
  context; a launcher that needs a GL translator plugs it in above the bridge.
- No fake SDL3/GLFW/Vulkan: see [SDL3.md](SDL3.md), [GLFW_COMPAT.md](GLFW_COMPAT.md),
  [GRAPHICS.md](GRAPHICS.md) and [LWJGL.md](LWJGL.md), each of which lists the
  exact remaining work.
