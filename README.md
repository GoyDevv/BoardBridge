# BoardBridge

![Build](https://github.com/GoyDevv/BoardBridge/actions/workflows/build.yml/badge.svg)

The native bridge/runtime a **Minecraft: Java Edition launcher for Android** is
built on: it owns the Android `Surface`, EGL, the frame loop and input
translation, so that a JVM game can render into a real Android window.

```
Kotlin shell  →  JNI  →  Rust core (libboardbridge.so)  →  EGL  →  ANativeWindow
```

BoardBridge modernizes the Surface-to-GL approach of
**[Boardwalk](https://github.com/zhuowei/Boardwalk)** by **zhuowei** (Apache-2.0):
Boardwalk subclassed the framework `GLSurfaceView` and let the framework bind the
`Surface` and own EGL. BoardBridge takes that *idea* and owns the whole path in
native code — `ANativeWindow_fromSurface`, the `EGLDisplay`/context/`EGLSurface`,
the loop, the fences and the input queue. See
[`docs/DESIGN.md`](docs/DESIGN.md) for the lineage analysis and
[`docs/MIGRATION.md`](docs/MIGRATION.md) for the C++ → Rust history.

## What works today

| Area | Status |
|---|---|
| Surface lifecycle (`Surface` → `ANativeWindow` ownership, rotation, pause/resume, bounded destroy fence) | **implemented** |
| EGL in Rust (display, config, ES 3.2 → 3.0 context chain, window surface, `eglSwapInterval`) | **implemented** |
| Frame-loop inversion (`attachGameThread` / `swapBuffers`, no sleep-and-hope) | **implemented** |
| Input translation to **real SDL3 values** (scancodes, keycodes, `SDL_KMOD_*`, gamepad axes/buttons, `SDL_BUTTON_*`), generated from SDL3 + AOSP headers | **implemented** |
| Mouse (absolute + captured/relative), touch batches, IME text, gamepads | **implemented** |
| Diagnostics: solid/triangle renderer, centre-pixel readback, `getStatus()`, `runSelfTest()` | **implemented** |
| Graphics backends: OpenGL ES via EGL | **implemented** |
| Graphics backends: Vulkan | **interface only** ([docs/GRAPHICS.md](docs/GRAPHICS.md)) |
| Platform backend: Android native (Kotlin input) | **implemented** |
| Platform backend: SDL3 (Minecraft 26.3+) | **interface only** ([docs/SDL3.md](docs/SDL3.md)) |
| Platform backend: GLFW compatibility (Minecraft ≤ 26.2) | **interface only** ([docs/GLFW_COMPAT.md](docs/GLFW_COMPAT.md)) |
| LWJGL native glue | **not started** ([docs/LWJGL.md](docs/LWJGL.md)) |
| Launcher features (downloads, auth, JVM, instances, UI) | out of scope here — a separate `launcher-core` on top of this bridge |

Nothing here is faked: a backend that is not implemented reports
`interface-only` with the exact remaining work in `getStatus()` and logcat, and
fails with `ERR_BACKEND_UNAVAILABLE` instead of silently succeeding. The full
ledger is [`docs/STATUS.md`](docs/STATUS.md).

## Layout

```
app/                     Kotlin shell (Android app)
  src/main/java/com/boardbridge/bridge/
    NativeBridge.kt      one external fun per exported Rust symbol (ABI v2)
    BridgeRuntime.kt     process-wide runtime facade, logs refused calls
    BridgeSurfaceView.kt SurfaceHolder callbacks + input translation
    MainActivity.kt      demo/verification activity
boardbridge/             Rust core (cdylib + rlib)
  src/lifecycle/         surface/pause state machine (pure, unit-tested)
  src/input/             SDL-shaped events, bounded queue, keymap, generated SDL tables
  src/android/           ANativeWindow ownership, JNI → event adapters
  src/egl/               EGL display/context/surface in Rust
  src/graphics/          backend trait, GLES implementation, Vulkan interface
  src/render/            the bridge's own diagnostic renderer
  src/platform/          Android native (implemented), SDL3 / GLFW (interfaces)
  src/runtime/           config, bridge thread, command queue, registry
  src/jni/               exported entry points
tools/generate_sdl_tables.py   regenerates src/input/sdl_tables.rs from SDL3 + AOSP headers
docs/                    ARCHITECTURE, THREADING, ANDROID, SDL3, GLFW_COMPAT, LWJGL, GRAPHICS, MIGRATION, STATUS, DESIGN
```

## Building

Requires: JDK 17, Android SDK (API 35), NDK `27.2.12479018`, Rust (1.75+) with
the Android targets, and `cargo-ndk`:

```bash
cargo install cargo-ndk --locked
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

./gradlew :app:assembleDebug                       # Kotlin + Rust, all ABIs
./gradlew :app:assembleDebug -PcargoProfile=debug  # faster Rust profile
./gradlew :app:assembleDebug -PskipRust            # Kotlin only (uses an existing .so)
```

Gradle runs `cargo ndk` itself (see `app/build.gradle.kts`), which writes
`app/build/rustJniLibs/<abi>/libboardbridge.so`; AGP packages that directory as
`jniLibs`. Gradle finds the NDK through `ANDROID_NDK_HOME`, then the `sdk.dir` in
`local.properties`. Details, including the 16 KB page requirement:
[`docs/ANDROID.md`](docs/ANDROID.md).

Host-side checks (no Android toolchain needed — the pure modules are portable):

```bash
cd boardbridge
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cd .. && sh tools/host_check_android.sh
```

A plain host build skips everything behind `cfg(target_os = "android")` — the
GLES backend, the EGL display/context/surface code and the JNI-adjacent modules
— so a mistake in them would first appear when the APK job cross-compiles.
`tools/host_check_android.sh` closes that gap cheaply: it copies the crate to a
scratch directory, removes the `cfg` gates and typechecks *and lints* the Android
half against the host target (nothing is linked, so no NDK is needed). The real
Android build is still the `apk` job.

### Why CI builds this

This project is developed on an aarch64 Android phone (inside a proot Linux
environment), where the Android SDK's x86-64 build tools cannot execute. The APK
is therefore built on **GitHub Actions**: the `rust` job runs the host checks,
and the `apk` job installs the NDK + `cargo-ndk` and runs the same
`./gradlew :app:assembleDebug`, then verifies the `.so` is 16 KB aligned before
uploading the APK. A second workflow boots an emulator, installs the APK and
asserts on logcat that the surface was bound, frames were drawn, the centre pixel
is the diagnostic colour, and that `KEYCODE_A` arrived as `SDL_SCANCODE_A`
([`.github/workflows/render-test.yml`](.github/workflows/render-test.yml)).

## Documentation

| Document | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | layers, module map, ownership rules, JNI contract, the two loops |
| [docs/THREADING.md](docs/THREADING.md) | threads, the lifecycle state machine, fences, rotation epochs, why no sleeps |
| [docs/ANDROID.md](docs/ANDROID.md) | toolchain, ABIs, 16 KB pages, build commands, logcat greps |
| [docs/GRAPHICS.md](docs/GRAPHICS.md) | GLES backend internals, Vulkan interface, how to add a backend |
| [docs/SDL3.md](docs/SDL3.md) | SDL3 integration plan and exact remaining work |
| [docs/GLFW_COMPAT.md](docs/GLFW_COMPAT.md) | GLFW compatibility layer plan and remaining work |
| [docs/LWJGL.md](docs/LWJGL.md) | LWJGL native glue (not started) |
| [docs/MIGRATION.md](docs/MIGRATION.md) | C++ demo → Rust bridge, old→new API mapping, launcher checklist |
| [docs/STATUS.md](docs/STATUS.md) | implemented / interface-only / not started, and how each was verified |
| [docs/DESIGN.md](docs/DESIGN.md) | the Boardwalk lineage and the design principles |

## Credits & license

- **Boardwalk** by [zhuowei](https://github.com/zhuowei) — Apache-2.0. The
  conceptual origin of the Surface/GL bridge modernized here.
- **SDL3** (zlib license) — the input tables in
  `boardbridge/src/input/sdl_tables.rs` are generated from SDL3's own headers.
- **The Android Open Source Project** — Apache-2.0 (the `AKeycode`/`AMotionEvent`
  tables, and the original Boardwalk headers).

BoardBridge is licensed under the **Apache License, Version 2.0**. See
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
