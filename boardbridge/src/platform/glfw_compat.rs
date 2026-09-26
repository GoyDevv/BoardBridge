// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! GLFW compatibility backend — **interface only, not implemented**.
//!
//! Minecraft ≤ 26.2 (and every mod-loader stack built for those versions) reaches
//! the window through **LWJGL's GLFW bindings**. That path is *not* being
//! dropped: the plan is a GLFW-compatible layer that exposes the subset of GLFW
//! the game actually calls, implemented on top of this bridge.
//!
//! ```text
//! Minecraft  ≤ 26.2
//!   → LWJGL GLFW bindings (glfw* symbols)
//!   → GLFW compatibility layer            ← this module (not implemented)
//!   → BoardBridge (surface + input queue) ← implemented
//!   → ANativeWindow / EGL
//! ```
//!
//! ## The subset that has to exist
//!
//! Instrumented observation of the versions in scope narrows GLFW down to:
//!
//! | Area | Functions |
//! |---|---|
//! | Init/lifecycle | `glfwInit`, `glfwTerminate`, `glfwInitHint`, `glfwGetError`, `glfwSetErrorCallback` |
//! | Window | `glfwCreateWindow`, `glfwDestroyWindow`, `glfwWindowShouldClose`, `glfwSetWindowShouldClose`, `glfwShowWindow`, `glfwHideWindow`, `glfwFocusWindow`, `glfwSetWindowTitle` |
//! | Geometry | `glfwGetWindowSize`, `glfwSetWindowSize`, `glfwGetFramebufferSize`, `glfwGetWindowContentScale`, `glfwGetWindowPos`, `glfwSetWindowPos` |
//! | Context | `glfwMakeContextCurrent`, `glfwGetCurrentContext`, `glfwSwapBuffers`, `glfwSwapInterval`, `glfwGetProcAddress`, `glfwWindowHint`, `glfwWindowHintString`, `glfwGetWindowAttrib`, `glfwSetWindowAttrib` |
//! | Input | `glfwPollEvents`, `glfwWaitEvents`, `glfwWaitEventsTimeout`, `glfwPostEmptyEvent`, `glfwGetKey`, `glfwGetKeyName`, `glfwGetKeyScancode`, `glfwGetInputMode`, `glfwSetInputMode`, `glfwGetCursorPos`, `glfwSetCursorPos`, `glfwSetCursorPosCallback`, `glfwSetKeyCallback`, `glfwSetCharCallback`, `glfwSetMouseButtonCallback`, `glfwSetScrollCallback`, `glfwSetCursorEnterCallback`, `glfwJoystickPresent`, `glfwGetGamepadState`, `glfwSetClipboardString`, `glfwGetClipboardString` |
//! | Time/monitors | `glfwGetTime`, `glfwSetTime`, `glfwGetTimerValue`, `glfwGetTimerFrequency`, `glfwGetPrimaryMonitor`, `glfwGetVideoMode`, `glfwGetMonitors`, `glfwGetMonitorPos`, `glfwGetMonitorContentScale`, `glfwGetMonitorName`, `glfwSetMonitorCallback` |
//!
//! ## Exact remaining work
//!
//! 1. **A `.so` that exports those symbols.** LWJGL loads GLFW from the native
//!    library paths it looks up on Android; a `libglfw.so` (name per LWJGL's
//!    expectations) built from this layer is what Minecraft binds to. Alternative
//!    (fewer symbols): build *real* GLFW for Android and implement only its
//!    platform backend (`_glfwPlatform*`) against this bridge. The second option
//!    reuses upstream's window/context bookkeeping and is the recommended one.
//! 2. **Callback dispatch.** GLFW is callback-based, SDL is poll-based: the layer
//!    must convert [`InputEvent`]s from the bridge queue into
//!    `glfwSet*Callback` invocations on the game thread inside `glfwPollEvents`
//!    (thread-affine, never from the bridge thread).
//! 3. **Per-window input state.** `glfwGetKey`/`glfwGetMouseButton`/
//!    `glfwGetCursorPos` require GLFW to keep its own state; that state must be
//!    updated from the same translated events (scancodes come from
//!    [`crate::input::keymap`]).
//! 4. **Cursor/relative mode.** `GLFW_CURSOR_DISABLED` (raw mouse) maps onto the
//!    bridge's relative mouse motion; `glfwSetCursorPos` needs a virtual cursor
//!    that Android does not have.
//! 5. **Instrumentation.** Every unsupported entry point must log once with its
//!    name and argument summary — silent no-ops are how launchers end up with
//!    "the game ignores my keyboard" bug reports. This is a requirement of the
//!    migration, not a nicety.
//! 6. **LWJGL glue.** `liblwjgl_glfw.so`-equivalent loading on Android, see
//!    `docs/LWJGL.md`.
//!
//! [`InputEvent`]: crate::input::InputEvent

use crate::error::{Error, Result};
use crate::graphics::BackendStatus;
use crate::input::{InputEvent, QueueStats};
use crate::platform::{PlatformBackend, PlatformKind, WindowInfo};
use crate::runtime::control::Shared;

/// One-line summary of the remaining work, quoted in diagnostics.
pub const GLFW_REMAINING_WORK: &str = "GLFW compatibility: build the glfw* shim (or upstream GLFW's Android platform backend) on top of the bridge queue (see docs/GLFW_COMPAT.md)";

/// GLFW compatibility backend placeholder.
pub struct GlfwCompatPlatformBackend;

impl PlatformBackend for GlfwCompatPlatformBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::GlfwCompat
    }

    fn name(&self) -> &'static str {
        "glfw-compat"
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::InterfaceOnly {
            remaining: GLFW_REMAINING_WORK,
        }
    }

    fn remaining_work(&self) -> Option<&'static str> {
        Some(GLFW_REMAINING_WORK)
    }

    fn api_surface(&self) -> &'static str {
        "GLFW 3 subset (window, context, input callbacks, gamepads) for Minecraft ≤ 26.2"
    }

    fn window_info(&self, _shared: &Shared) -> Result<WindowInfo> {
        Err(Error::BackendUnavailable(GLFW_REMAINING_WORK))
    }

    fn queue_stats(&self, _shared: &Shared) -> Result<QueueStats> {
        Err(Error::BackendUnavailable(GLFW_REMAINING_WORK))
    }

    fn poll_event(&self, _shared: &Shared) -> Result<Option<InputEvent>> {
        Err(Error::BackendUnavailable(GLFW_REMAINING_WORK))
    }

    fn push_event(&self, _shared: &Shared, _event: InputEvent) -> Result<bool> {
        Err(Error::BackendUnavailable(GLFW_REMAINING_WORK))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glfw_compat_is_interface_only_and_names_the_shim() {
        let backend = GlfwCompatPlatformBackend;
        assert_eq!(backend.kind(), PlatformKind::GlfwCompat);
        assert!(!backend.status().is_implemented());
        assert!(backend.remaining_work().unwrap().contains("glfw"));
        assert!(backend.api_surface().contains("GLFW"));
    }
}
