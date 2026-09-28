# Android: toolchain, build and device notes

## Targets

| | |
|---|---|
| compileSdk / targetSdk | 35 (Android 15) |
| minSdk | 26 (Android 8.0) |
| NDK | r27c — `27.2.12479018`, via AGP |
| Rust | 1.75+ (edition 2021), `cargo-ndk` |
| ABIs | `arm64-v8a` (primary), `armeabi-v7a`, `x86_64` |
| Reference hardware | Mali-G52 / Helio G85 class, OpenGL ES 3.2 |

`arm64-v8a` is the primary target: it is what the reference device and every
current phone uses, and it is the ABI the CI render test checks for 16 KB
alignment. `armeabi-v7a` and `x86_64` exist so old devices and emulators work;
they are built by the same command, so they cannot silently rot.

## Building

```bash
# once
cargo install cargo-ndk --locked
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# every time
./gradlew :app:assembleDebug
./gradlew :app:assembleRelease
```

Gradle drives the Rust build itself (`cargoBuildBoardBridge` in
`app/build.gradle.kts`), so a single Gradle command produces an APK containing
the Rust `.so` for every ABI. Escape hatches:

| Flag | Effect |
|---|---|
| `-PcargoProfile=debug` | faster, unoptimized Rust (`panic = unwind`, so the JNI panic guard is what protects the JVM) |
| `-PcargoProfile=release` | default; LTO, `codegen-units = 1`, `panic = "abort"`, `strip = debuginfo` |
| `-PskipRust` | skip the Rust build entirely and use whatever is already in `app/build/rustJniLibs` |

The Kotlin/Rust boundary is the *only* cross-language build dependency: there is
no CMake, no `externalNativeBuild`, and nothing in the app generates headers.
`cargo ndk -o app/build/rustJniLibs` writes `libboardbridge.so` per ABI, and AGP
picks that directory up as `jniLibs`.

### How Gradle finds the NDK

`cargoBuildBoardBridge` resolves the NDK in this order:

1. `$ANDROID_NDK_HOME`
2. `$ANDROID_NDK_ROOT`
3. `$ANDROID_SDK_ROOT`/`$ANDROID_HOME` + `ndk/<android.ndkVersion>`
4. `sdk.dir` from `local.properties` + `ndk/<android.ndkVersion>`

If none resolves, the task fails with a message naming all four options instead
of a linker error — and `-PskipRust` is still available.

### 16 KB page alignment

Android 15 can run with 16 KB memory pages, and a shared library whose `LOAD`
segments are aligned to 4 KB will not load on such a device. Two things enforce
this here:

- `android.ndkVersion = "27.2.12479018"` (r27c defaults to 16 KB alignment for
  64-bit targets);
- `RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"` in the Gradle task, so the
  requirement does not silently depend on the NDK's default;
- `packaging { jniLibs { useLegacyPackaging = false } }`, so the `.so` stays
  uncompressed and page-aligned inside the APK.

CI verifies it on the built artifact by reading the `LOAD` segment alignments of
`libboardbridge.so` and failing if any is below 16 KB
(`.github/workflows/build.yml`).

## Where things live

```
app/src/main/java/com/boardbridge/bridge/   Kotlin shell (package com.boardbridge.bridge)
app/build/rustJniLibs/<abi>/libboardbridge.so   cargo-ndk output (gitignored)
boardbridge/src/                            Rust core
tools/generate_sdl_tables.py                regenerates the SDL3/AOSP tables
```

The package name matters: exported symbols are
`Java_com_boardbridge_bridge_NativeBridge_*`, which is fixed by the Kotlin
package and object name. Renaming either side breaks the link with a
`NoSuchMethodError`/`UnsatisfiedLinkError` at first call — the ABI version in
`getStatus()` exists to make that obvious.

## Checking the Android-only code without an NDK

`cargo test` / `cargo clippy` on a host compile only the portable modules: the
GLES backend, the EGL display/context/surface code, `android/*`, `platform/*` and
`render/diagnostics.rs` all sit behind `cfg(target_os = "android")` and are never
built, so nothing there is even parsed. Mistaking an `impl` block, forgetting an
import or a wrong EGL call therefore used to surface only when the APK job
cross-compiled — a full CI round trip for a one-line mistake.

```bash
sh tools/host_check_android.sh          # HOST_CHECK_DIR=/tmp/... to relocate
```

The script copies `boardbridge/` to a scratch directory, strips every
`cfg(target_os = "android")` gate (and neutralises the `cfg(not(...))` host
halves so they cannot collide), drops the `jni` module — its crate needs a C
compiler of its own — and runs `cargo check` and `cargo clippy -D warnings`
there. Nothing is linked and no NDK symbol is resolved: this catches type and lint
errors in Android-only code, **not** ABI mistakes. CI runs it in the `rust` job,
and the cross-compiled build remains the authority.

