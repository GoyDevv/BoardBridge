// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL contexts.
//!
//! The bridge requests **OpenGL ES 3.x** and walks a documented fallback chain
//! instead of assuming the development device's capability:
//!
//! ```text
//! ES 3.2 (EGL_CONTEXT_MAJOR/MINOR_VERSION)
//!   → ES 3.1
//!   → ES 3.0
//!   → ES 3   (EGL_CONTEXT_CLIENT_VERSION, for EGL 1.4 drivers without
//!             KHR_create_context minor-version support)
//! ```
//!
//! Nothing here is GPU-vendor specific: devices that only expose ES 3.0 (and
//! emulators running on SwiftShader) land on the matching step of the chain and
//! the *actual* GL version is read back later through `glGetString`.

use crate::bb_debug;
use crate::bb_warn;
use crate::egl::ffi::{
    self, EGLint, EGL_CONTEXT_CLIENT_VERSION, EGL_CONTEXT_MAJOR_VERSION, EGL_CONTEXT_MINOR_VERSION,
    EGL_NONE, EGL_NO_CONTEXT, EGL_NO_SURFACE, EGL_TRUE,
};
use crate::egl::surface::{PbufferSurface, WindowSurface};
use crate::egl::{last_error, Config, Display};
use crate::error::{Error, Result};

/// A context version request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextRequest {
    /// Major version (always 3 for the ES 3.x chain).
    pub major: EGLint,
    /// Minor version.
    pub minor: EGLint,
    /// Use `EGL_CONTEXT_CLIENT_VERSION` instead of the major/minor attributes.
    pub client_version_only: bool,
}

impl ContextRequest {
    /// An OpenGL ES `major.minor` request.
    pub const fn gl_es(major: EGLint, minor: EGLint) -> ContextRequest {
        ContextRequest {
            major,
            minor,
            client_version_only: false,
        }
    }

    /// The legacy form: `EGL_CONTEXT_CLIENT_VERSION = major`.
    pub const fn gl_es_client_version(major: EGLint) -> ContextRequest {
        ContextRequest {
            major,
            minor: 0,
            client_version_only: true,
        }
    }

    fn attributes(&self) -> [EGLint; 5] {
        if self.client_version_only {
            [
                EGL_CONTEXT_CLIENT_VERSION,
                self.major,
                EGL_NONE,
                EGL_NONE,
                EGL_NONE,
            ]
        } else {
            [
                EGL_CONTEXT_MAJOR_VERSION,
                self.major,
                EGL_CONTEXT_MINOR_VERSION,
                self.minor,
                EGL_NONE,
            ]
        }
    }

    /// `ES 3.2`, for logs.
    pub fn describe(&self) -> String {
        if self.client_version_only {
            format!("ES {} (client version)", self.major)
        } else {
            format!("ES {}.{}", self.major, self.minor)
        }
    }
}

/// The order in which context versions are attempted.
pub const ES3_FALLBACK_CHAIN: [ContextRequest; 4] = [
    ContextRequest::gl_es(3, 2),
    ContextRequest::gl_es(3, 1),
    ContextRequest::gl_es(3, 0),
    ContextRequest::gl_es_client_version(3),
];

