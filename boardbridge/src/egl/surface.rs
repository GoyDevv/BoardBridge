// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL surfaces.
//!
//! Three types, each with one job:
//!
//! * [`WindowSurface`] — an `EGLSurface` backed by an `ANativeWindow`.
//! * [`PbufferSurface`] — an offscreen `EGLSurface`, used to keep a rendering
//!   context alive while Android has taken the window away.
//! * [`WindowBinding`] — the *pair* (window surface, owned window), which is the
//!   unit the bridge thread binds and retires. Declaring the surface before the
//!   window makes Rust's drop order — surface first, window second — the
//!   mechanical guarantee that an `EGLSurface` never outlives the window it was
//!   created from, and that the window reference is released only after EGL is
//!   done with it. That ordering is the whole reason this type exists.

use core::ptr;

use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::bb_debug;
use crate::bb_warn;
use crate::egl::ffi::{self, EGLDisplay, EGLSurface, EGLint, EGL_HEIGHT, EGL_TRUE, EGL_WIDTH};
use crate::egl::{last_error, Config, Display};
use crate::error::{Error, Result};

/// An `EGLSurface` created from an `ANativeWindow`.
pub struct WindowSurface {
    display: EGLDisplay,
    raw: EGLSurface,
    size: SurfaceSize,
}

impl WindowSurface {
    /// Creates a window surface for `window` using `config`.
    ///
    /// The window's buffer geometry is matched to the config's native visual id
    /// first; a driver that refuses is logged at debug level and not treated as
    /// fatal, because Android often already has the right format.
    pub fn create(
        display: &Display,
        config: &Config,
        window: &OwnedNativeWindow,
    ) -> Result<WindowSurface> {
        if config.visual_id() != 0 {
            if let Err(error) = window.set_buffers_geometry(0, 0, config.visual_id()) {
                bb_debug!("{error}");
            }
        }
        let raw = unsafe {
            ffi::eglCreateWindowSurface(
                display.raw(),
                config.raw(),
                window.as_void_ptr(),
                ptr::null(),
            )
        };
        if raw == ffi::EGL_NO_SURFACE {
            return Err(Error::graphics("eglCreateWindowSurface", last_error()));
        }
        let mut surface = WindowSurface {
            display: display.raw(),
            raw,
            size: SurfaceSize::default(),
        };
        // The size is written into `surface` instead of being applied through
        // struct update syntax (`WindowSurface { size, ..surface }`), and that is
        // deliberate. Every field that struct update syntax would take from the
        // source is `Copy` (two raw handles and a `SurfaceSize`), so it *copies*
        // them out and leaves the source fully initialised — `Drop` then runs
        // `eglDestroySurface` on the handle that is being returned. The caller
        // gets a non-null `EGLSurface` that EGL no longer recognises, and every
        // later call on it, starting with the first `eglMakeCurrent`, fails with
        // `EGL_BAD_SURFACE` (`0x300d`). That is exactly the black screen this
        // crate shipped: `eglCreateWindowSurface` succeeded, and `eglQuerySurface`
        // and `eglMakeCurrent` four milliseconds later did not.
        let size = surface.query_size().unwrap_or_else(|| window.size());
        surface.size = size;
        Ok(surface)
    }

    /// Raw handle for `eglMakeCurrent`/`eglSwapBuffers`.
    pub fn raw(&self) -> EGLSurface {
        self.raw
    }

    /// Forgets the handle without calling `eglDestroySurface`.
    ///
    /// Used when the display this surface was made from has been terminated by
    /// another EGL user in the process (see [`Display::disarm`]): the handle is
    /// still non-null, but EGL no longer recognizes it, so destroying it is at
    /// best a spurious error and inside libEGL a use-after-free.
    pub fn disarm(&mut self) {
        self.raw = ffi::EGL_NO_SURFACE;
    }

    /// Size reported by EGL at creation time. Use [`WindowSurface::refresh_size`]
    /// after a resize to pick up a new one.
    pub fn size(&self) -> SurfaceSize {
        self.size
    }

    /// Re-reads the surface size from EGL (the window may have been resized).
    pub fn refresh_size(&mut self) -> SurfaceSize {
        if let Some(size) = self.query_size() {
            self.size = size;
        }
        self.size
    }

    fn query_size(&self) -> Option<SurfaceSize> {
        let width = self.query_attribute(EGL_WIDTH)?;
        let height = self.query_attribute(EGL_HEIGHT)?;
        Some(SurfaceSize::new(width, height))
    }

    fn query_attribute(&self, attribute: EGLint) -> Option<i32> {
        let mut value: EGLint = 0;
        let ok = unsafe { ffi::eglQuerySurface(self.display, self.raw, attribute, &mut value) };
        if ok == EGL_TRUE {
            Some(value)
        } else {
            None
        }
    }
}

impl Drop for WindowSurface {
    fn drop(&mut self) {
        if self.raw == ffi::EGL_NO_SURFACE {
            return;
        }
        // EGL forbids destroying a surface that is current on the calling
        // thread, so unbind this thread first. A surface that is current on
        // *another* thread is never dropped: the graphics backend waits for that
        // thread to release it and defers the teardown otherwise.
        unsafe {
            ffi::eglMakeCurrent(
                self.display,
                ffi::EGL_NO_SURFACE,
                ffi::EGL_NO_SURFACE,
                ffi::EGL_NO_CONTEXT,
            );
        }
        let ok = unsafe { ffi::eglDestroySurface(self.display, self.raw) };
        if ok != EGL_TRUE {
            bb_warn!("eglDestroySurface failed: 0x{:04x}", last_error());
        }
        self.raw = ffi::EGL_NO_SURFACE;
    }
}

