# BoardBridge

![Build](https://github.com/GoyDevv/BoardBridge/actions/workflows/build.yml/badge.svg)

A modern, native **EGL / Surface bridge** for Android: it binds an Android
`Surface` to an **OpenGL ES 3.2** context in C++, drives it from a dedicated
render thread, and handles the full surface lifecycle and input dispatch.

BoardBridge modernizes the Surface-to-GL approach of
**[Boardwalk](https://github.com/zhuowei/Boardwalk)** by **zhuowei** (Apache-2.0).
It targets current Android (**Android 15 / API 35**) and Mali-G52 / Helio G85
class devices.

> **Note on Boardwalk:** the original (archived) Boardwalk did not hand-write an
> EGL bridge — it subclassed the framework `GLSurfaceView` and let the framework
> bind the `Surface` and manage EGL, while its native code did Dalvik VM tweaks.
> BoardBridge takes that *GLSurfaceView pattern* as the conceptual seed and
> rebuilds it natively. See [`docs/DESIGN.md`](docs/DESIGN.md) for the full
> analysis and the modernization table.

## What it does

- **Surface → EGLSurface binding** in native code via `ANativeWindow_fromSurface`
  + `eglCreateWindowSurface`, with an ES 3.2 context (3.1 / 3.0 fallback).
- **Thread-safe surface lifecycle**: `surfaceCreated / Changed / Destroyed` are
  marshaled to a dedicated render thread; `surfaceDestroyed` blocks until the
  native side releases the window, preventing use-after-free.
- **Input dispatch**: touch (per-pointer) and key events flow through a bounded
  native queue and are consumed each frame.
- **Live demo**: a spinning GLES 3.00 shader triangle over an animated clear
  color — touch steers the color, key presses shift the hue.

## Architecture

```
MainActivity ─ BridgeSurfaceView (SurfaceView + SurfaceHolder.Callback + input)
        │  JNI (NativeBridge)
        ▼
native_bridge.cpp ─▶ RenderThread ─▶ EglCore (EGL + GLES 3.2)
                          └─ InputQueue
```

- Kotlin: `app/src/main/java/com/boardbridge/egl/`
- Native C++: `app/src/main/cpp/` (`native_bridge`, `render_thread`, `egl_core`,
  `input_queue`)

## Requirements

| | |
|---|---|
| compileSdk / targetSdk | 35 (Android 15) |
| minSdk | 26 |
| NDK | r27c (`27.2.12479018`) — 16 KB page aligned |
| CMake | 3.22.1 |
| ABIs | `arm64-v8a` (primary), `armeabi-v7a`, `x86_64` |
| GPU | OpenGL ES 3.2 (e.g. Mali-G52) |

## Building

The canonical build uses the **Gradle wrapper** (no Android Studio required):

```bash
./gradlew :app:assembleDebug
# output: app/build/outputs/apk/debug/app-debug.apk
```

### Why CI builds this

This project is developed on an aarch64 Android phone (in a proot Linux
environment). The Android SDK build-tools available there are **x86-64** binaries
that cannot execute on aarch64, so a local `./gradlew` build is not possible on
the device. The build therefore runs on **GitHub Actions** (an `ubuntu-latest`
runner), which installs the NDK + CMake and runs the exact same
`./gradlew :app:assembleDebug`. See
[`.github/workflows/build.yml`](.github/workflows/build.yml). The built
`app-debug.apk` is uploaded as a workflow artifact.

To build locally on a normal x86-64 Linux/macOS/Windows machine, install the
Android SDK (API 35), accept licenses, point `ANDROID_HOME` at it (or add a
`local.properties` with `sdk.dir=...`), and run the command above; Gradle/AGP
will fetch NDK r27c and CMake 3.22.1 as needed.

## Credits & license

- **Boardwalk** by [zhuowei](https://github.com/zhuowei) — Apache-2.0. The
  conceptual origin of the Surface/GL bridge modernized here.
- **The Android Open Source Project** — Apache-2.0 (original Boardwalk headers).

BoardBridge is licensed under the **Apache License, Version 2.0**. See
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
