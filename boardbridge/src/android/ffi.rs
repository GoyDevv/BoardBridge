// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Raw NDK declarations used by the Android layer.
//!
//! Only the few `<android/native_window.h>` and `<android/native_window_jni.h>`
//! entry points the bridge needs are declared here. The JNI types are kept as
//! `*mut c_void` on purpose: the Android layer then does not depend on the `jni`
//! crate's wrapper types, so the ownership rules in [`super::surface`] can be
//! reviewed (and unit-tested for the pure parts) without JNI in the picture.
//!
//! Reference counting rules, straight from the NDK documentation:
//!
//! * `ANativeWindow_fromSurface` acquires **one** reference on success; the
//!   caller owns it and must `ANativeWindow_release` it exactly once.
//! * `ANativeWindow_acquire` adds a reference; `ANativeWindow_release` removes
//!   one and frees the window when the count reaches zero.
//! * The window stays *valid* after the Java `Surface` is destroyed, but its
//!   buffers may no longer be usable: the bridge therefore never uses a window
//!   after the lifecycle machine has moved it out of the "bound" state.

use core::ffi::c_void;

/// Opaque `ANativeWindow`.
#[repr(C)]
pub struct ANativeWindow {
    _private: [u8; 0],
}

/// Opaque `JNIEnv*` (only ever passed straight back to the NDK).
pub type JniEnvPtr = *mut c_void;
/// Opaque `jobject` (a `Surface`, in this case).
pub type JObjectPtr = *mut c_void;

#[link(name = "android")]
extern "C" {
    /// Acquires one reference from a Java `Surface`; null on failure.
    pub fn ANativeWindow_fromSurface(env: JniEnvPtr, surface: JObjectPtr) -> *mut ANativeWindow;

    /// Adds one reference.
    pub fn ANativeWindow_acquire(window: *mut ANativeWindow);

    /// Removes one reference.
    pub fn ANativeWindow_release(window: *mut ANativeWindow);

    /// Current buffer width in pixels.
    pub fn ANativeWindow_getWidth(window: *mut ANativeWindow) -> i32;

    /// Current buffer height in pixels.
    pub fn ANativeWindow_getHeight(window: *mut ANativeWindow) -> i32;

    /// Current buffer format (`WINDOW_FORMAT_RGBA_8888` etc.).
    pub fn ANativeWindow_getFormat(window: *mut ANativeWindow) -> i32;

    /// Requests a buffer geometry; returns 0 on success.
    pub fn ANativeWindow_setBuffersGeometry(
        window: *mut ANativeWindow,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;
}

// `WINDOW_FORMAT_RGBA_8888`, the format an 8/8/8/8 EGL config expects.
/// `WINDOW_FORMAT_RGBA_8888`.
pub const WINDOW_FORMAT_RGBA_8888: i32 = 1;
/// `WINDOW_FORMAT_RGBX_8888`.
pub const WINDOW_FORMAT_RGBX_8888: i32 = 2;
/// `WINDOW_FORMAT_RGB_565`.
pub const WINDOW_FORMAT_RGB_565: i32 = 4;
