// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Android platform layer (compiled only for `target_os = "android"`).
//!
//! * [`ffi`] — the handful of NDK entry points the bridge needs.
//! * [`surface`] — [`surface::OwnedNativeWindow`], the single owner of an
//!   `ANativeWindow` reference, and [`surface::SurfaceSize`].
//! * [`input`] — JNI argument → SDL-shaped [`crate::input::InputEvent`].
//! * [`lifecycle`] — Kotlin callback → [`crate::lifecycle::LifecycleEvent`].
//!
//! Nothing here reaches for the Java VM. The bridge never calls back into Java
//! (no cached `jobject`, no `AttachCurrentThread`): Kotlin *pushes* everything
//! through explicit JNI entry points, which keeps the native side free of
//! global Java references and their lifetime rules.

pub mod ffi;
pub mod input;
pub mod lifecycle;
pub mod surface;
