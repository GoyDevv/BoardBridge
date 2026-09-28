// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL display and configuration selection.

use core::ffi::CStr;
use core::ptr;

use crate::bb_debug;
use crate::bb_warn;
use crate::egl::config::ConfigRequest;
use crate::egl::ffi::{
    self, EGLConfig, EGLDisplay, EGLint, EGL_ALPHA_SIZE, EGL_CONFIG_CAVEAT, EGL_DEPTH_SIZE,
    EGL_EXTENSIONS, EGL_NATIVE_VISUAL_ID, EGL_NONE, EGL_PBUFFER_BIT, EGL_SAMPLES, EGL_STENCIL_SIZE,
    EGL_SURFACE_TYPE, EGL_TRUE, EGL_VENDOR, EGL_VERSION,
};
use crate::egl::{last_error, DisplayInfo};
use crate::error::{Error, Result};

// `ConfigRequest` lives in `crate::egl::config`: it is pure data that the
// cross-platform `graphics` layer also carries, so it must compile on hosts as
// well as on Android.

/// A chosen `EGLConfig` plus the attributes the bridge cares about.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    raw: EGLConfig,
    visual_id: EGLint,
    depth: EGLint,
    stencil: EGLint,
    alpha: EGLint,
    samples: EGLint,
    surface_type: EGLint,
    request: ConfigRequest,
}

impl Config {
    /// Raw handle for the EGL calls in [`super::surface`] and [`super::context`].
    pub fn raw(&self) -> EGLConfig {
        self.raw
    }

    /// `EGL_NATIVE_VISUAL_ID`: the `WINDOW_FORMAT_*` value the window should use.
    pub fn visual_id(&self) -> EGLint {
        self.visual_id
    }

    /// Depth bits, for diagnostics.
    pub fn depth(&self) -> EGLint {
        self.depth
    }

    /// Stencil bits, for diagnostics.
    pub fn stencil(&self) -> EGLint {
        self.stencil
    }

    /// Alpha bits, for diagnostics.
    pub fn alpha(&self) -> EGLint {
        self.alpha
    }

    /// MSAA sample count (0 when none), for diagnostics.
    pub fn samples(&self) -> EGLint {
        self.samples
    }

    /// Whether this config can back an offscreen pbuffer.
    pub fn supports_pbuffer(&self) -> bool {
        self.surface_type & EGL_PBUFFER_BIT != 0
    }

    /// The request this config was chosen for.
    pub fn request(&self) -> ConfigRequest {
        self.request
    }

    /// Summary for logs and `getRendererInfo`.
    pub fn describe(&self) -> String {
        format!(
            "visual_id=0x{:x} alpha={} depth={} stencil={} samples={}",
            self.visual_id, self.alpha, self.depth, self.stencil, self.samples
        )
    }
}

/// An initialized `EGLDisplay`.
///
/// Dropping it calls `eglTerminate`. Every context/surface created from this
/// display must be dropped *before* the display is dropped to avoid destroying a
/// display out from under a live surface; the graphics backend declares its
/// fields in that order (`docs/THREADING.md`).
#[derive(Debug)]
pub struct Display {
    raw: EGLDisplay,
    info: DisplayInfo,
}

impl Display {
    /// `eglGetDisplay(EGL_DEFAULT_DISPLAY)` + `eglInitialize`, and binds the
    /// OpenGL ES client API to the calling thread.
    pub fn initialize() -> Result<Display> {
        let raw = unsafe { ffi::eglGetDisplay(ffi::EGL_DEFAULT_DISPLAY) };
        if raw == ffi::EGL_NO_DISPLAY {
            return Err(Error::graphics("eglGetDisplay", last_error()));
        }
        let mut major: EGLint = 0;
        let mut minor: EGLint = 0;
        let ok = unsafe { ffi::eglInitialize(raw, &mut major, &mut minor) };
        if ok != EGL_TRUE {
            let code = last_error();
            // No handle to terminate: `eglInitialize` failed, the display was
            // never usable.
            return Err(Error::graphics("eglInitialize", code));
        }

        // EGL 1.4+ supports several client APIs; the bridge is GLES-only, but
        // binding explicitly keeps the state deterministic for any thread that
        // later calls into EGL.
        let bound = unsafe { ffi::eglBindAPI(ffi::EGL_OPENGL_ES_API) };
        if bound != EGL_TRUE {
            unsafe {
                ffi::eglTerminate(raw);
            }
            return Err(Error::graphics(
                "eglBindAPI(EGL_OPENGL_ES_API)",
                last_error(),
            ));
        }

        let info = DisplayInfo {
            major,
            minor,
            vendor: query_string(raw, EGL_VENDOR).unwrap_or_else(|| "?".to_string()),
            version: query_string(raw, EGL_VERSION).unwrap_or_else(|| "?".to_string()),
            client_apis: query_string(raw, ffi::EGL_CLIENT_APIS).unwrap_or_default(),
            extensions: query_string(raw, EGL_EXTENSIONS).unwrap_or_default(),
        };
        Ok(Display { raw, info })
    }

