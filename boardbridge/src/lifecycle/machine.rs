// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! The lifecycle machine itself.

use crate::lifecycle::state::{ActivityState, LifecycleEvent, SurfaceState};

/// How the machine reacted to an event.
///
/// The distinction between [`Outcome::Ignored`] and [`Outcome::Refused`]
/// matters: Kotlin may deliver callbacks out of order or twice (backgrounding
/// during rotation is the classic case), and Android may hand the bridge a
/// window that the machine no longer wants. A *refused* event carries a
/// resource that the caller must release; an *ignored* event carries nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The machine moved (or deliberately stayed) in this state.
    Applied(SurfaceState),
    /// Benign duplicate/out-of-order event; nothing to do.
    Ignored(&'static str),
    /// The event cannot be honored; the caller must discard any resource that
    /// came with it (for example release the `ANativeWindow` of a late bind).
    Refused(&'static str),
}

impl Outcome {
    /// The state after the event (for `Applied`).
    pub fn state(self) -> Option<SurfaceState> {
        match self {
            Outcome::Applied(state) => Some(state),
            _ => None,
        }
    }

    /// `true` when the machine actually handled the event.
    pub fn is_applied(self) -> bool {
        matches!(self, Outcome::Applied(_))
    }

    /// `true` when the caller must release a resource.
    pub fn is_refused(self) -> bool {
        matches!(self, Outcome::Refused(_))
    }

    /// Short kind name, for logs.
    pub fn kind(self) -> &'static str {
        match self {
            Outcome::Applied(_) => "applied",
            Outcome::Ignored(_) => "ignored",
            Outcome::Refused(_) => "refused",
        }
    }

    /// Reason string carried by `Ignored`/`Refused`.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Outcome::Applied(_) => None,
            Outcome::Ignored(reason) | Outcome::Refused(reason) => Some(reason),
        }
    }
}

/// Surface lifecycle state machine.
///
/// Cheap to copy-free and allocation-free in the hot paths: `on` only does
/// comparisons, and the only allocating helper is [`Lifecycle::summary`],
/// which diagnostics call.
#[derive(Clone, Debug)]
pub struct Lifecycle {
    state: SurfaceState,
    activity: ActivityState,
    surface_epoch: u64,
    bound: bool,
    transitions: u64,
    last_event: &'static str,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Lifecycle::new()
    }
}

impl Lifecycle {
    /// A fresh machine: no surface, activity resumed.
    pub fn new() -> Lifecycle {
        Lifecycle {
            state: SurfaceState::NoSurface,
            activity: ActivityState::Resumed,
            surface_epoch: 0,
            bound: false,
            transitions: 0,
            last_event: "none",
        }
    }

    /// Current surface state.
    pub fn state(&self) -> SurfaceState {
        self.state
    }

    /// Current activity state.
    pub fn activity(&self) -> ActivityState {
        self.activity
    }

    /// Monotonic epoch: incremented for every accepted `surfaceCreated`. Equal
    /// to the id of the newest requested surface.
    pub fn surface_epoch(&self) -> u64 {
        self.surface_epoch
    }

    /// `true` while the graphics backend holds a live window binding.
    pub fn is_bound(&self) -> bool {
        self.bound
    }

    /// Number of accepted transitions (diagnostics).
    pub fn transitions(&self) -> u64 {
        self.transitions
    }

