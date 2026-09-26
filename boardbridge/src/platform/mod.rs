// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Platform backends: who supplies a window and input to the game.
//!
//! The bridge sits *below* whatever windowing API the game uses. Minecraft's
//! windowing has two eras, and both have to work:
//!
//! | Minecraft | Windowing | Backend here |
//! |---|---|---|
//! | 26.3+ | SDL3 (through the LWJGL SDL bindings) | [`sdl3`] — **interface only** |
//! | ≤ 26.2 | GLFW 3 (through the LWJGL GLFW bindings) | [`glfw_compat`] — **interface only** |
//! | any (bridge diagnostics, launcher UI) | none: the bridge's own queue | [`android`] — **implemented** |
//!
//! What *is* implemented and shared by all three: the window lifecycle
//! (surface ownership, pause/resume, rotation) and the input translation
//! ([`crate::input::keymap`] produces real SDL scancodes/keycodes, gamepad axes
//! and buttons, relative mouse motion). A backend only has to *expose* that to
//! whichever API the game calls.
//!
//! Nothing here fakes one API with another: [`sdl3`] does not pretend to be SDL
//! by wrapping EGL, and [`glfw_compat`] does not pretend to be GLFW by wrapping
//! SDL. Each lists the real remaining work, and
//! [`PlatformBackend::status`] reports it as
//! [`BackendStatus::InterfaceOnly`](crate::graphics::BackendStatus::InterfaceOnly)
//! so diagnostics (and the launcher UI) can say so out loud.

pub mod android;
pub mod glfw_compat;
pub mod sdl3;

use std::sync::Arc;

use crate::error::Result;
use crate::graphics::BackendStatus;
use crate::input::{InputEvent, QueueStats};
use crate::runtime::control::Shared;

/// Which platform integration provides windows and input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum PlatformKind {
    /// Kotlin pushes Android events straight into the bridge queue.
    AndroidNative = 0,
    /// SDL3, as used by Minecraft 26.3+ through LWJGL.
    Sdl3 = 1,
    /// GLFW 3 compatibility, as used by Minecraft ≤ 26.2 through LWJGL.
    GlfwCompat = 2,
}

impl PlatformKind {
    /// Decodes the value sent from Kotlin.
    pub fn from_jni(value: i32) -> PlatformKind {
        match value {
            1 => PlatformKind::Sdl3,
            2 => PlatformKind::GlfwCompat,
            _ => PlatformKind::AndroidNative,
        }
    }

    /// Name for logs and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            PlatformKind::AndroidNative => "android-native",
            PlatformKind::Sdl3 => "sdl3",
            PlatformKind::GlfwCompat => "glfw-compat",
        }
    }
}

/// Window facts a game-facing API needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowInfo {
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
    /// Whether a window surface is currently bound.
    pub has_surface: bool,
    /// Monotonic binding id (changes on every rebind, including rotation).
    pub binding_generation: u64,
}

/// The seam a game-facing windowing/input layer plugs into.
pub trait PlatformBackend: Send + Sync {
    /// Which integration this is.
    fn kind(&self) -> PlatformKind;

    /// Human-readable name.
    fn name(&self) -> &'static str;

    /// Implemented, or interface-only with the remaining work named.
    fn status(&self) -> BackendStatus;

    /// Remaining work for an interface-only backend.
    fn remaining_work(&self) -> Option<&'static str>;

    /// The API surface this backend satisfies, for logs and documentation.
    fn api_surface(&self) -> &'static str;

    /// Current window state.
    fn window_info(&self, shared: &Shared) -> Result<WindowInfo>;

    /// Input queue counters.
    fn queue_stats(&self, shared: &Shared) -> Result<QueueStats>;

    /// Next queued event, if any.
    fn poll_event(&self, shared: &Shared) -> Result<Option<InputEvent>>;

    /// True when the event was queued without evicting an older one.
    fn push_event(&self, shared: &Shared, event: InputEvent) -> Result<bool>;
}

/// Creates a platform backend handle.
pub fn backend(kind: PlatformKind) -> Arc<dyn PlatformBackend> {
    match kind {
        PlatformKind::AndroidNative => Arc::new(android::AndroidPlatformBackend),
        PlatformKind::Sdl3 => Arc::new(sdl3::Sdl3PlatformBackend),
        PlatformKind::GlfwCompat => Arc::new(glfw_compat::GlfwCompatPlatformBackend),
    }
}

/// Every backend with its status, for `getStatus`/`runSelfTest`.
pub fn describe_all() -> String {
    [
        PlatformKind::AndroidNative,
        PlatformKind::Sdl3,
        PlatformKind::GlfwCompat,
    ]
    .iter()
    .map(|kind| {
        let backend = backend(*kind);
        match backend.remaining_work() {
            Some(remaining) => format!(
                "{}={} ({remaining})",
                backend.name(),
                backend.status().label()
            ),
            None => format!("{}={}", backend.name(), backend.status().label()),
        }
    })
    .collect::<Vec<_>>()
    .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_decoding_and_names() {
        assert_eq!(PlatformKind::from_jni(0), PlatformKind::AndroidNative);
        assert_eq!(PlatformKind::from_jni(1), PlatformKind::Sdl3);
        assert_eq!(PlatformKind::from_jni(2), PlatformKind::GlfwCompat);
        assert_eq!(PlatformKind::from_jni(9), PlatformKind::AndroidNative);
        assert_eq!(PlatformKind::GlfwCompat.name(), "glfw-compat");
    }

    #[test]
    fn only_the_android_backend_claims_to_be_implemented() {
        assert!(backend(PlatformKind::AndroidNative)
            .status()
            .is_implemented());
        assert!(!backend(PlatformKind::Sdl3).status().is_implemented());
        assert!(!backend(PlatformKind::GlfwCompat).status().is_implemented());
        assert!(backend(PlatformKind::Sdl3).remaining_work().is_some());
        assert!(backend(PlatformKind::GlfwCompat).remaining_work().is_some());
        assert!(backend(PlatformKind::AndroidNative)
            .remaining_work()
            .is_none());
    }

    #[test]
    fn describe_all_lists_every_backend_honestly() {
        let text = describe_all();
        assert!(text.contains("android-native=implemented"));
        assert!(text.contains("sdl3=interface-only"));
        assert!(text.contains("glfw-compat=interface-only"));
    }
}
