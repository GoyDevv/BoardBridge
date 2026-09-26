// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! States and events of the surface lifecycle.

/// State of the Surface → `ANativeWindow` → `EGLSurface` binding.
///
/// The machine tracks two *independent* facts about the app:
///
/// * the **surface binding** — this enum;
/// * the **activity pause state** — [`ActivityState`].
///
/// They are separate because Android delivers them separately: `onPause()` and
/// `surfaceDestroyed()` are distinct callbacks and either can arrive first
/// (backgrounding, rotation, multi-window, folding devices all differ). Trying
/// to fold the pause flag into the surface state is what makes naive bridges
/// drop a surface or leak a window reference. [`Lifecycle::reported_state`]
/// composes the two into exactly the seven names the design calls for:
/// `NO_SURFACE`, `SURFACE_PENDING`, `SURFACE_ACTIVE`, `SURFACE_DESTROY_PENDING`,
/// `PAUSED`, `STOPPING`, `STOPPED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceState {
    /// Nothing is bound and nothing has been requested.
    NoSurface,
    /// `surfaceCreated` arrived; the window has been acquired and handed to the
    /// bridge thread, which has not finished binding it yet.
    SurfacePending,
    /// `EGLSurface` created, `ANativeWindow` owned by the bridge thread.
    SurfaceActive,
    /// `surfaceDestroyed` arrived; the binding is being torn down
    /// (EGLSurface destroyed *before* the window reference is released).
    SurfaceDestroyPending,
    /// Shutdown requested; the bridge thread is draining, then releases
    /// everything.
    Stopping,
    /// The runtime no longer exists.
    Stopped,
}

impl SurfaceState {
    /// Uppercase name used in logs and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            SurfaceState::NoSurface => "NO_SURFACE",
            SurfaceState::SurfacePending => "SURFACE_PENDING",
            SurfaceState::SurfaceActive => "SURFACE_ACTIVE",
            SurfaceState::SurfaceDestroyPending => "SURFACE_DESTROY_PENDING",
            SurfaceState::Stopping => "STOPPING",
            SurfaceState::Stopped => "STOPPED",
        }
    }

    /// `true` once the runtime is shutting down or gone.
    pub fn is_shutdown(self) -> bool {
        matches!(self, SurfaceState::Stopping | SurfaceState::Stopped)
    }

    /// `true` while a surface exists or is being created/torn down, i.e. while
    /// a window reference may be held somewhere in the bridge.
    pub fn may_hold_window(self) -> bool {
        matches!(
            self,
            SurfaceState::SurfacePending
                | SurfaceState::SurfaceActive
                | SurfaceState::SurfaceDestroyPending
        )
    }
}

/// Foreground/background state of the hosting activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityState {
    /// Activity resumed.
    Resumed,
    /// Activity paused. Note that the Surface usually survives `onPause()` and
    /// is destroyed separately.
    Paused,
}

impl ActivityState {
    /// Uppercase name used in logs and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            ActivityState::Resumed => "RESUMED",
            ActivityState::Paused => "PAUSED",
        }
    }
}

/// Something that can move the lifecycle machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// Kotlin `surfaceCreated(holder.surface)`.
    SurfaceCreated,
    /// The bridge thread finished binding the window (success).
    SurfaceBound,
    /// The bridge thread failed to bind the window.
    SurfaceBindFailed,
    /// Kotlin `surfaceChanged(...)`.
    SurfaceChanged,
    /// Kotlin `surfaceDestroyed()`.
    SurfaceDestroyed,
    /// The bridge thread finished tearing the binding down.
    SurfaceUnbound,
    /// Kotlin `onPause()`.
    Pause,
    /// Kotlin `onResume()`.
    Resume,
    /// `destroyRuntime()`.
    StopRequested,
    /// The bridge thread has exited.
    Stopped,
}

impl LifecycleEvent {
    /// Name used in logs.
    pub fn name(self) -> &'static str {
        match self {
            LifecycleEvent::SurfaceCreated => "surfaceCreated",
            LifecycleEvent::SurfaceBound => "surfaceBound",
            LifecycleEvent::SurfaceBindFailed => "surfaceBindFailed",
            LifecycleEvent::SurfaceChanged => "surfaceChanged",
            LifecycleEvent::SurfaceDestroyed => "surfaceDestroyed",
            LifecycleEvent::SurfaceUnbound => "surfaceUnbound",
            LifecycleEvent::Pause => "pause",
            LifecycleEvent::Resume => "resume",
            LifecycleEvent::StopRequested => "stopRequested",
            LifecycleEvent::Stopped => "stopped",
        }
    }
}
