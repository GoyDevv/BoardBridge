# Design & lineage

BoardBridge binds an Android `Surface` to an OpenGL ES (or, later, Vulkan)
context **in native code** and hands the game a window, a context it can make
current on its own thread, and input in SDL/GLFW shape — with the surface
lifecycle handled so that no thread can ever touch a window Android has already
reclaimed.

It modernizes the Surface-to-GL approach of
[Boardwalk](https://github.com/zhuowei/Boardwalk) (by zhuowei, Apache-2.0). This
document records that lineage honestly and states the design rules the code
follows; the concrete architecture is in [ARCHITECTURE.md](ARCHITECTURE.md),
threading and lifecycle in [THREADING.md](THREADING.md).

---

## 1. What Boardwalk actually did (the honest starting point)

Reading the archived Boardwalk sources shows that the original project did **not**
hand-write an EGL/Surface bridge:

- **`BoardwalkGLSurfaceView.java`** is a thin subclass of the framework
  `android.opengl.GLSurfaceView`. The framework class is what actually performs
  the `Surface` → `EGLSurface` binding (internally via its `EglHelper`, using
  `eglCreateWindowSurface` on the `SurfaceHolder`) and manages the surface
  lifecycle through `SurfaceHolder.Callback` on an internal `GLThread`. The
  Boardwalk subclass only overrides `surfaceDestroyed` to print a line.
- **`jni/main.c` + `gdvm.h`** are not graphics code at all — they are Dalvik VM
  heap/stack tweaks (`DalvikTweaks`) reached via `dlsym` into `libdvm.so`, used
  to make the desktop LWJGL/Minecraft workload fit the old VM.
- **`jni/catcher.c`** is an anti-tamper constructor.
- **`Android.mk` / `Application.mk`** are ancient `ndk-build` configs
  (`APP_PLATFORM := android-14`, `APP_STL := gnustl_shared`, armeabi-v7a first).
- Input was handled by the LWJGL-Android port, which is not in this repo.

So **“the Boardwalk EGL/Surface bridge” is really the GLSurfaceView pattern**:
surface lifecycle via `SurfaceHolder`, EGL managed by the framework. That is the
conceptual seed BoardBridge takes and rebuilds natively — the framework's EGL
ownership, its `GLThread`, its virtual controls and its JVM tweaks are all
replaced with explicit, auditable code.

---

## 2. How Android GL launchers bridge a Surface (background)

> The following is a high-level, plain-English description of the well-known
> pattern that Android OpenGL game launchers use to get a desktop-style GL
> workload onto an Android `Surface`, written from general knowledge of the
> architecture. It is background / prior-art only; BoardBridge's implementation
> is original.

- **The Surface is just a canvas handed to native code.** The launcher shows a
  `SurfaceView` (or `TextureView`). When Android creates the underlying
  `Surface`, it is passed down through JNI, where native code turns it into an
  `ANativeWindow` (via `ANativeWindow_fromSurface`). The Java/Kotlin side does
  *not* own the GL context.
- **A pluggable “GL bridge” provides the actual context.** Rather than a single
  hard-coded path, these launchers select a renderer at runtime: system EGL +
  GLES directly, a bundled GL→GLES translator (gl4es / holy-gl4es) for desktop
  OpenGL, or GL-on-Vulkan (Zink/Mesa), sometimes via ANGLE. Whatever the
  backend, the fundamental step is the same: create an `EGLDisplay`, choose a
  config, create an `EGLContext`, and create an `EGLSurface` bound to the
  `ANativeWindow`, then `eglMakeCurrent` on the render/game thread and
  `eglSwapBuffers` each frame.
- **Surface lifecycle is marshaled to native and made thread-safe.**
  `surfaceCreated` / `surfaceChanged` / `surfaceDestroyed` events are forwarded
  to native. On destroy, the `EGLSurface` is torn down and the `ANativeWindow`
  released *before* the callback returns, so the render thread never touches a
  window that Android has reclaimed. On (re)create — e.g. returning from
  background — a new `EGLSurface` is bound to the new window.
- **Input is captured on the view and translated into the game’s input model.**
  Touch, hardware keyboard, and mouse events are intercepted and fed to a custom
  GLFW/LWJGL bridge the game reads. On top of that sit a virtual mouse/cursor,
  on-screen touch controls, gamepad mapping, and IME text input — plus a
  control-mapping editor. Modern launchers pair this with a Kotlin/Compose UI,
  current-Android support, and runtime selection of the Java runtime and
  renderer.

**What BoardBridge borrows conceptually:** the “hand the raw Surface to native,
own EGL there, run a dedicated bridge thread, and marshal a thread-safe surface
lifecycle plus an input queue” shape. BoardBridge implements the bridge itself —
window ownership, EGL, the loop model, input translation — and deliberately does
*not* implement translators, virtual controls, JVM management or a game runtime.

---

## 3. The design rules

1. **Android reports, Rust decides.** Kotlin's callbacks describe what Android
   thinks happened; the Rust lifecycle machine and the GLES backend describe what
   actually happened. Both feed the same state machine, and every JNI call
   returns a code instead of a hopeful boolean.
2. **Ownership is a single Rust value.** One `OwnedNativeWindow` per binding, no
   clones; `EGLSurface` is always destroyed before the window reference is
   released (the field order in `WindowBinding` is load-bearing).
3. **No sleep-based synchronisation, anywhere.** Waits are on command ids with
   deadlines; expiry defers work instead of guessing. See
   [THREADING.md](THREADING.md).
4. **Translate once, at the boundary.** Input enters the queue as real SDL
   values (`SDL_Scancode`, `SDL_KMOD_*`, `SDL_GAMEPAD_AXIS_*`), generated from
   SDL3's own headers, not as renamed Android keycodes.
5. **Unimplemented is a first-class state.** Vulkan, SDL3 and GLFW report
   `interface-only` with the remaining work named, and fail with
   `ERR_BACKEND_UNAVAILABLE` rather than silently succeeding
   ([STATUS.md](STATUS.md)).
6. **Observable from a phone.** `getStatus()`/`runSelfTest()`/logcat must be
   enough to diagnose a device problem without a debugger.

---

## 4. Modernization table

| Concern | Boardwalk (2015–2020) | BoardBridge (2026) |
|---|---|---|
| Surface → GL binding | framework `GLSurfaceView` (Java `EglHelper`) | native `ANativeWindow_fromSurface` + `eglCreateWindowSurface` in Rust |
| GL level | ES 1.x/2.x era defaults | ES 3.2 context with 3.1/3.0/ES-3 fallback, GLSL ES 3.00 |
| Native language | C++11 (earlier demo: C++17) | Rust 2021 (cdylib), no C++ left in the app |
| Native build | `ndk-build`, `Android.mk`, `gnustl_shared` | `cargo ndk` driven by Gradle; NDK r27c, 16 KB-page aligned |
| ABIs | armeabi-v7a first, x86, arm64-v8a | arm64-v8a primary (+ armeabi-v7a, x86_64) |
| Platform | `APP_PLATFORM android-14` | `compileSdk`/`targetSdk` 35 (Android 15), `minSdk` 26 |
| Language / AndroidX | Java, pre-AndroidX | Kotlin, AndroidX (`core-ktx`, `activity-ktx`) |
| Surface lifecycle | delegated to `GLSurfaceView` | explicit state machine (`NO_SURFACE` … `STOPPED`) + bounded fences |
| Render thread | `GLSurfaceView`'s internal `GLThread` | the bridge's own thread, with a FIFO command queue |
| Frame loop | framework-driven | `Internal` (bridge draws) or `Inverted` (the game draws; `attachGameThread`/`swapBuffers`) |
| Input | LWJGL-Android port (external) | translation to real SDL values, bounded queue, IME text, gamepads |
| Graphics backends | system EGL + GLES only | GLES implemented; Vulkan a documented interface |
| Verification | manual on a phone | `cargo test` + APK build + emulator assertions in CI, plus on-device logcat greps |

---

## 5. Building and verifying

See [ANDROID.md](ANDROID.md) for the toolchain and the exact commands, and
[STATUS.md](STATUS.md) for what each CI job proves. In short: the APK is built on
CI (`./gradlew :app:assembleDebug`, which cross-compiles the Rust core for every
ABI), the `.so` is checked for 16 KB page alignment and the expected exported
symbols, and an emulator run asserts on logcat that the surface was bound, frames
were drawn, the centre pixel is the solid clear colour, and that `KEYCODE_A`
arrived as `SDL_SCANCODE_A`.

Two notes carried over from the C++ era, still true:

- **`glReadPixels` is the authoritative pixel proof**, not `screencap`: the
  readback sees the raw framebuffer, while a screenshot passes through the
  display's colour-management path and can differ by a few LSBs. This is why the
  CI assertion on the centre pixel allows 1 LSB of rounding.
- **The primary development device is an aarch64 Android phone**, where the SDK's
  x86-64 build tools cannot execute; cross-compilation on CI is what proves the
  arm64 artifact, and a manual sideload is what proves the GPU path.
