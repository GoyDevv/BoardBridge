// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Lifecycle state machine.
//!
//! This module is *pure logic*: no EGL, no JNI, no Android types. That is
//! deliberate — the surface/lifecycle rules are the part of the bridge that is
//! easiest to get subtly wrong (rotation, backgrounding, race between
//! `surfaceCreated`/`surfaceDestroyed`), so they are isolated here and covered
//! by unit tests that run on any machine (`cargo test`).
//!
//! The runtime (`crate::runtime`) and the graphics backends
//! (`crate::graphics`) are the callers; they must obey the outcomes returned by
//! [`Lifecycle::on`].

mod machine;
mod state;

pub use machine::{Lifecycle, Outcome};
pub use state::{ActivityState, LifecycleEvent, SurfaceState};
