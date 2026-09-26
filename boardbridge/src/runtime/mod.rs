// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! The runtime: the object that owns everything for one bridge lifetime.
//!
//! ```text
//! createRuntime()  → registry (one Runtime per process) → bridge thread
//!                        │                                  │
//!   Kotlin (UI thread) ──┤ commands + lifecycle events ──────┤ owns EGL
//!   game thread ─────────┤ attach / present ────────────────-┘
//! ```
//!
//! * [`config`] — what Kotlin can configure (pure, unit-tested).
//! * `control` — the bridge thread, the command queue and the surface fences
//!   (Android-only).
//! * `registry` — the process-wide handle the JNI layer talks to
//!   (Android-only).
//!
//! # The one rule that makes the rest work
//!
//! Kotlin's lifecycle callbacks *describe Android's view of the world*, and the
//! bridge thread *reports what EGL has actually done*. Both feed the same
//! [`crate::lifecycle::Lifecycle`]:
//!
//! | Event | Pushed by | Meaning |
//! |---|---|---|
//! | `SurfaceCreated`, `SurfaceChanged`, `SurfaceDestroyed` | JNI thread | Android says a window exists / changed / went away |
//! | `SurfaceBound`, `SurfaceBindFailed`, `SurfaceUnbound` | bridge thread | EGL has bound / failed / released |
//! | `Pause`, `Resume` | JNI thread | activity visibility |
//! | `StopRequested`, `Stopped` | JNI thread / bridge thread | runtime shutdown |
//!
//! That split is what lets `surfaceDestroyed` return to Android quickly while
//! still guaranteeing that no thread touches a dead window: the *release* is the
//! bridge thread's job, and it happens (or is safely deferred) regardless of how
//! long the UI thread waited.

pub mod config;

#[cfg(target_os = "android")]
pub mod control;
#[cfg(target_os = "android")]
pub mod registry;

pub use config::{
    LoopMode, RuntimeConfig, FLAG_DIAGNOSTIC_LOGS, FLAG_PRESERVE_CONTEXT, FLAG_VSYNC,
};

#[cfg(target_os = "android")]
pub use control::Runtime;
#[cfg(target_os = "android")]
pub use registry::{create, destroy, exists, with_runtime};

/// Version of the Kotlin ↔ Rust contract (entry points, argument order, error
/// codes). Bumped whenever the JNI surface changes; reported by `getStatus` so a
/// mismatched APK/`.so` pair is obvious in logcat.
pub const ABI_VERSION: u32 = 2;
