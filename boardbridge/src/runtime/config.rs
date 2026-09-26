// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Runtime configuration.
//!
//! Everything the Kotlin shell can decide about the bridge's behaviour, decoded
//! from the primitive arguments of `createRuntime` and validated here — this
//! module is pure, so the decoding rules (and the defaults) are unit-tested on
//! the CI runner instead of on a phone.

use crate::graphics::{GraphicsConfig, RendererKind};
use crate::input::DEFAULT_CAPACITY;
use crate::render::DiagnosticMode;

/// Who drives the frame loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum LoopMode {
    /// The bridge thread draws through [`crate::render::DiagnosticRenderer`].
    ///
    /// This is the mode the demo app and the CI render test use, and the mode in
    /// which the bridge can prove the surface/EGL path works on its own.
    #[default]
    Internal = 0,
    /// The game owns the loop (Minecraft): it attaches, renders and presents
    /// from its own thread.
    ///
    /// The bridge thread then only services surface lifecycle changes, and the
    /// game is told about surface loss through a [`crate::input::LifecycleNotice`]
    /// instead of being interrupted.
    Inverted = 1,
}

impl LoopMode {
    /// Decodes the value sent from Kotlin.
    pub fn from_jni(value: i32) -> LoopMode {
        match value {
            1 => LoopMode::Inverted,
            _ => LoopMode::Internal,
        }
    }

    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            LoopMode::Internal => "internal",
            LoopMode::Inverted => "inverted",
        }
    }

    /// `true` for [`LoopMode::Inverted`].
    pub fn is_inverted(self) -> bool {
        self == LoopMode::Inverted
    }
}

// Flag bits. Keep in sync with `NativeBridge.kt`.
/// Request vsync (`eglSwapInterval(1)`).
pub const FLAG_VSYNC: i32 = 1 << 0;
/// Keep the EGL context alive across surface loss.
pub const FLAG_PRESERVE_CONTEXT: i32 = 1 << 1;
/// Emit the per-second diagnostics line while the internal loop runs.
pub const FLAG_DIAGNOSTIC_LOGS: i32 = 1 << 2;

/// Runtime configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Which graphics backend to use.
    pub renderer: RendererKind,
    /// Who owns the frame loop.
    pub loop_mode: LoopMode,
    /// Initial mode of the diagnostic renderer (`None` to keep the bridge from
    /// painting, e.g. while the launcher shows its own loading UI).
    pub diagnostic_mode: DiagnosticMode,
    /// Whether to emit the per-second diagnostics line.
    pub diagnostic_logs: bool,
    /// Swap interval for EGL (1 = vsync, 0 = immediate).
    pub swap_interval: i32,
    /// Keep the EGL context (and the game's GL objects) across surface loss.
    pub preserve_context: bool,
    /// Frame-rate cap for the internal loop; 0 means "let vsync pace it".
    pub target_fps: u32,
    /// Input queue capacity.
    pub input_capacity: usize,
    /// How long the UI thread may block in `surfaceDestroyed` waiting for the
    /// bridge to release the surface. Safety never depends on this wait: on
    /// timeout the release completes asynchronously.
    pub detach_timeout_ms: u64,
    /// How long the graphics backend waits for in-flight presents / the owning
    /// thread before deferring a release.
    pub drain_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            renderer: RendererKind::Auto,
            loop_mode: LoopMode::Internal,
            diagnostic_mode: DiagnosticMode::Solid,
            diagnostic_logs: true,
            swap_interval: 1,
            preserve_context: true,
            target_fps: 0,
            input_capacity: DEFAULT_CAPACITY,
            detach_timeout_ms: 250,
            drain_timeout_ms: 250,
        }
    }
}

impl RuntimeConfig {
    /// Decodes the arguments of `NativeBridge.createRuntime`.
    ///
    /// Out-of-range values are clamped rather than rejected: a launcher that asks
    /// for 240 fps on a 60 Hz display should get 240, but a negative frame rate
    /// or a nonsensical timeout must not become a footgun.
    pub fn from_jni(
        renderer: i32,
        loop_mode: i32,
        flags: i32,
        diagnostic_mode: i32,
        target_fps: i32,
    ) -> RuntimeConfig {
        let vsync = flags & FLAG_VSYNC != 0;
        RuntimeConfig {
            renderer: RendererKind::from_jni(renderer),
            loop_mode: LoopMode::from_jni(loop_mode),
            diagnostic_mode: DiagnosticMode::from_jni(diagnostic_mode),
            diagnostic_logs: flags & FLAG_DIAGNOSTIC_LOGS != 0,
            swap_interval: if vsync { 1 } else { 0 },
            preserve_context: flags & FLAG_PRESERVE_CONTEXT != 0,
            target_fps: if target_fps > 0 { target_fps as u32 } else { 0 },
            ..RuntimeConfig::default()
        }
    }

