// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! SDL3 platform backend — **interface only, not implemented**.
//!
//! Minecraft Java Edition 26.3+ gets its window, input and platform integration
//! from **SDL3** (through the LWJGL SDL bindings), after years of GLFW. That
//! makes SDL3 the primary modern path for a launcher like this one, which is why
//! the seam exists here and not in a rewrite later.
//!
//! ## What *is* already done
//!
//! The hard, non-obvious half: **event translation**. [`crate::input::keymap`]
//! produces real SDL values from Android input — `SDL_Scancode` from SDL3's own
//! Android keymap, `SDL_Keycode` from the layout character Android reports,
//! `SDL_KMOD_*` modifier state, `SDL_GAMEPAD_AXIS_*`/`SDL_GAMEPAD_BUTTON_*` from
//! `AMOTION_EVENT_AXIS_*`/`AKEYCODE_BUTTON_*`, and `SDL_BUTTON_*` masks for the
//! mouse. A future SDL3 build consumes exactly these events; it does not have to
//! re-derive them.
//!
//! ## What is missing (the exact remaining work)
//!
//! 1. **Vendor SDL3.** Build SDL3 for each ABI (`arm64-v8a`, `armeabi-v7a`,
//!    `x86_64`) with CMake/NDK and ship `libSDL3.so` in `jniLibs`, or add it as a
//!    `third_party/` submodule with a Gradle-driven CMake target. This is the
//!    `third_party/` entry the repository layout reserves.
//! 2. **Give SDL3 the window we already own.** SDL3 supports exactly this:
//!    `SDL_CreateWindowWithProperties()` with the property
//!    `SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER` (`"SDL.window.android.window"`,
//!    declared in `SDL_video.h`) set to our `ANativeWindow`. The bridge therefore
//!    keeps owning the window — the same [`crate::android::surface::OwnedNativeWindow`]
//!    the EGL backend binds — and SDL3 renders into it instead of creating a
//!    second, competing surface. (`SDL_PROP_WINDOW_ANDROID_SURFACE_POINTER`, the
//!    EGLSurface property, is the alternative when an already-created
//!    `EGLSurface` should be handed over instead.)
//! 3. **Feed SDL's event queue from our queue.** SDL3 applications call
//!    `SDL_PollEvent`; the bridge must push translated events with
//!    `SDL_PushEvent` (from the bridge thread, `SDL_EVENT_*` values mapped 1:1
//!    from [`crate::input::event::InputEvent`]) rather than letting SDL's own
//!    Android input path run — Android input already reaches us through Kotlin,
//!    and two producers would double-deliver every touch.
//! 4. **Wire the lifecycle to SDL window events.** Surface loss becomes
//!    `SDL_EVENT_WINDOW_*`/`SDL_EVENT_TERMINATING` (the bridge's
//!    [`crate::input::LifecycleNotice`] values map onto them), so a game that
//!    waits in `SDL_WaitEvent` sees rotation and backgrounding the way it would
//!    on a desktop.
//! 5. **Text input, clipboard, relative mouse, gamepads.**
//!    `SDL_StartTextInput`/`SDL_SetClipboardText`/`SDL_SetRelativeMouseMode`/
//!    `SDL_OpenGamepad` must be routed to the Android equivalents Kotlin already
//!    drives (IME via `InputConnection`, clipboard via `ClipboardManager`,
//!    `requestPointerCapture`, and [`crate::input::keymap`]'s gamepad tables).
//! 6. **LWJGL glue.** LWJGL's SDL bindings load `liblwjgl_sdl.so`-style shims and
//!    call SDL entry points; on Android those must resolve to this SDL3 build.
//!    See `docs/LWJGL.md`.
//!
//! Until 1–3 are done there is no honest way to claim SDL3 support, and none is
//! claimed here: [`PlatformBackend::status`] reports
//! `interface-only`, `poll_event`/`push_event` return
//! [`crate::error::Error::BackendUnavailable`], and `getStatus` prints this list
//! in one line.

use crate::error::{Error, Result};
use crate::graphics::BackendStatus;
use crate::input::{InputEvent, QueueStats};
use crate::platform::{PlatformBackend, PlatformKind, WindowInfo};
use crate::runtime::control::Shared;

/// One-line summary of the remaining work, quoted in diagnostics.
pub const SDL3_REMAINING_WORK: &str = "SDL3: vendor libSDL3.so per ABI and create the window with SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER (see docs/SDL3.md)";

/// SDL3 platform backend placeholder.
pub struct Sdl3PlatformBackend;

impl PlatformBackend for Sdl3PlatformBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Sdl3
    }

    fn name(&self) -> &'static str {
        "sdl3"
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::InterfaceOnly {
            remaining: SDL3_REMAINING_WORK,
        }
    }

    fn remaining_work(&self) -> Option<&'static str> {
        Some(SDL3_REMAINING_WORK)
    }

    fn api_surface(&self) -> &'static str {
        "SDL3 (SDL_CreateWindowWithProperties, SDL_PushEvent, SDL_Gamepad) for Minecraft 26.3+"
    }

    fn window_info(&self, _shared: &Shared) -> Result<WindowInfo> {
        Err(Error::BackendUnavailable(SDL3_REMAINING_WORK))
    }

    fn queue_stats(&self, _shared: &Shared) -> Result<QueueStats> {
        Err(Error::BackendUnavailable(SDL3_REMAINING_WORK))
    }

    fn poll_event(&self, _shared: &Shared) -> Result<Option<InputEvent>> {
        // Deliberately an error rather than an empty poll: a caller must not
        // believe SDL-style polling works when no SDL event source exists.
        Err(Error::BackendUnavailable(SDL3_REMAINING_WORK))
    }

    fn push_event(&self, _shared: &Shared, _event: InputEvent) -> Result<bool> {
        Err(Error::BackendUnavailable(SDL3_REMAINING_WORK))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdl3_is_interface_only_and_says_what_is_missing() {
        let backend = Sdl3PlatformBackend;
        assert_eq!(backend.kind(), PlatformKind::Sdl3);
        assert!(!backend.status().is_implemented());
        let remaining = backend.remaining_work().expect("remaining work is listed");
        assert!(remaining.contains("libSDL3.so"));
        assert!(backend.api_surface().contains("SDL3"));
    }
}
