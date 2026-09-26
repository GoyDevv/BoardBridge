# Status: what is implemented, what is not, and how each claim is verified

This file exists because the fastest way to make a bridge project useless is to
claim support it does not have. Everything below is either **implemented and
verifiable**, **interface only** (the seam exists, the implementation does not,
and the code says so at runtime), or **not started**.

## Implemented

| Area | Where | How it is verified |
|---|---|---|
| Surface lifecycle state machine: `NO_SURFACE`, `SURFACE_PENDING`, `SURFACE_ACTIVE`, `SURFACE_DESTROY_PENDING`, `PAUSED`, `STOPPING`, `STOPPED`; out-of-order and duplicate callbacks; rotation-before-destroy; `Outcome::{Applied,Ignored,Refused}` | `lifecycle/` (pure) | `cargo test` — unit tests walk every transition, including rotation races and the refused-`surfaceCreated`-during-shutdown case |
| Bounded, non-blocking, drop-oldest input queue with counters | `input/queue.rs` | `cargo test` (capacity, overflow, stats) |
| Input translation: `AKEYCODE_*` → `SDL_Scancode`, layout char → `SDL_Keycode`, `META_*` → `SDL_KMOD_*`, gamepad axes/buttons, mouse buttons, touch phases, text | `input/keymap.rs`, `input/sdl_tables.rs` (generated from SDL3 + AOSP headers) | `cargo test` (spot checks incl. `AKEYCODE_A`→`SDL_SCANCODE_A`, `BUTTON_A`→`SDL_GAMEPAD_BUTTON_SOUTH`, trigger normalisation) + the emulator workflow asserting `key DOWN code=29 scancode=A` on a device |
| Generated tables are in sync with upstream headers | `tools/generate_sdl_tables.py` | CI runs the generator in `--check` mode and fails if `sdl_tables.rs` is stale |
| Runtime configuration decoding and defaults | `runtime/config.rs` (pure) | `cargo test` |
| The crate compiles for `aarch64-linux-android`, `armv7-linux-androideabi` and `x86_64-linux-android`, including the `cfg(target_os = "android")` modules the host test run never sees | whole crate | the `apk` job cross-compiles all three ABIs before packaging, and inspects the exported symbols |
| Bridge thread, FIFO command queue, surface epochs, `surfaceDestroyed` fence, `detach_timeouts` counter | `runtime/control.rs` | `cargo test` for the queue/lifecycle parts; `getStatus()` exposes the counters on a device; the emulator workflow produces a second real binding via HOME + relaunch |
| EGL: display, config selection, ES 3.2→3.1→3.0→client-version context chain, window surface, pbuffer, `eglSwapInterval`, GL strings | `egl/` | `cargo test` for the pure parts (attribute lists, fallback chain, config request — these are compiled for every target); on a device, the `GL_VENDOR` / `GL_RENDERER` / `GL_VERSION` string must appear (asserted by CI) |
| GLES backend: single-owner context, revoke/drain/deferred-release teardown, `in_flight` accounting, field-order-safe drop | `graphics/gles.rs` | unit tests for status/stats/decoding live in an Android-gated module (they run only for an Android target, not on the host runner); the emulator workflow exercises bind → draw → present → destroy → rebind, and the counters in `getStatus()` make it visible |
| Frame-loop inversion: `attachGameThread`, `detachGameThread`, `swapBuffers`, context hand-over on loop switch | `runtime/control.rs`, `graphics/gles.rs` | `cargo test` for the loop-mode state; the API is exercised by the demo (attach/swap path is the same code the game will use) |
| Diagnostic renderer: `None`/`Solid`/`Triangle`, centre-pixel readback, legacy-compatible log lines | `render/diagnostics.rs` | `cargo test` (log string shapes, colour maths); CI greps `First frame rendered` and `center_pixel_RGBA=(…)` from a real emulator |
| Diagnostics: `getRendererInfo()`, `getStatus()`, `runSelfTest()`, `ABI_VERSION` reporting | `jni/native_bridge.rs`, `runtime/control.rs` | CI asserts `GL_VERSION=` appears; `runSelfTest()` returns `PASS`/`PARTIAL` based on live checks |
| JNI boundary: 25 entry points, `Int` status codes, panic containment, argument clamping | `jni/`, `error.rs` | `cargo test` for the error codes; the APK build fails if the exported symbols are missing |

## Interface only

The seam exists, is wired into diagnostics, and refuses to do anything it cannot
do honestly. Each returns `ERR_BACKEND_UNAVAILABLE` (code −8) and names the
remaining work in `getStatus()`/`runSelfTest()`.

| Area | Where | Remaining work |
|---|---|---|
| **Vulkan** graphics backend | `graphics/vulkan.rs` | loader, instance/device, `vkCreateAndroidSurfaceKHR` swapchain, queue/threading protocol, LWJGL extension advertising — [GRAPHICS.md](GRAPHICS.md) |
| **SDL3** platform backend | `platform/sdl3.rs` | vendor `libSDL3.so` per ABI, `SDL_CreateWindowWithProperties` + `SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER`, `SDL_PushEvent` mapping, lifecycle→`SDL_EVENT_WINDOW_*` — [SDL3.md](SDL3.md) |
| **GLFW** compatibility backend | `platform/glfw_compat.rs` | a `glfw*` shim (or upstream GLFW with a custom platform backend), callback dispatch, per-window input state, cursor modes, instrumentation — [GLFW_COMPAT.md](GLFW_COMPAT.md) |
| Pen hover / stylus eraser input | `BridgeSurfaceView` | Android reports these as `SOURCE_STYLUS` hover events; SDL models them as `SDL_EVENT_PEN_*`, which `InputEvent` does not have yet |

## Not started

| Area | Notes |
|---|---|
| LWJGL native glue (building LWJGL's `.so` files for Android, pointing its loader at the bridge's GLFW/SDL shims, GL function-pointer lookup) | plan and ordering: [LWJGL.md](LWJGL.md) |
| Launcher features: version/asset download, auth, JVM management, instance folders, UI | explicitly out of scope for this repository; a `launcher-core` on top of the bridge |
| Renderer translation (gl4es, Zink, ANGLE) | a launcher may plug one in above the bridge; the bridge hands over a real EGL context and does not translate GL |

## Verification status of this repository as a whole

Everything claimed above is checked by CI, because this project is developed on
an aarch64 Android device where the Android SDK's x86-64 build tools cannot run:

| Workflow | Checks |
|---|---|
| `.github/workflows/build.yml` (`rust` job) | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, generated-table freshness |
| `.github/workflows/build.yml` (`apk` job) | cross-compiles the Rust core for all three ABIs, builds the APK, verifies the `.so` exists, is 16 KB page aligned, and exports the `Java_com_boardbridge_bridge_NativeBridge_*` symbols, then uploads the APK |
| `.github/workflows/render-test.yml` | boots an emulator, installs the APK, and asserts on logcat: GL strings present, `First frame rendered`, frame statistics with the SOLID centre pixel, `KEYCODE_A` → `SDL_SCANCODE_A`, a touch event reaching the diagnostic renderer, at least two surface bindings across HOME + relaunch, and a clean `runtime stopped` |

`getStatus()` prints the same facts at runtime, so a device report can be
compared against these expectations without guessing.
