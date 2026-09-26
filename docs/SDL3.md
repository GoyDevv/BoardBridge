# SDL3 integration

## Why this matters now

Minecraft: Java Edition 26.3 (snapshot line) replaced GLFW with **SDL3** as its
platform layer, still reached through LWJGL's native bindings; LWJGL ships
bindings for both GLFW and SDL. A launcher that only supports the GLFW path
therefore loses every newer Minecraft version, and one that only supports SDL3
loses everything up to 26.2. BoardBridge keeps **both** seams
(`platform::sdl3`, `platform::glfw_compat`) precisely so that this fork in the
road does not become a fork in the codebase.

## What is already done (and not faked)

`platform::sdl3` is **interface only**, but the hard, non-obvious half of the
work is finished and shared with the implemented Android backend:

- **Real SDL event shapes.** `input::event::InputEvent` is modelled like SDL, not
  like Android: `Key { scancode, keycode, modifiers, repeat, device }`,
  `MouseMotion { x, y, relative, buttons }`, `MouseWheel`, `GamepadAxis`,
  `GamepadButton`, `Text`, plus `Lifecycle(LifecycleNotice)`.
- **Real SDL values.** `input::keymap` + the generated tables in
  `input/sdl_tables.rs` (363 constants, built by
  `tools/generate_sdl_tables.py` from SDL3's `SDL_scancode.h`,
  `SDL_keycode.h`, `SDL_keymod.h`, `SDL_gamepad.h` and AOSP's `Keycodes.h` /
  `AMotionEvent` axis constants) translate Android input into:

  | Android | SDL |
  |---|---|
  | `AKEYCODE_*` (29 = A) | `SDL_Scancode` (`SDL_SCANCODE_A` = 4) |
  | `getUnicodeChar(metaState)` + layout | `SDL_Keycode` (`SDLK_*`) |
  | `META_*` state | `SDL_KMOD_*` bits (`SDL_KMOD_LSHIFT` … `SDL_KMOD_SCROLL`) |
  | `AMOTION_EVENT_AXIS_X/Y/Z/RZ`, `LTRIGGER`, `RTRIGGER`, `HAT_X/Y` | `SDL_GAMEPAD_AXIS_LEFTX` … `RIGHT_TRIGGER` |
  | `AKEYCODE_BUTTON_*` from a gamepad | `SDL_GAMEPAD_BUTTON_*` |
  | `MotionEvent.getButtonState()` / `BUTTON_*` | `SDL_BUTTON_*` masks and button indices |

  134 of the 280 Android keycodes have no SDL equivalent and map to
  `SDL_SCANCODE_UNKNOWN` (`ANDROID_KEYCODE_UNMAPPED_COUNT` in the generated
  file) — a device key that SDL itself does not model, documented rather than
  invented.
- **A queue the SDL backend can read.** The bridge's queue is bounded (4096),
  non-blocking, drop-oldest and instrumented; `platform::PlatformBackend` already
  exposes `poll_event`/`push_event`/`window_info` over it.

## What is missing (exact remaining work)

1. **Vendor SDL3.** Build SDL3 for `arm64-v8a`, `armeabi-v7a` and `x86_64` with
   CMake/NDK and ship `libSDL3.so` in `jniLibs`, or add it as a
   `third_party/SDL` submodule with a Gradle-driven CMake target. The
   `third_party/` entry in the repository layout is reserved for this.
2. **Give SDL3 the window the bridge already owns.** SDL3 supports this
   directly, verified against `SDL_video.h`:

   ```c
   SDL_Window *SDL_CreateWindowWithProperties(SDL_PropertiesID props);
   /* SDL_video.h: "the ANativeWindow associated with the window" */
   #define SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER "SDL.window.android.window"
   /* SDL_video.h: "the EGLSurface associated with the window" */
   #define SDL_PROP_WINDOW_ANDROID_SURFACE_POINTER "SDL.window.android.surface"
   ```

   So the bridge keeps owning the window — the same `OwnedNativeWindow` the EGL
   backend binds — and SDL3 renders into it instead of creating a second,
   competing surface. This is the mechanism that makes "one window, two APIs"
   work at all, and it is the reason the SDL3 seam can be added without
   disturbing the lifecycle machine.