    /// The flag bits that correspond to this configuration.
    pub fn flags(&self) -> i32 {
        let mut bits = 0;
        if self.swap_interval > 0 {
            bits |= FLAG_VSYNC;
        }
        if self.preserve_context {
            bits |= FLAG_PRESERVE_CONTEXT;
        }
        if self.diagnostic_logs {
            bits |= FLAG_DIAGNOSTIC_LOGS;
        }
        bits
    }

    /// Graphics-backend configuration derived from this runtime configuration.
    pub fn graphics_config(&self) -> GraphicsConfig {
        GraphicsConfig {
            swap_interval: self.swap_interval,
            preserve_context: self.preserve_context,
            drain_timeout_ms: self.drain_timeout_ms,
            ..GraphicsConfig::default()
        }
    }

    /// Compact summary for logs and `getStatus`.
    pub fn describe(&self) -> String {
        format!(
            "renderer={} loop={} diag={} vsync={} fps_cap={} preserve={} input_cap={} detach_wait={}ms drain_wait={}ms",
            self.renderer.name(),
            self.loop_mode.name(),
            self.diagnostic_mode.name(),
            self.swap_interval,
            self.target_fps,
            self.preserve_context,
            self.input_capacity,
            self.detach_timeout_ms,
            self.drain_timeout_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative_and_documented() {
        let config = RuntimeConfig::default();
        assert_eq!(config.renderer, RendererKind::Auto);
        assert_eq!(config.loop_mode, LoopMode::Internal);
        assert_eq!(config.diagnostic_mode, DiagnosticMode::Solid);
        assert_eq!(config.swap_interval, 1);
        assert!(config.preserve_context);
        assert_eq!(config.target_fps, 0);
        assert_eq!(config.input_capacity, DEFAULT_CAPACITY);
        // Timeouts must be short enough not to risk an ANR.
        assert!(config.detach_timeout_ms <= 500);
        assert!(config.drain_timeout_ms <= 500);
    }

    #[test]
    fn decodes_jni_arguments() {
        let config = RuntimeConfig::from_jni(
            1,
            1,
            FLAG_VSYNC | FLAG_PRESERVE_CONTEXT | FLAG_DIAGNOSTIC_LOGS,
            2,
            120,
        );
        assert_eq!(config.renderer, RendererKind::Gles);
        assert_eq!(config.loop_mode, LoopMode::Inverted);
        assert_eq!(config.diagnostic_mode, DiagnosticMode::Triangle);
        assert_eq!(config.swap_interval, 1);
        assert!(config.preserve_context);
        assert_eq!(config.target_fps, 120);
    }

    #[test]
    fn decodes_absent_flags_to_safe_defaults() {
        let config = RuntimeConfig::from_jni(0, 0, 0, 0, -5);
        assert_eq!(config.renderer, RendererKind::Auto);
        assert_eq!(config.loop_mode, LoopMode::Internal);
        assert_eq!(config.diagnostic_mode, DiagnosticMode::None);
        assert_eq!(
            config.swap_interval, 0,
            "no vsync flag means immediate swap"
        );
        assert!(!config.preserve_context);
        assert!(!config.diagnostic_logs);
        assert_eq!(config.target_fps, 0, "negative frame caps are ignored");
    }

    #[test]
    fn flag_bits_round_trip() {
        let config = RuntimeConfig::default();
        let flags = config.flags();
        let decoded = RuntimeConfig::from_jni(0, 0, flags, 0, 0);
        assert_eq!(decoded.swap_interval, config.swap_interval);
        assert_eq!(decoded.preserve_context, config.preserve_context);
        assert_eq!(decoded.diagnostic_logs, config.diagnostic_logs);
    }

    #[test]
    fn graphics_config_inherits_the_relevant_fields() {
        let config = RuntimeConfig {
            swap_interval: 0,
            preserve_context: false,
            drain_timeout_ms: 42,
            ..RuntimeConfig::default()
        };
        let graphics = config.graphics_config();
        assert_eq!(graphics.swap_interval, 0);
        assert!(!graphics.preserve_context);
        assert_eq!(graphics.drain_timeout_ms, 42);
        assert_eq!(
            graphics.config_request,
            crate::egl::ConfigRequest::launcher()
        );
    }

    #[test]
    fn describe_mentions_every_dimension() {
        let text = RuntimeConfig::default().describe();
        for needle in [
            "renderer=",
            "loop=",
            "diag=",
            "vsync=",
            "fps_cap=",
            "preserve=",
        ] {
            assert!(text.contains(needle), "missing {needle} in {text}");
        }
    }
}
