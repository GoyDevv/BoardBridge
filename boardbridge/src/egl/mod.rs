// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL in Rust (replaces the former `egl_core.{h,cpp}`).
//!
//! * [`display`] — `EGLDisplay` initialization and `EGLConfig` selection with a
//!   documented fallback chain.
//! * [`context`] — `EGLContext` creation with the ES 3.2 → 3.0 → legacy chain.
//! * [`surface`] — window/pbuffer surfaces and [`surface::WindowBinding`], whose
//!   field order guarantees "EGLSurface destroyed before its window is released".
//! * [`ffi`] — the EGL entry points and constants.
//!
//! Threading: EGL handles are usable from any thread, but a *context* may be
//! current on only one thread at a time, and a surface must not be destroyed
//! while it is current anywhere. [`crate::graphics::gles`] enforces both; this
//! module documents them where they are relevant ([`context::Context`],
//! [`surface::WindowSurface`]).

pub mod context;
pub mod display;
pub mod ffi;
pub mod surface;

pub use context::{Context, ContextRequest, CurrentTarget, ES3_FALLBACK_CHAIN};
pub use display::{Config, ConfigRequest, Display};
pub use surface::{PbufferSurface, WindowBinding, WindowSurface};

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

/// Reads and clears the last EGL error.
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
pub fn check(ok: ffi::EGLBoolean, op: &'static str) -> Result<()> {
    if ok == ffi::EGL_TRUE {
        Ok(())
    } else {
        Err(Error::graphics(op, last_error()))
    }
}