3. **Feed SDL's event queue from ours.** SDL3 applications call `SDL_PollEvent`.
   The backend must push translated events with `SDL_PushEvent` (from the bridge
   thread) and **not** let SDL's own Android input path run: Android input
   already reaches the bridge through Kotlin, and two producers would
   double-deliver every touch. Mapping, which is the remaining design work:

   | `InputEvent` | SDL3 event |
   |---|---|
   | `Key` | `SDL_EVENT_KEY_DOWN` / `SDL_EVENT_KEY_UP` (`scancode`, `key`, `mod`, `repeat`) |
   | `Text` | `SDL_EVENT_TEXT_INPUT` (after `SDL_StartTextInput`) |
   | `MouseMotion` | `SDL_EVENT_MOUSE_MOTION` (`x`/`y` absolute, `xrel`/`yrel` when `relative`) |
   | `MouseButton` | `SDL_EVENT_MOUSE_BUTTON_DOWN` / `_UP` |
   | `MouseWheel` | `SDL_EVENT_MOUSE_WHEEL` (float `x`/`y`) |
   | `Touch` | `SDL_EVENT_FINGER_DOWN` / `_MOTION` / `_UP` / `_CANCEL` — note SDL wants **normalised** `x`/`y` in 0..1 plus `dx`/`dy`, so the backend must divide by the surface size and track deltas |
   | `GamepadAxis` | `SDL_EVENT_GAMEPAD_AXIS_MOTION` (needs the Android device id → `SDL_JoystickID` map, i.e. real `SDL_OpenGamepad` calls) |
   | `GamepadButton` | `SDL_EVENT_GAMEPAD_BUTTON_DOWN` / `_UP` |
   | `Lifecycle` | `SDL_EVENT_WINDOW_RESIZED`, `SDL_EVENT_WINDOW_DESTROYED`, … and `SDL_EVENT_QUIT` on shutdown |
4. **Wire the lifecycle into SDL window events.** Surface loss becomes
   `SDL_EVENT_WINDOW_*`, so a game waiting in `SDL_WaitEvent` sees rotation and
   backgrounding the way it would on a desktop. `LifecycleNotice` already carries
   exactly the facts needed (`SurfaceAvailable`, `SurfaceLost`,
   `SurfaceRevoked`, `Paused`, `Resumed`).
5. **Text input, clipboard, relative mouse, gamepads.**
   `SDL_StartTextInput`/`SDL_SetClipboardText`/`SDL_SetRelativeMouseMode`/
   `SDL_OpenGamepad` must be routed to the Android facilities Kotlin already
   drives: the view's `InputConnection` (IME), `ClipboardManager`,
   `requestPointerCapture()` (the captured-pointer path already sends
   `relative = true` motion), and the gamepad tables.
6. **LWJGL glue.** LWJGL's SDL bindings load a native library and call SDL entry
   points; on Android those must resolve to the SDL3 build from (1). See
   [LWJGL.md](LWJGL.md).

## What the launcher sees meanwhile

Selecting the SDL3 backend today returns
`ERR_BACKEND_UNAVAILABLE` with the one-line reason
(`SDL3: vendor libSDL3.so per ABI and create the window with
SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER (see docs/SDL3.md)`), and `getStatus()`
reports:

```
platform_backends=[android-native=implemented | sdl3=interface-only (…) | glfw-compat=interface-only (…)]
```

There is deliberately no "SDL3 mode" that internally runs the Android path: the
Android path is *not* SDL3, and calling it SDL3 would be exactly the kind of fake
support this project refuses to ship.
