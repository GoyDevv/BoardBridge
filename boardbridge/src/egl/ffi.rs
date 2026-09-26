// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! EGL FFI declarations.
//!
//! Only what the bridge uses. Values are taken from the Khronos `EGL/egl.h` and
//! `EGL/eglext.h` headers (Apache-2.0/MIT dual licensed) shipped with the NDK;
//! the Rust types mirror the C ones exactly:
//!
//! | C | Rust |
//! |---|---|
//! | `EGLDisplay`, `EGLConfig`, `EGLContext`, `EGLSurface` | `*mut c_void` |
//! | `EGLint` | `i32` |
//! | `EGLBoolean` | `u32` |
//! | `EGLNativeWindowType` (`ANativeWindow*`) | `*mut c_void` |
//!
//! EGL handles are opaque and, per the specification, may be used from any
//! thread as long as a context is not current on two threads at once. The bridge
//! enforces that rule with its own state (see `docs/THREADING.md`); the handles
//! themselves are therefore `Send`-transparent, which is why the raw-pointer
//! wrappers in this module can be marked `Send` with a written safety argument.

use core::ffi::c_void;

// `c_char` is only used by the declarations below, which are Android-only.
#[cfg(target_os = "android")]
use core::ffi::c_char;

/// Opaque `EGLDisplay`.
pub type EGLDisplay = *mut c_void;
/// Opaque `EGLConfig`.
pub type EGLConfig = *mut c_void;
/// Opaque `EGLContext`.
pub type EGLContext = *mut c_void;
/// Opaque `EGLSurface`.
pub type EGLSurface = *mut c_void;
/// `EGLNativeWindowType`: an `ANativeWindow*` on Android.
pub type EGLNativeWindow = *mut c_void;
/// `EGLint`.
pub type EGLint = i32;
/// `EGLBoolean`.
pub type EGLBoolean = u32;
/// `EGLenum`.
pub type EGLenum = u32;

/// `EGL_FALSE`.
pub const EGL_FALSE: EGLBoolean = 0;
/// `EGL_TRUE`.
pub const EGL_TRUE: EGLBoolean = 1;

/// `EGL_DEFAULT_DISPLAY`.
pub const EGL_DEFAULT_DISPLAY: *mut c_void = core::ptr::null_mut();
/// `EGL_NO_DISPLAY`.
pub const EGL_NO_DISPLAY: EGLDisplay = core::ptr::null_mut();
/// `EGL_NO_CONTEXT`.
pub const EGL_NO_CONTEXT: EGLContext = core::ptr::null_mut();
/// `EGL_NO_SURFACE`.
pub const EGL_NO_SURFACE: EGLSurface = core::ptr::null_mut();

// Configuration attributes.
/// `EGL_ALPHA_SIZE`.
pub const EGL_ALPHA_SIZE: EGLint = 0x3021;
/// `EGL_BLUE_SIZE`.
pub const EGL_BLUE_SIZE: EGLint = 0x3022;
/// `EGL_GREEN_SIZE`.
pub const EGL_GREEN_SIZE: EGLint = 0x3023;
/// `EGL_RED_SIZE`.
pub const EGL_RED_SIZE: EGLint = 0x3024;
/// `EGL_DEPTH_SIZE`.
pub const EGL_DEPTH_SIZE: EGLint = 0x3025;
/// `EGL_STENCIL_SIZE`.
pub const EGL_STENCIL_SIZE: EGLint = 0x3026;
/// `EGL_CONFIG_CAVEAT`.
pub const EGL_CONFIG_CAVEAT: EGLint = 0x3027;
/// `EGL_CONFIG_ID`.
pub const EGL_CONFIG_ID: EGLint = 0x3028;
/// `EGL_MAX_PBUFFER_WIDTH`.
pub const EGL_MAX_PBUFFER_WIDTH: EGLint = 0x302C;
/// `EGL_MAX_PBUFFER_HEIGHT`.
pub const EGL_MAX_PBUFFER_HEIGHT: EGLint = 0x302A;
/// `EGL_NATIVE_RENDERABLE`.
pub const EGL_NATIVE_RENDERABLE: EGLint = 0x302D;
/// `EGL_NATIVE_VISUAL_ID`.
pub const EGL_NATIVE_VISUAL_ID: EGLint = 0x302E;
/// `EGL_NATIVE_VISUAL_TYPE`.
pub const EGL_NATIVE_VISUAL_TYPE: EGLint = 0x302F;
/// `EGL_SAMPLES`.
pub const EGL_SAMPLES: EGLint = 0x3031;
/// `EGL_SAMPLE_BUFFERS`.
pub const EGL_SAMPLE_BUFFERS: EGLint = 0x3032;
/// `EGL_SURFACE_TYPE`.
pub const EGL_SURFACE_TYPE: EGLint = 0x3033;
/// `EGL_NONE`.
pub const EGL_NONE: EGLint = 0x3038;
/// `EGL_RENDERABLE_TYPE`.
pub const EGL_RENDERABLE_TYPE: EGLint = 0x3040;

