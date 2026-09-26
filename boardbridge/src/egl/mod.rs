// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL in Rust (replaces the former `egl_core.{h,cpp}`).
//!
//! * [`config`] — [`config::ConfigRequest`]: pure data, compiled on every
//!   target so the cross-platform `graphics` layer can carry one and so its
//!   construction is unit-tested on the CI runner.
//! * [`ffi`] — the EGL entry points, constants and types. The constants are also
//!   host-visible; only the `extern "C"` declarations are Android-only.
//! * [`display`] — `EGLDisplay` initialization and `EGLConfig` selection with a
//!   documented fallback chain *(Android only)*.
//! * [`context`] — `EGLContext` creation with the ES 3.2 → 3.0 → legacy chain
//!   *(Android only)*.
//! * [`surface`] — window/pbuffer surfaces and `WindowBinding`, whose field order
//!   guarantees "EGLSurface destroyed before its window is released"
//!   *(Android only)*.
//!
//! Threading: EGL handles are usable from any thread, but a *context* may be
//! current on only one thread at a time, and a surface must not be destroyed
//! while it is current anywhere. [`crate::graphics::gles`] enforces both; this
//! module documents them where they are relevant.

pub mod config;
pub mod ffi;

#[cfg(target_os = "android")]
pub mod context;
#[cfg(target_os = "android")]
pub mod display;
#[cfg(target_os = "android")]
pub mod surface;

pub use config::ConfigRequest;

#[cfg(target_os = "android")]
pub use context::{Context, ContextRequest, CurrentTarget, ES3_FALLBACK_CHAIN};
#[cfg(target_os = "android")]
pub use display::{Config, Display};
#[cfg(target_os = "android")]
pub use surface::{PbufferSurface, WindowBinding, WindowSurface};

#[cfg(target_os = "android")]
use crate::error::{Error, Result};

/// What EGL reports about itself, captured once at initialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    /// EGL major version.
    pub major: ffi::EGLint,
    /// EGL minor version.
    pub minor: ffi::EGLint,
    /// `EGL_VENDOR`.
    pub vendor: String,
    /// `EGL_VERSION`.
    pub version: String,
    /// `EGL_CLIENT_APIS`.
    pub client_apis: String,
    /// `EGL_EXTENSIONS` (space separated).
    pub extensions: String,
}

impl DisplayInfo {
    /// `"1.5"`.
    pub fn version_label(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// `true` when an extension is advertised.
    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.split_whitespace().any(|ext| ext == name)
    }

    /// Compact summary for logs.
    pub fn describe(&self) -> String {
        format!(
            "EGL {} by {} (client APIs: {})",
            self.version_label(),
            self.vendor,
            if self.client_apis.is_empty() {
                "?"
            } else {
                &self.client_apis
            }
        )
    }
}

/// Reads and clears the last EGL error. Requires a live EGL implementation.
#[cfg(target_os = "android")]
pub fn last_error() -> ffi::EGLint {
    unsafe { ffi::eglGetError() }
}

/// Symbolic name of an EGL error code, for logs.
pub fn error_label(code: ffi::EGLint) -> &'static str {
    match code {
        ffi::EGL_SUCCESS => "EGL_SUCCESS",
        ffi::EGL_NOT_INITIALIZED => "EGL_NOT_INITIALIZED",
        ffi::EGL_BAD_ACCESS => "EGL_BAD_ACCESS",
        ffi::EGL_BAD_ALLOC => "EGL_BAD_ALLOC",
        ffi::EGL_BAD_ATTRIBUTE => "EGL_BAD_ATTRIBUTE",
        ffi::EGL_BAD_CONFIG => "EGL_BAD_CONFIG",
        ffi::EGL_BAD_CONTEXT => "EGL_BAD_CONTEXT",
        ffi::EGL_BAD_CURRENT_SURFACE => "EGL_BAD_CURRENT_SURFACE",
        ffi::EGL_BAD_DISPLAY => "EGL_BAD_DISPLAY",
        ffi::EGL_BAD_MATCH => "EGL_BAD_MATCH",
        ffi::EGL_BAD_NATIVE_PIXMAP => "EGL_BAD_NATIVE_PIXMAP",
        ffi::EGL_BAD_NATIVE_WINDOW => "EGL_BAD_NATIVE_WINDOW",
        ffi::EGL_BAD_PARAMETER => "EGL_BAD_PARAMETER",
        ffi::EGL_BAD_SURFACE => "EGL_BAD_SURFACE",
        ffi::EGL_CONTEXT_LOST => "EGL_CONTEXT_LOST",
        _ => "EGL_UNKNOWN",
    }
}

/// Turns an `EGLBoolean` result into a [`Result`], capturing the EGL error.
#[cfg(target_os = "android")]
pub fn check(ok: ffi::EGLBoolean, op: &'static str) -> Result<()> {
    if ok == ffi::EGL_TRUE {
        Ok(())
    } else {
        Err(Error::graphics(op, last_error()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_info_is_readable_and_reports_extensions() {
        let info = DisplayInfo {
            major: 1,
            minor: 5,
            vendor: "Android".to_string(),
            version: "1.5 Android META-EGL".to_string(),
            client_apis: "OpenGL_ES".to_string(),
            extensions: "EGL_KHR_fence_sync EGL_ANDROID_recordable".to_string(),
        };
        assert_eq!(info.version_label(), "1.5");
        assert!(info.has_extension("EGL_KHR_fence_sync"));
        assert!(!info.has_extension("EGL_KHR_no_config_context"));
        assert!(info.describe().contains("Android"));
    }

    #[test]
    fn error_labels_name_the_documented_codes() {
        assert_eq!(error_label(ffi::EGL_SUCCESS), "EGL_SUCCESS");
        assert_eq!(error_label(ffi::EGL_BAD_CONFIG), "EGL_BAD_CONFIG");
        assert_eq!(error_label(ffi::EGL_CONTEXT_LOST), "EGL_CONTEXT_LOST");
        assert_eq!(error_label(0x1234), "EGL_UNKNOWN");
    }
}
