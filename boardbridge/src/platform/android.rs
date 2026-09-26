// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Android-native platform backend (the implemented one).
//!
//! Windows come from `SurfaceHolder` callbacks (through
//! [`crate::runtime::Runtime::surface_created`] and friends), and input arrives
//! from Android input callbacks through the JNI entry points, already translated
//! to SDL semantics. This backend is what a consumer polls:
//!
//! ```text
//! Kotlin MotionEvent/KeyEvent
//!   → NativeBridge.sendTouchBatch/sendKey/…      (JNI)
//!   → android::input (SDL-shaped events)
//!   → EventQueue
//!   → poll_event()                                (this backend, or the diagnostic loop)
//! ```
//!
//! For the diagnostic loop the consumer is the bridge itself; for Minecraft it is
//! the game thread, which is what the SDL3/GLFW backends will call once they
//! exist. Nothing about the queue changes between the two.

use crate::error::Result;
use crate::graphics::BackendStatus;
use crate::input::{InputEvent, QueueStats};
use crate::platform::{PlatformBackend, PlatformKind, WindowInfo};
use crate::runtime::control::Shared;

/// Polls the bridge's native window/input state.
pub struct AndroidPlatformBackend;

impl PlatformBackend for AndroidPlatformBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::AndroidNative
    }

    fn name(&self) -> &'static str {
        "android-native"
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::Implemented
    }

    fn remaining_work(&self) -> Option<&'static str> {
        None
    }

    fn api_surface(&self) -> &'static str {
        "BoardBridge native bridge: Kotlin lifecycle + JNI input → SDL-shaped event queue"
    }

    fn window_info(&self, shared: &Shared) -> Result<WindowInfo> {
        let backend = shared.backend()?;
        let stats = backend.stats();
        let size = backend.window_size();
        Ok(WindowInfo {
            width: size.width,
            height: size.height,
            has_surface: stats.has_window,
            binding_generation: backend.binding_generation(),
        })
    }

    fn queue_stats(&self, shared: &Shared) -> Result<QueueStats> {
        Ok(shared.queue_stats())
    }

    fn poll_event(&self, shared: &Shared) -> Result<Option<InputEvent>> {
        Ok(shared.input_queue().pop())
    }

    fn push_event(&self, shared: &Shared, event: InputEvent) -> Result<bool> {
        Ok(shared.input_queue().push(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_documented_api_surface() {
        let backend = AndroidPlatformBackend;
        assert_eq!(backend.kind(), PlatformKind::AndroidNative);
        assert!(backend.status().is_implemented());
        assert!(backend.api_surface().contains("JNI"));
    }
}