    /// Name of the last event that reached the machine.
    pub fn last_event(&self) -> &'static str {
        self.last_event
    }

    /// One of the seven lifecycle names from the design:
    /// `NO_SURFACE`, `SURFACE_PENDING`, `SURFACE_ACTIVE`,
    /// `SURFACE_DESTROY_PENDING`, `PAUSED`, `STOPPING`, `STOPPED`.
    ///
    /// `PAUSED` is reported whenever the activity is paused and the runtime is
    /// still alive, because that is the single most useful fact when reading
    /// logcat from a device.
    pub fn reported_state(&self) -> &'static str {
        if self.state.is_shutdown() {
            return self.state.name();
        }
        if self.activity == ActivityState::Paused {
            return "PAUSED";
        }
        self.state.name()
    }

    /// Compact `k=v` summary for diagnostics output.
    pub fn summary(&self) -> String {
        format!(
            "state={} surface={} activity={} bound={} epoch={} transitions={} last_event={}",
            self.reported_state(),
            self.state.name(),
            self.activity.name(),
            self.bound,
            self.surface_epoch,
            self.transitions,
            self.last_event
        )
    }

    /// Feeds one event into the machine.
    pub fn on(&mut self, event: LifecycleEvent) -> Outcome {
        self.last_event = event.name();

        if self.state == SurfaceState::Stopped {
            return Outcome::Ignored("runtime already stopped");
        }

        if self.state == SurfaceState::Stopping {
            return match event {
                LifecycleEvent::Stopped => {
                    self.bound = false;
                    self.apply(SurfaceState::Stopped)
                }
                LifecycleEvent::StopRequested => Outcome::Ignored("stop already requested"),
                // The bridge thread reports the teardown of the surface it still
                // held; the state stays STOPPING until the thread exits.
                LifecycleEvent::SurfaceUnbound => {
                    self.bound = false;
                    Outcome::Applied(SurfaceState::Stopping)
                }
                LifecycleEvent::Pause => {
                    self.activity = ActivityState::Paused;
                    Outcome::Applied(SurfaceState::Stopping)
                }
                LifecycleEvent::Resume => {
                    self.activity = ActivityState::Resumed;
                    Outcome::Applied(SurfaceState::Stopping)
                }
                LifecycleEvent::SurfaceDestroyed | LifecycleEvent::SurfaceChanged => {
                    Outcome::Ignored("runtime is shutting down")
                }
                LifecycleEvent::SurfaceBindFailed => Outcome::Ignored("runtime is shutting down"),
                // A window that arrives during shutdown must be released by the
                // caller: nothing will ever bind it.
                LifecycleEvent::SurfaceCreated | LifecycleEvent::SurfaceBound => {
                    Outcome::Refused("runtime is shutting down; release the window")
                }
            };
        }

        match event {
            LifecycleEvent::StopRequested => self.apply(SurfaceState::Stopping),
            LifecycleEvent::Stopped => {
                self.bound = false;
                self.apply(SurfaceState::Stopped)
            }
            LifecycleEvent::Pause => {
                if self.activity == ActivityState::Paused {
                    return Outcome::Ignored("already paused");
                }
                self.activity = ActivityState::Paused;
                let state = self.state;
                self.apply(state)
            }
            LifecycleEvent::Resume => {
                if self.activity == ActivityState::Resumed {
                    return Outcome::Ignored("already resumed");
                }
                self.activity = ActivityState::Resumed;
                let state = self.state;
                self.apply(state)
            }
            LifecycleEvent::SurfaceCreated => {
                // Accepted from ACTIVE (rotation: Android may deliver the new
                // surface before the old one is torn down) and from
                // DESTROY_PENDING (the retire is still queued). The runtime
                // queues the retire before the new bind, so the ordering
                // guarantee "EGLSurface destroyed before window released" is
                // preserved by the command queue, not by this machine.
                self.surface_epoch += 1;
                self.apply(SurfaceState::SurfacePending)
            }
            LifecycleEvent::SurfaceBound => match self.state {
                SurfaceState::SurfacePending => {
                    self.bound = true;
                    self.apply(SurfaceState::SurfaceActive)
                }
                SurfaceState::SurfaceActive => {
                    Outcome::Refused("a window is already bound; release the extra window")
                }
                _ => Outcome::Refused("bind finished without a requested surface"),
            },
            LifecycleEvent::SurfaceBindFailed => match self.state {
                SurfaceState::SurfacePending => {
                    self.bound = false;
                    self.apply(SurfaceState::NoSurface)
                }
                _ => Outcome::Ignored("bind failure without a pending surface"),
            },
            LifecycleEvent::SurfaceChanged => match self.state {
                SurfaceState::SurfacePending | SurfaceState::SurfaceActive => {
                    let state = self.state;
                    self.apply(state)
                }
                _ => Outcome::Ignored("no surface to resize"),
            },
            LifecycleEvent::SurfaceDestroyed => match self.state {
                SurfaceState::SurfaceActive => self.apply(SurfaceState::SurfaceDestroyPending),
                // Destroyed before the bind completed: drop the pending request.
                SurfaceState::SurfacePending => {
                    self.bound = false;
                    self.apply(SurfaceState::NoSurface)
                }
                _ => Outcome::Ignored("surface was already gone"),
            },
            LifecycleEvent::SurfaceUnbound => {
                let was_bound = self.bound;
                self.bound = false;
                match self.state {
                    SurfaceState::SurfaceActive | SurfaceState::SurfaceDestroyPending => {
                        self.apply(SurfaceState::NoSurface)
                    }
                    // Rotation: the new window was already accepted
                    // (`SurfaceCreated` moved us to SURFACE_PENDING) while the
                    // previous binding is only now retired. Reporting these as
                    // ignored would hide a real teardown from the caller; the
                    // state stays SURFACE_PENDING because that window is what
                    // the next bind will use. As in the STOPPING branch above,
                    // no transition is counted: the state genuinely did not
                    // change.
                    SurfaceState::SurfacePending if was_bound => {
                        Outcome::Applied(SurfaceState::SurfacePending)
                    }
                    _ => Outcome::Ignored("no bound surface to unbind"),
                }
            }
        }
    }

    fn apply(&mut self, state: SurfaceState) -> Outcome {
        self.state = state;
        self.transitions += 1;
        Outcome::Applied(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::state::{ActivityState, LifecycleEvent, SurfaceState};

    /// Drives the machine through a list of events and returns the final state.
    fn run(machine: &mut Lifecycle, events: &[LifecycleEvent]) -> SurfaceState {
        let mut state = machine.state();
        for event in events {
            let outcome = machine.on(*event);
            if let Some(next) = outcome.state() {
                state = next;
            }
        }
        state
    }

    #[test]
    fn walks_the_full_surface_cycle() {
        let mut machine = Lifecycle::new();
        assert_eq!(machine.state(), SurfaceState::NoSurface);
        assert_eq!(machine.surface_epoch(), 0);

        assert!(machine.on(LifecycleEvent::SurfaceCreated).is_applied());
        assert_eq!(machine.state(), SurfaceState::SurfacePending);
        assert_eq!(machine.surface_epoch(), 1);

        assert!(machine.on(LifecycleEvent::SurfaceBound).is_applied());
        assert_eq!(machine.state(), SurfaceState::SurfaceActive);
        assert!(machine.is_bound());

        assert!(machine.on(LifecycleEvent::SurfaceDestroyed).is_applied());
        assert_eq!(machine.state(), SurfaceState::SurfaceDestroyPending);
        assert!(
            machine.is_bound(),
            "the binding is torn down asynchronously"
        );

        assert!(machine.on(LifecycleEvent::SurfaceUnbound).is_applied());
        assert_eq!(machine.state(), SurfaceState::NoSurface);
        assert!(!machine.is_bound());
    }

    #[test]
    fn accepts_a_new_surface_while_the_old_one_is_still_being_torn_down() {
        // Rotation: Android delivers surfaceCreated for the new window while the
        // retire of the previous window is still queued.
        let mut machine = Lifecycle::new();
        run(
            &mut machine,
            &[
                LifecycleEvent::SurfaceCreated,
                LifecycleEvent::SurfaceBound,
                LifecycleEvent::SurfaceDestroyed,
            ],
        );
        assert_eq!(machine.state(), SurfaceState::SurfaceDestroyPending);

        let outcome = machine.on(LifecycleEvent::SurfaceCreated);
        assert!(
            outcome.is_applied(),
            "rotation must not be refused: {outcome:?}"
        );
        assert_eq!(machine.state(), SurfaceState::SurfacePending);
        assert_eq!(machine.surface_epoch(), 2);

        // ... and the second bind is accepted once the first one is unbound.
        assert!(machine.on(LifecycleEvent::SurfaceUnbound).is_applied());
        assert_eq!(machine.state(), SurfaceState::SurfacePending);
        assert!(machine.on(LifecycleEvent::SurfaceBound).is_applied());
        assert_eq!(machine.state(), SurfaceState::SurfaceActive);
    }

    #[test]
    fn pause_is_independent_of_the_surface_and_is_reported() {
        let mut machine = Lifecycle::new();
        run(
            &mut machine,
            &[LifecycleEvent::SurfaceCreated, LifecycleEvent::SurfaceBound],
        );
        assert_eq!(machine.reported_state(), "SURFACE_ACTIVE");

        machine.on(LifecycleEvent::Pause);
        assert_eq!(machine.activity(), ActivityState::Paused);
        assert_eq!(machine.state(), SurfaceState::SurfaceActive);
        assert_eq!(machine.reported_state(), "PAUSED");

        // A paused activity keeps churning surface callbacks: the surface is
        // destroyed on backgrounding and the bridge still reports PAUSED.
        machine.on(LifecycleEvent::SurfaceDestroyed);
        assert_eq!(machine.state(), SurfaceState::SurfaceDestroyPending);
        assert_eq!(machine.reported_state(), "PAUSED");

        // Back to the foreground with a pending teardown: the bridge reports the
        // real surface state again.
        machine.on(LifecycleEvent::Resume);
        assert_eq!(machine.reported_state(), "SURFACE_DESTROY_PENDING");

        machine.on(LifecycleEvent::SurfaceUnbound);
        assert_eq!(machine.reported_state(), "NO_SURFACE");
    }

    #[test]
    fn duplicate_pause_events_are_ignored() {
        let mut machine = Lifecycle::new();
        run(&mut machine, &[LifecycleEvent::Pause]);
        let outcome = machine.on(LifecycleEvent::Pause);
        assert!(!outcome.is_applied());
        assert_eq!(outcome.reason(), Some("already paused"));
    }

    #[test]
    fn duplicate_surface_destroyed_events_are_ignored() {
        let mut machine = Lifecycle::new();
        run(
            &mut machine,
            &[
                LifecycleEvent::SurfaceCreated,
                LifecycleEvent::SurfaceBound,
                LifecycleEvent::SurfaceDestroyed,
            ],
        );
        let outcome = machine.on(LifecycleEvent::SurfaceDestroyed);
        assert!(!outcome.is_applied());
        assert!(matches!(outcome, Outcome::Ignored(_)));
    }

    #[test]
    fn refuses_surplus_window_bindings_so_the_caller_releases_them() {
        let mut machine = Lifecycle::new();
        let outcome = machine.on(LifecycleEvent::SurfaceBound);
        assert!(
            outcome.is_refused(),
            "a bind without a request must be refused"
        );

        run(
            &mut machine,
            &[LifecycleEvent::SurfaceCreated, LifecycleEvent::SurfaceBound],
        );
        let again = machine.on(LifecycleEvent::SurfaceBound);
        assert!(
            again.is_refused(),
            "a second window must be released, not installed"
        );
    }

    #[test]
    fn failed_bind_returns_to_no_surface() {
        let mut machine = Lifecycle::new();
        run(&mut machine, &[LifecycleEvent::SurfaceCreated]);
        machine.on(LifecycleEvent::SurfaceBindFailed);
        assert_eq!(machine.state(), SurfaceState::NoSurface);
        assert!(!machine.is_bound());
        assert_eq!(
            machine.surface_epoch(),
            1,
            "the epoch still identifies the surface"
        );
    }

    #[test]
    fn shutdown_keeps_torn_down_resources_and_refuses_new_windows() {
        let mut machine = Lifecycle::new();
        run(
            &mut machine,
            &[
                LifecycleEvent::SurfaceCreated,
                LifecycleEvent::SurfaceBound,
                LifecycleEvent::StopRequested,
            ],
        );
        assert_eq!(machine.state(), SurfaceState::Stopping);
        assert_eq!(machine.reported_state(), "STOPPING");

        // A late surfaceCreated during shutdown must be released by the caller.
        assert!(machine.on(LifecycleEvent::SurfaceCreated).is_refused());

        // The bridge thread finishes its teardown but the machine stays STOPPING.
        let unbound = machine.on(LifecycleEvent::SurfaceUnbound);
        assert!(unbound.is_applied());
        assert_eq!(machine.state(), SurfaceState::Stopping);
        assert!(!machine.is_bound());

        assert!(machine.state().is_shutdown());
        run(&mut machine, &[LifecycleEvent::Stopped]);
        assert_eq!(machine.state(), SurfaceState::Stopped);
        assert_eq!(machine.reported_state(), "STOPPED");

        // Everything after STOPPED is ignored, never a panic.
        for event in [
            LifecycleEvent::SurfaceCreated,
            LifecycleEvent::SurfaceBound,
            LifecycleEvent::Pause,
            LifecycleEvent::Resume,
            LifecycleEvent::StopRequested,
        ] {
            let outcome = machine.on(event);
            assert!(!outcome.is_applied());
            assert_eq!(outcome.reason(), Some("runtime already stopped"));
        }
    }

    #[test]
    fn reported_states_cover_the_seven_design_names() {
        let mut machine = Lifecycle::new();
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::Pause);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::Resume);
        machine.on(LifecycleEvent::SurfaceCreated);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::SurfaceBound);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::SurfaceDestroyed);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::SurfaceUnbound);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::StopRequested);
        seen.insert(machine.reported_state());

        machine.on(LifecycleEvent::Stopped);
        seen.insert(machine.reported_state());

        // `reported_state` returns `&'static str`, so the set is a set of
        // string slices — the expected names are spelled out here to catch a
        // rename in either direction.
        let expected: std::collections::BTreeSet<&str> = [
            "NO_SURFACE",
            "PAUSED",
            "STOPPED",
            "STOPPING",
            "SURFACE_ACTIVE",
            "SURFACE_DESTROY_PENDING",
            "SURFACE_PENDING",
        ]
        .into_iter()
        .collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn summary_is_log_friendly() {
        let mut machine = Lifecycle::new();
        machine.on(LifecycleEvent::SurfaceCreated);
        let summary = machine.summary();
        assert!(summary.contains("state=SURFACE_PENDING"));
        assert!(summary.contains("epoch=1"));
        assert!(summary.contains("last_event=surfaceCreated"));
    }
}