// Surface type bits.
/// `EGL_PBUFFER_BIT`.
pub const EGL_PBUFFER_BIT: EGLint = 0x0001;
/// `EGL_WINDOW_BIT`.
pub const EGL_WINDOW_BIT: EGLint = 0x0004;

// Renderable type bits.
/// `EGL_OPENGL_ES_BIT`.
pub const EGL_OPENGL_ES_BIT: EGLint = 0x0001;
/// `EGL_OPENGL_ES2_BIT`.
pub const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
/// `EGL_OPENGL_ES3_BIT` (`EGL_OPENGL_ES3_BIT_KHR`).
pub const EGL_OPENGL_ES3_BIT: EGLint = 0x0040;

// Query attributes.
/// `EGL_VENDOR`.
pub const EGL_VENDOR: EGLint = 0x3053;
/// `EGL_VERSION`.
pub const EGL_VERSION: EGLint = 0x3054;
/// `EGL_EXTENSIONS`.
pub const EGL_EXTENSIONS: EGLint = 0x3055;
/// `EGL_HEIGHT`.
pub const EGL_HEIGHT: EGLint = 0x3056;
/// `EGL_WIDTH`.
pub const EGL_WIDTH: EGLint = 0x3057;
/// `EGL_LARGEST_PBUFFER`.
pub const EGL_LARGEST_PBUFFER: EGLint = 0x3058;
/// `EGL_CONFIGS`.
pub const EGL_CONFIGS: EGLint = 0x3042;
/// `EGL_CLIENT_APIS`.
pub const EGL_CLIENT_APIS: EGLint = 0x308D;

// Context attributes.
/// `EGL_CONTEXT_CLIENT_VERSION` (also the value of `EGL_CONTEXT_MAJOR_VERSION`).
pub const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
/// `EGL_CONTEXT_MAJOR_VERSION` (`EGL_CONTEXT_MAJOR_VERSION_KHR`).
pub const EGL_CONTEXT_MAJOR_VERSION: EGLint = 0x3098;
/// `EGL_CONTEXT_MINOR_VERSION` (`EGL_CONTEXT_MINOR_VERSION_KHR`).
pub const EGL_CONTEXT_MINOR_VERSION: EGLint = 0x30FB;

// Client APIs.
/// `EGL_OPENGL_ES_API`.
pub const EGL_OPENGL_ES_API: EGLenum = 0x30A0;

// `eglGetCurrentSurface` argument.
/// `EGL_DRAW`.
pub const EGL_DRAW: EGLint = 0x3059;
/// `EGL_READ`.
pub const EGL_READ: EGLint = 0x305A;

// Errors.
/// `EGL_SUCCESS`.
pub const EGL_SUCCESS: EGLint = 0x3000;
/// `EGL_NOT_INITIALIZED`.
pub const EGL_NOT_INITIALIZED: EGLint = 0x3001;
/// `EGL_BAD_ACCESS`.
pub const EGL_BAD_ACCESS: EGLint = 0x3002;
/// `EGL_BAD_ALLOC`.
pub const EGL_BAD_ALLOC: EGLint = 0x3003;
/// `EGL_BAD_ATTRIBUTE`.
pub const EGL_BAD_ATTRIBUTE: EGLint = 0x3004;
/// `EGL_BAD_CONFIG`.
pub const EGL_BAD_CONFIG: EGLint = 0x3005;
/// `EGL_BAD_CONTEXT`.
pub const EGL_BAD_CONTEXT: EGLint = 0x3006;
/// `EGL_BAD_CURRENT_SURFACE`.
pub const EGL_BAD_CURRENT_SURFACE: EGLint = 0x3007;
/// `EGL_BAD_DISPLAY`.
pub const EGL_BAD_DISPLAY: EGLint = 0x3008;
/// `EGL_BAD_MATCH`.
pub const EGL_BAD_MATCH: EGLint = 0x3009;
/// `EGL_BAD_NATIVE_PIXMAP`.
pub const EGL_BAD_NATIVE_PIXMAP: EGLint = 0x300A;
/// `EGL_BAD_NATIVE_WINDOW`.
pub const EGL_BAD_NATIVE_WINDOW: EGLint = 0x300B;
/// `EGL_BAD_PARAMETER`.
pub const EGL_BAD_PARAMETER: EGLint = 0x300C;
/// `EGL_BAD_SURFACE`.
pub const EGL_BAD_SURFACE: EGLint = 0x300D;
/// `EGL_CONTEXT_LOST`.
pub const EGL_CONTEXT_LOST: EGLint = 0x300E;