## Verifying on a device

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb logcat -c
adb shell am start -n com.boardbridge.bridge/.MainActivity
adb logcat -s BoardBridge:V
```

The demo runs the internal loop, so within a second or two logcat shows:

```
runtime created (renderer=auto loop=internal diag=SOLID vsync=1 fps_cap=0 preserve=true ...)
surface created: 1080x2340 queued for binding (epoch 1)
ANativeWindow acquired, EGL surface bound (generation 1, 1080x2340)
First frame rendered (1080x2340, mode=SOLID)
frames=61 fps=59.8 mode=SOLID center_pixel_RGBA=(0,158,166,255)
renderer: GL_VENDOR=ARM | GL_RENDERER=Mali-G52 | GL_VERSION=OpenGL ES 3.2
```

What each line proves:

| Line | Proves |
|---|---|
| `ANativeWindow acquired, EGL surface bound` | `ANativeWindow_fromSurface` + `eglCreateWindowSurface` worked |
| `First frame rendered` | the loop drew and presented at least one frame |
| `center_pixel_RGBA=(0,158,166,255)` | the pixels really are the clear colour (a `glReadPixels` readback, not an assumption) |
| `GL_VENDOR=…` (plus `GL_RENDERER` / `GL_VERSION`) | the GL strings crossed JNI, so `getRendererInfo()` works end to end |

Interaction checks (all observable in logcat):

```bash
adb shell input tap 1200 540     # → "touch DOWN at (…)" and a mode toggle
adb shell input keyevent KEYCODE_A
                                 # → "key DOWN code=29 scancode=A ..."
adb shell input keyevent KEYCODE_HOME && adb shell am start -n com.boardbridge.bridge/.MainActivity
                                 # → a second "surface created:" line
```

`key DOWN code=29 scancode=A` is the important one: `29` is the Android keycode
and `A` is the SDL scancode the bridge translated it into. If the game were ever
to receive `29`, the translation layer is broken — that is exactly what the CI
render test asserts against.

Other diagnostics:

```bash
adb shell am start -n com.boardbridge.bridge/.MainActivity
# getStatus()/runSelfTest() are logged by MainActivity; from Kotlin call
# BridgeRuntime.status() / .selfTest().
adb shell dumpsys SurfaceFlinger --list | grep boardbridge
```

## Status codes

Kotlin sees `0` or a negative code; `NativeBridge.codeName(code)` names it, and
`BridgeRuntime.describe(code)` adds the lifecycle state. The full list is in
`boardbridge/src/error.rs`:

| Code | Name | Usual cause |
|---|---|---|
| −1 | `ERR_NOT_INITIALIZED` | a lifecycle callback arrived after `destroyRuntime()` |
| −2 | `ERR_ALREADY_INITIALIZED` | `createRuntime` called twice |
| −3 | `ERR_INVALID_STATE` | `attachGameThread` before a surface exists, or a state the machine refused |
| −4 | `ERR_NO_SURFACE` | `swapBuffers` with nothing bound |
| −5 | `ERR_SURFACE_BUSY` | another thread owns the EGL context |
| −6 | `ERR_SURFACE_REVOKED` | the game presented after the surface was destroyed; stop presenting and re-attach after the next `surfaceCreated` |
| −7 | `ERR_GRAPHICS` | an EGL/GL call failed; the log line carries the operation and `EGL_*` code |
| −8 | `ERR_BACKEND_UNAVAILABLE` | a backend that is interface-only was selected (Vulkan, SDL3, GLFW) |
| −9 | `ERR_INVALID_ARGUMENT` | malformed JNI argument (bad array length, unreadable string) |
| −10 | `ERR_MESSAGE` | anything else, with the message in logcat |

## Notes for a launcher

- Start the runtime **before** the surface exists; `NO_SURFACE` is a valid
  starting state and `surfaceCreated` arrives later.
- Keep `preserve_context = true` unless you have a reason not to: the game's
  textures/shaders/VAOs then survive rotation and backgrounding, and only the
  window surface is recreated.
- Do not call `attachGameThread` from the UI thread. It must be the thread that
  will own the game's GL work (Minecraft's main thread).
- The bridge does not spawn or manage the JVM. `libjvm.so`, `java.home`,
  `LD_LIBRARY_PATH` and the Minecraft classpath are the launcher's business;
  this repository stops at the surface/EGL/input boundary on purpose.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `UnsatisfiedLinkError: dlopen failed: library "libboardbridge.so" not found` | `-PskipRust` with an empty `app/build/rustJniLibs`, or an ABI filter that excludes the device's ABI |
| `UnsatisfiedLinkError: No implementation found for …NativeBridge.createRuntime` | Kotlin package/object renamed, or the `.so` came from an older ABI version (`getStatus()` prints the native one) |
| `No Android NDK found for the Rust build` | see “How Gradle finds the NDK” above |
| `error: linker 'cc' not found` during `cargo ndk` | `ANDROID_NDK_HOME` points at a non-NDK directory |
| `ERR_GRAPHICS (eglCreateContext failed code 0x3005)` | the device does not offer the requested ES 3.x config; the backend falls back ES 3.2 → 3.1 → 3.0 and logs each attempt |
