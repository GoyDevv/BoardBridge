# BoardBridge — Design & Lineage

BoardBridge is a small, self-contained Android app that binds an Android
`Surface` to an OpenGL ES 3.2 context **in native code** and drives it from a
dedicated render thread, with full surface-lifecycle handling and input
dispatch. It modernizes the Surface-to-GL approach used by
[Boardwalk](https://github.com/zhuowei/Boardwalk) (by zhuowei, Apache-2.0) and
uses [ZalithLauncher2](https://github.com/ZalithLauncher/ZalithLauncher2)
(GPL-3.0) purely as a *conceptual* reference.

---

## 1. What Boardwalk actually did (the honest starting point)

Reading the archived Boardwalk sources shows that the original project did **not**
hand-write an EGL/Surface bridge:

- **`BoardwalkGLSurfaceView.java`** is a thin subclass of the framework
  `android.opengl.GLSurfaceView`. The framework class is what actually performs
  the `Surface -> EGLSurface` binding (internally via its `EglHelper`, using
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
conceptual seed BoardBridge takes and rebuilds natively.

---

## 2. ZalithLauncher2 — its approach in plain English (conceptual reference only)

> No ZalithLauncher2 source was copied into this project. The following is a
> high-level, plain-English description of how launchers in this lineage
> (Boardwalk → PojavLauncher → FoldCraftLauncher → ZalithLauncher) structure
> their rendering bridge, written from general knowledge of the architecture.

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
  control-mapping editor. ZalithLauncher2 specifically pairs this with a modern
  Kotlin + Jetpack Compose UI, Android 15 support, and runtime selection of the
  Java runtime and renderer.

**What BoardBridge borrows conceptually:** the “hand the raw Surface to native,
own EGL there, run a dedicated render thread, and marshal a thread-safe surface
lifecycle plus an input queue” shape. BoardBridge implements only the core
bridge (system EGL + GLES 3.2) — not translators, virtual controls, or a game
runtime — and does so as original Apache-2.0 code.

---

## 3. BoardBridge architecture

```
              Java / Kotlin                     |            Native C++ (libboardbridge.so)
------------------------------------------------+-----------------------------------------------------
 MainActivity (ComponentActivity)               |
   └─ BridgeSurfaceView : SurfaceView,          |
        SurfaceHolder.Callback                  |
        • surfaceCreated  ─┐                     |
        • surfaceChanged   │  JNI  NativeBridge  |   native_bridge.cpp (JNI entry points)
        • surfaceDestroyed ├────────────────────▶   • ANativeWindow_fromSurface(surface)
        • onTouchEvent     │                     |   • forwards to a single global RenderThread
        • onKeyDown/Up    ─┘                     |
                                                 |   RenderThread (dedicated std::thread)
                                                 |     • mutex + condition_variable
                                                 |     • thread-safe window handoff
                                                 |     • SYNCHRONOUS clearWindow()
                                                 |     • drains InputQueue each frame
                                                 |          │
                                                 |          ▼
                                                 |   EglCore   • eglInitialize / chooseConfig(ES3)
                                                 |             • eglCreateContext (3.2→3.1→3.0)
                                                 |             • eglCreateWindowSurface(window)
                                                 |             • makeCurrent / swapBuffers
                                                 |   InputQueue • bounded, mutex-protected deque
```

### Threading & the surface lifecycle race
The classic Android pitfall is using an `ANativeWindow` after `surfaceDestroyed`
returns. BoardBridge avoids it by making `clearWindow()` **block** the UI thread
inside `surfaceDestroyed` until the render thread has destroyed the `EGLSurface`
and called `ANativeWindow_release`. Window ownership is a single transferred
reference: `ANativeWindow_fromSurface` gives one ref to JNI, JNI transfers it to
`RenderThread`, and the render thread releases it when the window is replaced,
cleared, or the thread stops.

### Input dispatch
`BridgeSurfaceView` forwards touch (per-pointer for MOVE batches) and key events
through JNI into a bounded `InputQueue`. The render thread drains the queue once
per frame. In this demo, touch position steers the animated clear color and key
presses shift the palette hue — proving the whole input path end-to-end. The
BACK key is deliberately passed through to the system so the user is never
trapped.

### The render demo
After binding the surface the render thread compiles a minimal GLES 3.00 shader
program and draws a spinning colored triangle over an animated clear color. If
shader compilation fails on some driver, it gracefully falls back to the
animated clear alone — either way the `Surface → EGLSurface` binding, context,
and swap chain are exercised.

---

## 4. Modernization: Boardwalk (2015-2020) → BoardBridge (2026)

| Concern            | Boardwalk                                   | BoardBridge                                                   |
|--------------------|---------------------------------------------|--------------------------------------------------------------|
| Surface→GL binding | Framework `GLSurfaceView` (Java `EglHelper`)| Native `ANativeWindow_fromSurface` + `eglCreateWindowSurface`|
| GL level           | ES 1.x/2.x era defaults                     | OpenGL ES 3.2 context (3.1/3.0 fallback), GLSL ES 3.00       |
| Native build       | `ndk-build`, `Android.mk`, `gnustl_shared`, C++11 | CMake `externalNativeBuild`, libc++, C++17               |
| ABIs               | armeabi-v7a first, x86, arm64-v8a           | arm64-v8a primary (+ armeabi-v7a, x86_64), NDK r27c 16 KB pages |
| Platform           | `APP_PLATFORM android-14`                   | `compileSdk`/`targetSdk` 35 (Android 15), `minSdk` 26        |
| Language / AndroidX| Java, pre-AndroidX                          | Kotlin, AndroidX (`core-ktx`, `activity-ktx`)                |
| Surface lifecycle  | Delegated to `GLSurfaceView`                | Explicit thread-safe handoff (mutex/condvar, synchronous destroy) |
| Render thread      | LWJGL/GLSurfaceView internal `GLThread`     | Dedicated `std::thread` state machine we own                 |
| Input              | LWJGL-Android port (external)               | Native bounded `InputQueue`, per-pointer touch + keys        |

---

## 5. Building

The native pipeline requires an Android SDK/NDK that can run on the build host.
See the repository `README.md` for details; in short, the canonical build is
`./gradlew :app:assembleDebug`, executed on CI (GitHub Actions, ubuntu runner)
because the primary development device here is an aarch64 Android phone whose
downloaded SDK build-tools are x86-64 and cannot execute locally.