// The declarations are Android-only: nothing on a host build may call them, and
// gating them keeps `cargo test` linkable without a libEGL. The constants above
// are shared, which is why this file is not Android-gated as a whole.
#[cfg(target_os = "android")]
#[link(name = "EGL")]
extern "C" {
    /// Returns the display for `EGL_DEFAULT_DISPLAY`.
    pub fn eglGetDisplay(display_id: *mut c_void) -> EGLDisplay;
    /// Initializes the display; reports the EGL version in the out params.
    pub fn eglInitialize(display: EGLDisplay, major: *mut EGLint, minor: *mut EGLint)
        -> EGLBoolean;
    /// Releases the resources of a display.
    pub fn eglTerminate(display: EGLDisplay) -> EGLBoolean;
    /// Queries a display string; null when the attribute is not available.
    pub fn eglQueryString(display: EGLDisplay, name: EGLint) -> *const c_char;
    /// Returns and clears the last EGL error.
    pub fn eglGetError() -> EGLint;
    /// Returns the available configurations.
    pub fn eglGetConfigs(
        display: EGLDisplay,
        configs: *mut EGLConfig,
        config_size: EGLint,
        num_config: *mut EGLint,
    ) -> EGLBoolean;
    /// Picks configurations matching `attrib_list`.
    pub fn eglChooseConfig(
        display: EGLDisplay,
        attrib_list: *const EGLint,
        configs: *mut EGLConfig,
        config_size: EGLint,
        num_config: *mut EGLint,
    ) -> EGLBoolean;
    /// Reads one configuration attribute.
    pub fn eglGetConfigAttrib(
        display: EGLDisplay,
        config: EGLConfig,
        attribute: EGLint,
        value: *mut EGLint,
    ) -> EGLBoolean;
    /// Binds a client API for the calling thread.
    pub fn eglBindAPI(api: EGLenum) -> EGLBoolean;
    /// Returns the client API bound to the calling thread.
    pub fn eglQueryAPI() -> EGLenum;
    /// Creates a rendering context.
    pub fn eglCreateContext(
        display: EGLDisplay,
        config: EGLConfig,
        share_context: EGLContext,
        attrib_list: *const EGLint,
    ) -> EGLContext;
    /// Destroys a context.
    pub fn eglDestroyContext(display: EGLDisplay, context: EGLContext) -> EGLBoolean;
    /// Makes a context current on the calling thread.
    pub fn eglMakeCurrent(
        display: EGLDisplay,
        draw: EGLSurface,
        read: EGLSurface,
        context: EGLContext,
    ) -> EGLBoolean;
    /// Returns the context current on the calling thread.
    pub fn eglGetCurrentContext() -> EGLContext;
    /// Returns the draw/read surface current on the calling thread.
    pub fn eglGetCurrentSurface(readdraw: EGLint) -> EGLSurface;
    /// Creates an `EGLSurface` bound to a native window.
    pub fn eglCreateWindowSurface(
        display: EGLDisplay,
        config: EGLConfig,
        window: EGLNativeWindow,
        attrib_list: *const EGLint,
    ) -> EGLSurface;
    /// Creates an offscreen `EGLSurface`.
    pub fn eglCreatePbufferSurface(
        display: EGLDisplay,
        config: EGLConfig,
        attrib_list: *const EGLint,
    ) -> EGLSurface;
    /// Destroys a surface.
    pub fn eglDestroySurface(display: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
    /// Reads a surface attribute.
    pub fn eglQuerySurface(
        display: EGLDisplay,
        surface: EGLSurface,
        attribute: EGLint,
        value: *mut EGLint,
    ) -> EGLBoolean;
    /// Presents the back buffer.
    pub fn eglSwapBuffers(display: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
    /// Sets the swap interval (1 = vsync) for the calling thread.
    pub fn eglSwapInterval(display: EGLDisplay, interval: EGLint) -> EGLBoolean;
    /// Sets a surface attribute.
    pub fn eglSurfaceAttrib(
        display: EGLDisplay,
        surface: EGLSurface,
        attribute: EGLint,
        value: EGLint,
    ) -> EGLBoolean;
    /// Releases EGL state belonging to the calling thread.
    pub fn eglReleaseThread() -> EGLBoolean;
}