impl core::fmt::Debug for WindowSurface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WindowSurface")
            .field("ptr", &self.raw)
            .field("size", &self.size)
            .finish()
    }
}

// SAFETY: see `crate::egl::ffi` — EGL handles are thread agnostic; the bridge
// serializes access in the graphics backend.
unsafe impl Send for WindowSurface {}

/// An offscreen `EGLSurface` (pbuffer).
pub struct PbufferSurface {
    display: EGLDisplay,
    raw: EGLSurface,
    size: SurfaceSize,
}

impl PbufferSurface {
    /// Creates a pbuffer of the requested size.
    ///
    /// Fails with a clear message when the chosen config cannot back a pbuffer
    /// (the config request of a device may have narrowed to `EGL_WINDOW_BIT`);
    /// callers treat that as "no surface-loss support", not as fatal.
    pub fn create(display: &Display, config: &Config, size: SurfaceSize) -> Result<PbufferSurface> {
        if !config.supports_pbuffer() {
            return Err(Error::Message(
                "chosen EGL config has no EGL_PBUFFER_BIT; cannot keep the context alive offscreen"
                    .to_string(),
            ));
        }
        let width = size.width.max(1);
        let height = size.height.max(1);
        let attribs: [EGLint; 5] = [EGL_WIDTH, width, EGL_HEIGHT, height, ffi::EGL_NONE];
        let raw =
            unsafe { ffi::eglCreatePbufferSurface(display.raw(), config.raw(), attribs.as_ptr()) };
        if raw == ffi::EGL_NO_SURFACE {
            return Err(Error::graphics("eglCreatePbufferSurface", last_error()));
        }
        Ok(PbufferSurface {
            display: display.raw(),
            raw,
            size: SurfaceSize::new(width, height),
        })
    }

    /// Raw handle.
    pub fn raw(&self) -> EGLSurface {
        self.raw
    }

    /// Requested size.
    pub fn size(&self) -> SurfaceSize {
        self.size
    }

    /// Forgets the handle without calling `eglDestroySurface` (see
    /// [`WindowSurface::disarm`]).
    pub fn disarm(&mut self) {
        self.raw = ffi::EGL_NO_SURFACE;
    }
}

impl Drop for PbufferSurface {
    fn drop(&mut self) {
        if self.raw == ffi::EGL_NO_SURFACE {
            return;
        }
        unsafe {
            ffi::eglMakeCurrent(
                self.display,
                ffi::EGL_NO_SURFACE,
                ffi::EGL_NO_SURFACE,
                ffi::EGL_NO_CONTEXT,
            );
        }
        let ok = unsafe { ffi::eglDestroySurface(self.display, self.raw) };
        if ok != EGL_TRUE {
            bb_warn!("eglDestroySurface (pbuffer) failed: 0x{:04x}", last_error());
        }
        self.raw = ffi::EGL_NO_SURFACE;
    }
}

impl core::fmt::Debug for PbufferSurface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PbufferSurface")
            .field("ptr", &self.raw)
            .field("size", &self.size)
            .finish()
    }
}

// SAFETY: as for `WindowSurface`.
unsafe impl Send for PbufferSurface {}

/// A window surface together with the window reference it was created from.
///
/// **Field order is load-bearing**: Rust drops struct fields in declaration
/// order, so `surface` (first) is destroyed before `window` (second) is
/// released. Reordering these two fields would break the guarantee that the
/// bridge never releases an `ANativeWindow` that EGL may still be using.
#[derive(Debug)]
pub struct WindowBinding {
    surface: WindowSurface,
    window: OwnedNativeWindow,
    generation: u64,
}

impl WindowBinding {
    /// Binds `window` with a freshly created `EGLSurface`.
    ///
    /// If the surface cannot be created the window is released here (it is moved
    /// into this function and dropped on the error path), so a failed bind can
    /// never leak a window reference.
    pub fn new(
        display: &Display,
        config: &Config,
        window: OwnedNativeWindow,
        generation: u64,
    ) -> Result<WindowBinding> {
        let surface = WindowSurface::create(display, config, &window)?;
        Ok(WindowBinding {
            surface,
            window,
            generation,
        })
    }

    /// The EGL surface.
    pub fn surface(&self) -> &WindowSurface {
        &self.surface
    }

    /// Rebuilds the `EGLSurface` from the window this binding already owns.
    ///
    /// Needed when the EGL display was re-initialized: the old `EGLSurface`
    /// belonged to the terminated display and cannot be used again, but the
    /// `ANativeWindow` is still ours. The old surface is forgotten (never handed
    /// back to EGL) and the new one is created before the old is dropped, so the
    /// "surface destroyed before its window is released" invariant holds.
    pub fn recreate_surface(&mut self, display: &Display, config: &Config) -> Result<()> {
        let fresh = WindowSurface::create(display, config, &self.window)?;
        let mut stale = core::mem::replace(&mut self.surface, fresh);
        stale.disarm();
        Ok(())
    }

    /// Mutable access, for resize queries.
    pub fn surface_mut(&mut self) -> &mut WindowSurface {
        &mut self.surface
    }

    /// The owned window reference.
    pub fn window(&self) -> &OwnedNativeWindow {
        &self.window
    }

    /// Monotonic binding id; input events and presents are fenced against it so
    /// a stale surface can never be presented to.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Surface size as EGL sees it.
    pub fn size(&self) -> SurfaceSize {
        self.surface.size()
    }
}
