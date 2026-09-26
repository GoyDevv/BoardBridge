# GLFW compatibility

## Why it exists in a project that also plans SDL3

Minecraft ≤ 26.2 (and every mod-loader stack built for those versions) reaches
the window through **LWJGL's GLFW bindings**. Minecraft 26.3+ reaches it through
SDL3. A launcher that supports both eras needs both, and the cheaper of the two
to build *second* is GLFW: the window, the context, the input translation and the
frame-loop inversion are already implemented here, so a GLFW layer is a
translation surface, not a renderer.

```
Minecraft ≤ 26.2
  → LWJGL GLFW bindings (glfw* symbols)
  → GLFW compatibility layer              ← this document (not implemented)
  → BoardBridge (surface + input queue)   ← implemented
  → ANativeWindow / EGL
```

This module is **interface only**: `platform::glfw_compat` reports
`interface-only`, every entry point returns `ERR_BACKEND_UNAVAILABLE`, and
`getStatus()` names the remaining work. Nothing about GLFW is faked by wrapping
SDL or EGL and claiming it satisfies GLFW's contract.

## The subset that has to exist

Instrumented observation of the versions in scope (see the module header for the
same list) narrows GLFW down to:

| Area | Functions |
|---|---|
| Init/lifecycle | `glfwInit`, `glfwTerminate`, `glfwInitHint`, `glfwGetError`, `glfwSetErrorCallback` |
| Window | `glfwCreateWindow`, `glfwDestroyWindow`, `glfwWindowShouldClose`, `glfwSetWindowShouldClose`, `glfwShowWindow`, `glfwHideWindow`, `glfwFocusWindow`, `glfwSetWindowTitle` |
| Geometry | `glfwGetWindowSize`, `glfwSetWindowSize`, `glfwGetFramebufferSize`, `glfwGetWindowContentScale`, `glfwGetWindowPos`, `glfwSetWindowPos` |
| Context | `glfwMakeContextCurrent`, `glfwGetCurrentContext`, `glfwSwapBuffers`, `glfwSwapInterval`, `glfwGetProcAddress`, `glfwWindowHint`, `glfwWindowHintString`, `glfwGetWindowAttrib`, `glfwSetWindowAttrib` |
| Input | `glfwPollEvents`, `glfwWaitEvents`, `glfwWaitEventsTimeout`, `glfwPostEmptyEvent`, `glfwGetKey`, `glfwGetKeyName`, `glfwGetKeyScancode`, `glfwGetInputMode`, `glfwSetInputMode`, `glfwGetCursorPos`, `glfwSetCursorPos`, cursor/key/char/mouse-button/scroll callbacks, `glfwJoystickPresent`, `glfwGetGamepadState`, `glfwSetClipboardString`, `glfwGetClipboardString` |
| Time/monitors | `glfwGetTime`, `glfwSetTime`, `glfwGetTimerValue`, `glfwGetTimerFrequency`, primary-monitor and video-mode queries |

## How it connects to what is already implemented

| GLFW call | BoardBridge equivalent (already working) |
|---|---|
| `glfwCreateWindow` | the surface is created by Android and handed to the bridge; the bridge returns a synthetic window object carrying the binding generation |
| `glfwMakeContextCurrent` | `BridgeRuntime.attachGameThread()` (`Runtime::attach_game_thread`) |
| `glfwSwapBuffers` | `BridgeRuntime.present()` (`Runtime::present`) |
| `glfwSwapInterval` | `BridgeRuntime.setSwapInterval()` |
| `glfwGetFramebufferSize` | `getStatus()`'s surface size / the backend's `window_size()` |
| `glfwPollEvents` | drain the bridge's event queue and dispatch GLFW callbacks on the calling thread |
| `glfwGetKey`, `glfwGetCursorPos` | derived from the same translated events (`InputEvent::Key`, `MouseMotion`) |
| `GLFW_CURSOR_DISABLED` | the captured-pointer path, which already delivers `relative = true` motion |
| `glfwSetCharCallback` | `InputEvent::Text` (IME-committed text and synthesized key characters) |
| `glfwGetGamepadState` | `InputEvent::GamepadAxis` / `GamepadButton` |

The frame-loop inversion was built for exactly this: `attachGameThread` and
`swapBuffers` exist so a game that expects `glfwMakeContextCurrent` +
`glfwSwapBuffers` works without the bridge owning the loop.

## Exact remaining work

1. **A `.so` that exports those symbols.** Two options:
   - **(recommended) Upstream GLFW + a custom platform backend.** Build real
     GLFW for Android and implement only its `_glfwPlatform*` layer against this
     bridge (window = the bridge's binding, input = the bridge's queue, time =
     `clock_gettime`). Upstream keeps the window/context bookkeeping, hints,
     attributes and error strings correct — more code reused, less invented.
   - **A from-scratch shim** exposing exactly the subset above. Smaller, but it
     must re-implement GLFW semantics (hints, attributes, error codes) precisely
     enough that Minecraft's init checks pass.
2. **Callback dispatch.** GLFW is callback-based (`glfwSetKeyCallback`,
   `glfwSetCursorPosCallback`, …) while the bridge is queue-based: the layer must
   convert queued `InputEvent`s into callbacks **on the game thread** inside
   `glfwPollEvents`/`glfwWaitEvents`, never from the bridge thread.
3. **Per-window input state.** `glfwGetKey`/`glfwGetMouseButton`/
   `glfwGetCursorPos` need GLFW's own state, updated from the same translated
   events. Android has no global cursor position, so `glfwSetCursorPos` needs a
   virtual cursor (accumulated deltas) and must say so in its documentation.
4. **Cursor modes.** `GLFW_CURSOR_DISABLED` → `requestPointerCapture()` +
   relative motion; `GLFW_CURSOR_NORMAL` → `releasePointerCapture()`.
   `GLFW_CURSOR_HIDDEN` has no Android equivalent (the launcher's UI decides).
5. **Instrumentation.** Every entry point that is not implemented must log once
   with its name and arguments. Silent no-ops are how "the game ignores my
   keyboard" bug reports happen; this is a requirement of the migration, not a
   nicety.
6. **LWJGL glue.** LWJGL loads the module's native library by name
   (`org.lwjgl.glfw.libname`, default `glfw`), so the shim must be present in the
   APK's `lib/<abi>/` as `libglfw.so` (or pointed at explicitly). See
   [LWJGL.md](LWJGL.md) — including the version-specific detail that must be
   verified rather than assumed.

## Verification plan (when it exists)

- Unit: the event→callback conversion and the input-state bookkeeping are pure
  functions; test them on the CI runner like `input::keymap` is tested today.
- Device: a tiny GLFW program (create window, attach, draw a clear, poll, log the
  scancode of `KEYCODE_A`) run through the same emulator workflow as
  [ANDROID.md](ANDROID.md) describes, asserting the log line
  `key DOWN code=29 scancode=A` still appears — the translation is shared, so it
  is already proven for the GLFW path too.
