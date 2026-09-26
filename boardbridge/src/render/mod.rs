// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Rendering helpers for the bridge's own loop.
//!
//! The bridge does not render the game. It provides two loop arrangements, and
//! this module supplies the *content* the bridge itself draws in the first one:
//!
//! | Mode | Who drives the loop | What draws |
//! |---|---|---|
//! | [`crate::runtime::LoopMode::Internal`] | the bridge thread | [`DiagnosticRenderer`] |
//! | [`crate::runtime::LoopMode::Inverted`] | the game thread (Minecraft) | the game |
//!
//! [`DiagnosticMode`] is defined here and not in the renderer, because the
//! *choice* is pure data: it travels through `RuntimeConfig` and is decoded from
//! JNI on every target, while only the GLES renderer itself is Android-only.
//!
//! The diagnostic renderer keeps the two capabilities the old demo had that are
//! genuinely useful — proof that pixels reached the framebuffer, and a shader
//! pipeline check — and drops the "spinning triangle is the product" framing: the
//! triangle is now one selectable diagnostic mode among several, and the default
//! is the cheapest possible verification (a solid clear plus a centre-pixel
//! readback that logcat can grep).

#[cfg(target_os = "android")]
pub mod diagnostics;

#[cfg(target_os = "android")]
pub use diagnostics::DiagnosticRenderer;

/// Diagnostic render mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum DiagnosticMode {
    /// Clear to black, nothing else.
    None = 0,
    /// Clear to a fixed colour.
    #[default]
    Solid = 1,
    /// Spinning shaded triangle.
    Triangle = 2,
}

impl DiagnosticMode {
    /// Decodes the value sent from Kotlin.
    pub fn from_jni(value: i32) -> DiagnosticMode {
        match value {
            1 => DiagnosticMode::Solid,
            2 => DiagnosticMode::Triangle,
            _ => DiagnosticMode::None,
        }
    }

    /// Name used in the log lines (`SOLID`, `TRIANGLE`, `NONE`).
    pub fn name(self) -> &'static str {
        match self {
            DiagnosticMode::None => "NONE",
            DiagnosticMode::Solid => "SOLID",
            DiagnosticMode::Triangle => "TRIANGLE",
        }
    }

    /// Next mode in the toggle cycle.
    pub fn next(self) -> DiagnosticMode {
        match self {
            DiagnosticMode::None => DiagnosticMode::Solid,
            DiagnosticMode::Solid => DiagnosticMode::Triangle,
            DiagnosticMode::Triangle => DiagnosticMode::Solid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle_and_names() {
        assert_eq!(DiagnosticMode::from_jni(1), DiagnosticMode::Solid);
        assert_eq!(DiagnosticMode::from_jni(2), DiagnosticMode::Triangle);
        assert_eq!(DiagnosticMode::from_jni(7), DiagnosticMode::None);
        assert_eq!(DiagnosticMode::default(), DiagnosticMode::Solid);
        assert_eq!(DiagnosticMode::Solid.name(), "SOLID");
        assert_eq!(DiagnosticMode::Solid.next(), DiagnosticMode::Triangle);
        assert_eq!(DiagnosticMode::Triangle.next(), DiagnosticMode::Solid);
        assert_eq!(DiagnosticMode::None.next(), DiagnosticMode::Solid);
    }
}
