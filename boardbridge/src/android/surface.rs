// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Explicit, balanced `ANativeWindow` ownership.
//!
//! Every `ANativeWindow` reference in the bridge lives inside one
//! [`OwnedNativeWindow`]. The type is deliberately *move-only* — there is no
//! `Clone` — so the compiler enforces the "exactly one owner releases exactly
//! one reference" rule that the old C++ implementation had to maintain by hand
//! across a mutex and a condition variable.
//!
//! Ownership flow for one surface:
//!
//! ```text
//! surfaceCreated (UI thread)
//!   ANativeWindow_fromSurface  → +1 reference            [OwnedNativeWindow]
//!   command::Attach(window)    → moved to the bridge thread
//! bridge thread
//!   create EGLSurface from the window
//!     … rendering …
//!   destroy EGLSurface (always first)
//!   drop(window)               → ANativeWindow_release   (-1)
//! ```
//!
//! The teardown order is not left to call sites: [`WindowBinding`] (see
//! [`crate::egl::surface`]) owns both the `EGLSurface` and the window and drops
//! them in that order, so "EGLSurface destroyed before its window is released"
//! is a property of the type, not of the code path that happens to run.

use core::fmt;
use core::ptr::NonNull;

use crate::android::ffi;
use crate::error::{Error, Result};

/// Size of a surface in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceSize {
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
}

impl SurfaceSize {
    /// Creates a size.
    pub const fn new(width: i32, height: i32) -> SurfaceSize {
        SurfaceSize { width, height }
    }

    /// `true` when both dimensions are positive.
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// `true` when the size is different from `other`.
    pub fn differs_from(&self, other: &SurfaceSize) -> bool {
        self.width != other.width || self.height != other.height
    }

    /// `width x height`, for logs.
    pub fn label(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// An owned `ANativeWindow` reference.
///
/// The wrapper is `Send` because the NDK window object is reference counted and
/// thread agnostic; the bridge's synchronization guarantees that only one thread
/// uses a given binding for EGL work at a time (see `docs/THREADING.md`).
pub struct OwnedNativeWindow {
    raw: NonNull<ffi::ANativeWindow>,
}

// SAFETY: `ANativeWindow` is an opaque, reference-counted NDK object that is
// safe to use from any thread (the NDK explicitly supports handing it between
// threads). The bridge additionally serializes EGL use of a binding through the
// lifecycle machine and the graphics backend's own lock, so exactly one thread
// touches the window at any time. `NonNull` is `!Send` only because it is a raw
// pointer; ownership is unique (no `Clone`), so moving it cannot alias.
unsafe impl Send for OwnedNativeWindow {}

impl OwnedNativeWindow {
    /// Takes ownership of a reference obtained elsewhere.
    ///
    /// # Safety
    ///
    /// `raw` must be a valid window pointer whose reference the caller is
    /// transferring (e.g. the result of [`ffi::ANativeWindow_fromSurface`] or of
    /// [`OwnedNativeWindow::acquire`]).
    pub unsafe fn from_owned_raw(raw: *mut ffi::ANativeWindow) -> Option<OwnedNativeWindow> {
        NonNull::new(raw).map(|raw| OwnedNativeWindow { raw })
    }

    /// Acquires one reference from a Java `Surface` (`ANativeWindow_fromSurface`).
    ///
    /// Returns `None` when the NDK refuses (for example for an already released
    /// `Surface`), which is not fatal: the caller logs and keeps running.
    ///
    /// # Safety
    ///
    /// `env` and `surface` must be the JNI pointers of a live native method call
    /// (only valid for the duration of that call).
    pub unsafe fn from_surface(env: ffi::JniEnvPtr, surface: ffi::JObjectPtr) -> Option<Self> {
        let raw = ffi::ANativeWindow_fromSurface(env, surface);
        Self::from_owned_raw(raw)
    }

    /// Adds one reference and returns a second owner.
    pub fn acquire(&self) -> Option<OwnedNativeWindow> {
        unsafe {
            ffi::ANativeWindow_acquire(self.raw.as_ptr());
            Self::from_owned_raw(self.raw.as_ptr())
        }
    }

    /// Raw pointer for NDK/EGL calls. The window is still owned by `self`.
    pub fn as_ptr(&self) -> *mut ffi::ANativeWindow {
        self.raw.as_ptr()
    }

    /// Raw pointer as `void*`, the form EGL and the NDK take.
    pub fn as_void_ptr(&self) -> *mut core::ffi::c_void {
        self.raw.as_ptr() as *mut core::ffi::c_void
    }

    /// Current buffer size.
    pub fn size(&self) -> SurfaceSize {
        unsafe {
            SurfaceSize {
                width: ffi::ANativeWindow_getWidth(self.raw.as_ptr()),
                height: ffi::ANativeWindow_getHeight(self.raw.as_ptr()),
            }
        }
    }

    /// Current buffer format (`WINDOW_FORMAT_*`).
    pub fn format(&self) -> i32 {
        unsafe { ffi::ANativeWindow_getFormat(self.raw.as_ptr()) }
    }

    /// Matches the window's buffer geometry to the chosen EGL config.
    ///
    /// Passing `0` for a dimension means "leave it as is", which is what the
    /// bridge wants for width/height: the window is already the right size, only
    /// the format has to agree with the config's native visual id.
    pub fn set_buffers_geometry(&self, width: i32, height: i32, format: i32) -> Result<()> {
        let rc = unsafe {
            ffi::ANativeWindow_setBuffersGeometry(self.raw.as_ptr(), width, height, format)
        };
        if rc != 0 {
            return Err(Error::Message(format!(
                "ANativeWindow_setBuffersGeometry({width}x{height}, format {format}) failed: {rc}"
            )));
        }
        Ok(())
    }
}

impl Drop for OwnedNativeWindow {
    fn drop(&mut self) {
        // SAFETY: `raw` was acquired by `from_owned_raw`/`acquire` and this type
        // is move-only, so this is the single release for this reference.
        unsafe { ffi::ANativeWindow_release(self.raw.as_ptr()) }
    }
}

impl fmt::Debug for OwnedNativeWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = self.size();
        f.debug_struct("OwnedNativeWindow")
            .field("ptr", &self.raw.as_ptr())
            .field("size", &size)
            .field("format", &self.format())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_reports_validity_and_difference() {
        assert!(SurfaceSize::new(1080, 2400).is_valid());
        assert!(!SurfaceSize::new(0, 2400).is_valid());
        assert!(!SurfaceSize::new(-1, -1).is_valid());
        assert!(SurfaceSize::new(1, 2).differs_from(&SurfaceSize::new(1, 3)));
        assert!(!SurfaceSize::new(1, 2).differs_from(&SurfaceSize::new(1, 2)));
        assert_eq!(SurfaceSize::new(1080, 2400).label(), "1080x2400");
    }

    #[test]
    fn null_window_pointers_are_rejected() {
        // The NDK returns null when a Surface is already gone; the bridge must
        // treat that as "no window", never as a valid binding.
        assert!(unsafe { OwnedNativeWindow::from_owned_raw(core::ptr::null_mut()) }.is_none());
    }
}
