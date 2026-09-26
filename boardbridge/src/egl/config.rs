// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! What the bridge asks EGL for.
//!
//! [`ConfigRequest`] is **pure data** (no EGL call, no FFI) and therefore lives
//! outside the Android-gated EGL modules: `graphics::GraphicsConfig` carries one
//! on every target, and the request's construction is unit-tested on the CI
//! runner (`cargo test`) rather than only on a device.

use crate::egl::ffi::{
    EGLint, EGL_ALPHA_SIZE, EGL_BLUE_SIZE, EGL_DEPTH_SIZE, EGL_GREEN_SIZE, EGL_NONE,
    EGL_OPENGL_ES3_BIT, EGL_PBUFFER_BIT, EGL_RED_SIZE, EGL_RENDERABLE_TYPE, EGL_SURFACE_TYPE,
    EGL_WINDOW_BIT,
};

/// What the bridge asks for when choosing an `EGLConfig`.
///
/// Kept as data (not a hard-coded attribute array) so the Vulkan/ANGLE paths and
/// the diagnostics can change requirements without touching the EGL code, and so
/// the fallback chain is explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigRequest {
    /// Alpha channel bits (8 for a translucent window, 0 to save bandwidth).
    pub alpha: EGLint,
    /// Depth bits.
    pub depth: EGLint,
    /// Stencil bits.
    pub stencil: EGLint,
    /// `EGL_SURFACE_TYPE` bits.
    pub surface_type: EGLint,
    /// `EGL_RENDERABLE_TYPE` bits.
    pub renderable_type: EGLint,
}

impl ConfigRequest {
    /// Default request for a launcher window: RGBA8888, depth 24, stencil 8,
    /// window + pbuffer surfaces, OpenGL ES 3.
    ///
    /// The pbuffer bit is requested up front because the bridge needs an
    /// offscreen surface to keep the game's context alive when Android takes the
    /// window away (`docs/THREADING.md`, "surface loss").
    pub const fn launcher() -> ConfigRequest {
        ConfigRequest {
            alpha: 8,
            depth: 24,
            stencil: 8,
            surface_type: EGL_WINDOW_BIT | EGL_PBUFFER_BIT,
            renderable_type: EGL_OPENGL_ES3_BIT,
        }
    }

    /// A request without alpha (the window is opaque anyway).
    pub const fn without_alpha(&self) -> ConfigRequest {
        ConfigRequest { alpha: 0, ..*self }
    }

    /// Reduced depth/stencil: some mobile drivers expose no 24/8 config.
    pub const fn reduced_depth(&self) -> ConfigRequest {
        ConfigRequest {
            depth: 16,
            stencil: 0,
            ..*self
        }
    }

    /// `EGL_SURFACE_TYPE` narrowed to windows only (used when a driver reports
    /// no config that can do both).
    pub const fn window_only(&self) -> ConfigRequest {
        ConfigRequest {
            surface_type: EGL_WINDOW_BIT,
            ..*self
        }
    }

    /// The `eglChooseConfig` attribute list (always `EGL_NONE`-terminated).
    pub fn attribute_list(&self) -> [EGLint; 15] {
        [
            EGL_RENDERABLE_TYPE,
            self.renderable_type,
            EGL_SURFACE_TYPE,
            self.surface_type,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            self.alpha,
            EGL_DEPTH_SIZE,
            self.depth,
            EGL_NONE,
        ]
    }

    /// Human-readable summary for logs.
    pub fn describe(&self) -> String {
        format!(
            "rgba8 alpha={} depth={} stencil={} surface=0x{:x} renderable=0x{:x}",
            self.alpha, self.depth, self.stencil, self.surface_type, self.renderable_type
        )
    }
}

impl Default for ConfigRequest {
    fn default() -> Self {
        ConfigRequest::launcher()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_request_asks_for_a_window_and_a_pbuffer() {
        let request = ConfigRequest::launcher();
        assert_eq!(request.alpha, 8);
        assert_eq!(request.depth, 24);
        assert_eq!(request.stencil, 8);
        assert_eq!(request.surface_type & EGL_WINDOW_BIT, EGL_WINDOW_BIT);
        assert_eq!(request.surface_type & EGL_PBUFFER_BIT, EGL_PBUFFER_BIT);
        assert_eq!(request.renderable_type, EGL_OPENGL_ES3_BIT);
        assert_eq!(ConfigRequest::default(), request);
    }

    #[test]
    fn narrowing_helpers_change_only_their_own_field() {
        let base = ConfigRequest::launcher();
        assert_eq!(base.without_alpha().alpha, 0);
        assert_eq!(base.without_alpha().depth, base.depth);
        assert_eq!(base.reduced_depth().depth, 16);
        assert_eq!(base.reduced_depth().stencil, 0);
        assert_eq!(base.window_only().surface_type, EGL_WINDOW_BIT);
        assert_eq!(base.window_only().renderable_type, base.renderable_type);
    }

    #[test]
    fn the_attribute_list_is_none_terminated_and_uses_egl_attributes() {
        let list = ConfigRequest::launcher().attribute_list();
        assert_eq!(list.len(), 15);
        assert_eq!(list[0], EGL_RENDERABLE_TYPE);
        assert_eq!(list[14], EGL_NONE);
        assert_eq!(list[11], 8, "alpha bits are carried through");
        assert_eq!(list[13], 24, "depth bits are carried through");
    }

    #[test]
    fn describe_mentions_every_dimension() {
        let text = ConfigRequest::launcher().describe();
        for needle in ["alpha=", "depth=", "stencil=", "surface=", "renderable="] {
            assert!(text.contains(needle), "missing {needle} in {text}");
        }
    }
}