/// What to make current on a thread.
#[derive(Clone, Copy, Debug)]
pub enum CurrentTarget<'a> {
    /// Render to a window surface.
    Window(&'a WindowSurface),
    /// Render offscreen (surface-loss path).
    Pbuffer(&'a PbufferSurface),
    /// Unbind whatever is current on the calling thread.
    None,
}

/// An EGL rendering context.
pub struct Context {
    raw: ffi::EGLContext,
    request: ContextRequest,
    display: ffi::EGLDisplay,
}

impl Context {
    /// Creates a context for exactly one request.
    pub fn create(display: &Display, config: &Config, request: ContextRequest) -> Result<Context> {
        let attributes = request.attributes();
        let raw = unsafe {
            ffi::eglCreateContext(
                display.raw(),
                config.raw(),
                EGL_NO_CONTEXT,
                attributes.as_ptr(),
            )
        };
        if raw == EGL_NO_CONTEXT {
            return Err(Error::graphics("eglCreateContext", last_error()));
        }
        Ok(Context {
            raw,
            request,
            display: display.raw(),
        })
    }

    /// Creates the best context the device can give, walking
    /// [`ES3_FALLBACK_CHAIN`] and logging which step was refused.
    pub fn create_best(display: &Display, config: &Config) -> Result<Context> {
        for (index, request) in ES3_FALLBACK_CHAIN.iter().enumerate() {
            match Context::create(display, config, *request) {
                Ok(context) => {
                    if index > 0 {
                        bb_warn!(
                            "OpenGL ES {} unavailable; using {}",
                            ES3_FALLBACK_CHAIN[0].describe(),
                            request.describe()
                        );
                    }
                    return Ok(context);
                }
                Err(error) => {
                    bb_debug!("could not create {}: {}", request.describe(), error);
                }
            }
        }
        Err(Error::graphics("eglCreateContext", last_error()))
    }

    /// The request this context was created with.
    pub fn request(&self) -> ContextRequest {
        self.request
    }

    /// Raw handle.
    pub fn raw(&self) -> ffi::EGLContext {
        self.raw
    }

    /// Forgets the handle without calling `eglDestroyContext`.
    ///
    /// Used only when the display that owns this context has been terminated by
    /// someone else in the process (see [`Display::disarm`]); touching the
    /// handle after that is a use-after-free inside libEGL.
    pub fn disarm(&mut self) {
        self.raw = EGL_NO_CONTEXT;
    }

    /// Makes this context current on the calling thread against `target`.
    ///
    /// Note the subject: *the calling thread*. The bridge relies on this being
    /// called on the thread that will issue GL commands, which is why the
    /// graphics backend records which thread owns the binding.
    pub fn make_current(&self, display: &Display, target: CurrentTarget<'_>) -> Result<()> {
        let (draw, read, context) = match target {
            CurrentTarget::Window(surface) => (surface.raw(), surface.raw(), self.raw),
            CurrentTarget::Pbuffer(surface) => (surface.raw(), surface.raw(), self.raw),
            CurrentTarget::None => (EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT),
        };
        let ok = unsafe { ffi::eglMakeCurrent(display.raw(), draw, read, context) };
        if ok != EGL_TRUE {
            return Err(Error::graphics("eglMakeCurrent", last_error()));
        }
        Ok(())
    }

    /// `true` when this context is current on the calling thread.
    pub fn is_current(&self) -> bool {
        unsafe { ffi::eglGetCurrentContext() == self.raw }
    }

    /// The draw surface current on the calling thread (null when none).
    pub fn current_draw_surface() -> ffi::EGLSurface {
        unsafe { ffi::eglGetCurrentSurface(ffi::EGL_DRAW) }
    }

    /// Releases EGL state owned by the calling thread
    /// (`eglReleaseThread`, called when a thread that used EGL ends).
    pub fn release_thread() {
        unsafe {
            ffi::eglReleaseThread();
        }
    }

    /// Sets the swap interval for the calling thread (`1` = vsync).
    pub fn set_swap_interval(&self, display: &Display, interval: EGLint) -> Result<()> {
        let ok = unsafe { ffi::eglSwapInterval(display.raw(), interval) };
        if ok != EGL_TRUE {
            // Not fatal: a driver that refuses means "no vsync control", and the
            // loop still paces itself.
            bb_warn!("eglSwapInterval({interval}) failed: 0x{:04x}", last_error());
        }
        Ok(())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if self.raw == EGL_NO_CONTEXT {
            return;
        }
        unsafe {
            ffi::eglMakeCurrent(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        }
        let ok = unsafe { ffi::eglDestroyContext(self.display, self.raw) };
        if ok != EGL_TRUE {
            bb_warn!("eglDestroyContext failed: 0x{:04x}", last_error());
        }
        self.raw = EGL_NO_CONTEXT;
    }
}

impl core::fmt::Debug for Context {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Context")
            .field("ptr", &self.raw)
            .field("request", &self.request)
            .finish()
    }
}

// SAFETY: see `crate::egl::ffi`. The backend guarantees this context is current
// on at most one thread at a time.
unsafe impl Send for Context {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_chain_is_ordered_from_newest_to_oldest() {
        assert_eq!(ES3_FALLBACK_CHAIN[0], ContextRequest::gl_es(3, 2));
        assert_eq!(ES3_FALLBACK_CHAIN[1], ContextRequest::gl_es(3, 1));
        assert_eq!(ES3_FALLBACK_CHAIN[2], ContextRequest::gl_es(3, 0));
        assert!(ES3_FALLBACK_CHAIN[3].client_version_only);
        assert_eq!(ES3_FALLBACK_CHAIN[3].describe(), "ES 3 (client version)");
    }

    #[test]
    fn attribute_lists_use_the_right_egl_attributes() {
        let modern = ContextRequest::gl_es(3, 2).attributes();
        assert_eq!(
            modern,
            [
                EGL_CONTEXT_MAJOR_VERSION,
                3,
                EGL_CONTEXT_MINOR_VERSION,
                2,
                EGL_NONE
            ]
        );
        let legacy = ContextRequest::gl_es_client_version(3).attributes();
        assert_eq!(legacy[0], EGL_CONTEXT_CLIENT_VERSION);
        assert_eq!(legacy[1], 3);
        assert_eq!(legacy[2], EGL_NONE);
    }

    #[test]
    fn describe_is_readable() {
        assert_eq!(ContextRequest::gl_es(3, 2).describe(), "ES 3.2");
    }
}
