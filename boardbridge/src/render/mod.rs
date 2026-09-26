// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Rendering helpers for the bridge's own loop.
//!
//! The bridge does not render the game. It provides two loop arrangements, and
//! this module supplies the *content* the bridge itself draws in the first one:
//!
//! | Mode | Who drives the loop | What draws |
//! |---|---|---|
//! | [`crate::runtime::LoopMode::Internal`] | the bridge thread | [`diagnostics::DiagnosticRenderer`] |
//! | [`crate::runtime::LoopMode::Inverted`] | the game thread (Minecraft) | the game |
//!
//! The diagnostic renderer keeps the two capabilities the old demo had that are
//! genuinely useful — proof that pixels reached the framebuffer, and a shader
//! pipeline check — and drops the "spinning triangle is the product"
//! framing: the triangle is now one selectable diagnostic mode among several,
//! and the default is the cheapest possible verification (a solid clear plus a
//! centre-pixel readback that logcat can grep).

pub mod diagnostics;

pub use diagnostics::{DiagnosticMode, DiagnosticRenderer};