    /// Raw handle.
    pub fn raw(&self) -> EGLDisplay {
        self.raw
    }

    /// Forgets the handle *without* calling `eglTerminate`.
    ///
    /// The default EGL display is process-wide and its objects (`EGLSurface`,
    /// `EGLContext`) live in an object table owned by libEGL. Any other EGL user
    /// in the same process calling `eglTerminate` on it empties that table while
    /// the display itself stays "ready", so our handles stay non-null but become
    /// unresolvable: every later call on them fails with `EGL_BAD_SURFACE`.
    /// Once that has happened the handle must not be passed back to EGL at all —
    /// at best it produces a spurious error, inside libEGL it is a
    /// use-after-free. `disarm` is how a handle is dropped in that state; see
    /// [`crate::graphics::gles`].
    pub fn disarm(&mut self) {
        self.raw = ffi::EGL_NO_DISPLAY;
    }

    /// Version/vendor/extension information gathered at initialization.
    pub fn info(&self) -> &DisplayInfo {
        &self.info
    }

    /// `true` when the display advertises an extension.
    pub fn has_extension(&self, name: &str) -> bool {
        self.info
            .extensions
            .split_whitespace()
            .any(|ext| ext == name)
    }

    /// Chooses a config, walking the documented fallback chain:
    ///
    /// 1. the request as-is;
    /// 2. without alpha;
    /// 3. with 16-bit depth and no stencil;
    /// 4. window-only surfaces (no pbuffer support).
    ///
    /// Every failure is logged with its EGL error, so a device that ends up on
    /// step 4 is visible in logcat instead of silently degraded.
    pub fn choose_config(&self, request: &ConfigRequest) -> Result<Config> {
        let candidates = [
            ("requested", *request),
            ("no alpha", request.without_alpha()),
            ("reduced depth", request.reduced_depth()),
            ("window only", request.window_only()),
        ];

        let mut last_error_code = ffi::EGL_SUCCESS;
        for (label, candidate) in candidates {
            let mut raw: EGLConfig = ptr::null_mut();
            let mut count: EGLint = 0;
            let attribs = candidate.attribute_list();
            let ok = unsafe {
                ffi::eglChooseConfig(self.raw, attribs.as_ptr(), &mut raw, 1, &mut count)
            };
            if ok != EGL_TRUE || count < 1 || raw.is_null() {
                last_error_code = last_error();
                bb_debug!(
                    "eglChooseConfig [{label}] found no config ({})",
                    candidate.describe()
                );
                continue;
            }

            let visual_id = self.config_attrib(raw, EGL_NATIVE_VISUAL_ID, 0);
            let depth = self.config_attrib(raw, EGL_DEPTH_SIZE, 0);
            let stencil = self.config_attrib(raw, EGL_STENCIL_SIZE, 0);
            let alpha = self.config_attrib(raw, EGL_ALPHA_SIZE, 0);
            let samples = self.config_attrib(raw, EGL_SAMPLES, 0);
            let surface_type = self.config_attrib(raw, EGL_SURFACE_TYPE, 0);
            let caveat = self.config_attrib(raw, EGL_CONFIG_CAVEAT, 0);

            // `EGL_CONFIG_CAVEAT` is `EGL_NONE` (0x3038) on the configs that are
            // fine; only `EGL_SLOW_CONFIG` / `EGL_NON_CONFORMANT_CONFIG` are worth
            // reporting. Comparing against `0` meant every device logged a caveat
            // of `0x3038` — a warning that says "no caveat" and hides the real
            // ones while reading a device's logcat.
            if caveat != EGL_NONE {
                bb_warn!("eglChooseConfig [{label}] picked a config with caveat 0x{caveat:x}");
            }

            let config = Config {
                raw,
                visual_id,
                depth,
                stencil,
                alpha,
                samples,
                surface_type,
                request: candidate,
            };
            bb_debug!("eglChooseConfig [{label}] -> {}", config.describe());
            return Ok(config);
        }

        Err(Error::graphics("eglChooseConfig", last_error_code))
    }

    fn config_attrib(&self, config: EGLConfig, attribute: EGLint, default: EGLint) -> EGLint {
        let mut value: EGLint = default;
        let ok = unsafe { ffi::eglGetConfigAttrib(self.raw, config, attribute, &mut value) };
        if ok == EGL_TRUE {
            value
        } else {
            default
        }
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        if self.raw != ffi::EGL_NO_DISPLAY {
            unsafe {
                ffi::eglTerminate(self.raw);
            }
            self.raw = ffi::EGL_NO_DISPLAY;
        }
    }
}

// SAFETY: `EGLDisplay` is an opaque handle that the EGL specification allows to
// be used from any thread; the bridge serializes all use through the graphics
// backend's mutex and never makes one context current on two threads.
unsafe impl Send for Display {}

fn query_string(display: EGLDisplay, name: EGLint) -> Option<String> {
    let ptr = unsafe { ffi::eglQueryString(display, name) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: EGL returns a NUL-terminated string owned by the display, valid
    // until the display is terminated.
    let value = unsafe { CStr::from_ptr(ptr) };
    Some(value.to_string_lossy().into_owned())
}
