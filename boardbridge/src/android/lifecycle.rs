// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Kotlin callback → lifecycle event mapping.
//!
//! Kept as its own tiny module so there is exactly one place that knows how the
//! Kotlin shell's callbacks feed the (pure) state machine in
//! [`crate::lifecycle`]. The JNI layer looks up the mapping here instead of
//! inventing events inline.
//!
//! Ordering facts the mapping relies on (Android behavior, not bridge policy):
//!
//! | Callback pair | Order | Notes |
//! |---|---|---|
//! | `onPause` / `surfaceDestroyed` | either first | Backgrounding, rotation, fold and multi-window all differ; the machine treats them as independent. |
//! | `surfaceCreated` (new surface) / `surfaceDestroyed` (old surface) | `surfaceDestroyed` may arrive *after* the new `surfaceCreated` | The runtime queues the retire before the new bind, so the ordering guarantee holds regardless. |
//! | `surfaceChanged` | always after `surfaceCreated`, may repeat | Resizes are advisory: EGL query surfaces are the source of truth for size. |

use crate::lifecycle::LifecycleEvent;

/// The three `SurfaceHolder.Callback` entry points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceCallback {
    /// `surfaceCreated(holder)`.
    Created,
    /// `surfaceChanged(holder, format, width, height)`.
    Changed,
    /// `surfaceDestroyed(holder)`.
    Destroyed,
}

impl SurfaceCallback {
    /// Lifecycle event this callback produces.
    pub fn event(self) -> LifecycleEvent {
        match self {
            SurfaceCallback::Created => LifecycleEvent::SurfaceCreated,
            SurfaceCallback::Changed => LifecycleEvent::SurfaceChanged,
            SurfaceCallback::Destroyed => LifecycleEvent::SurfaceDestroyed,
        }
    }

    /// Kotlin method name, for logs.
    pub fn name(self) -> &'static str {
        match self {
            SurfaceCallback::Created => "surfaceCreated",
            SurfaceCallback::Changed => "surfaceChanged",
            SurfaceCallback::Destroyed => "surfaceDestroyed",
        }
    }
}

/// The activity callbacks that matter to the bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityCallback {
    /// `onResume()`.
    Resume,
    /// `onPause()`.
    Pause,
}

impl ActivityCallback {
    /// Lifecycle event this callback produces.
    pub fn event(self) -> LifecycleEvent {
        match self {
            ActivityCallback::Resume => LifecycleEvent::Resume,
            ActivityCallback::Pause => LifecycleEvent::Pause,
        }
    }

    /// Kotlin method name, for logs.
    pub fn name(self) -> &'static str {
        match self {
            ActivityCallback::Resume => "onResume",
            ActivityCallback::Pause => "onPause",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callbacks_map_to_the_expected_events() {
        assert_eq!(
            SurfaceCallback::Created.event(),
            LifecycleEvent::SurfaceCreated
        );
        assert_eq!(
            SurfaceCallback::Changed.event(),
            LifecycleEvent::SurfaceChanged
        );
        assert_eq!(
            SurfaceCallback::Destroyed.event(),
            LifecycleEvent::SurfaceDestroyed
        );
        assert_eq!(ActivityCallback::Resume.event(), LifecycleEvent::Resume);
        assert_eq!(ActivityCallback::Pause.event(), LifecycleEvent::Pause);
        assert_eq!(SurfaceCallback::Created.name(), "surfaceCreated");
    }
}
