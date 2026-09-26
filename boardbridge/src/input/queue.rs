// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Bounded input queue.
//!
//! Producers are Android input callbacks (the UI thread) plus the runtime
//! itself for lifecycle notices; the consumer is whichever loop owns input —
//! the diagnostic renderer or the game thread.
//!
//! Properties that matter here:
//!
//! * **Bounded**: a game that stops polling (blocked in chunk generation, or
//!   simply paused) must not grow the queue without limit.
//! * **Drop-oldest**: when the cap is hit the *oldest* event is discarded, and
//!   the number of dropped events is counted. The newest input is the most
//!   valuable for a game (a stale `move` is worthless), and the counter makes
//!   the loss visible in diagnostics instead of silent.
//! * **No allocation per frame**: consumers drain into a caller-owned `Vec`
//!   ([`EventQueue::drain_into`]) instead of collecting into a fresh one.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::input::event::InputEvent;

/// Default cap: 4096 events. A 60 Hz game that polls every frame will never see
/// this; it exists for the case where polling stops entirely.
pub const DEFAULT_CAPACITY: usize = 4096;

/// Counters describing queue usage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueStats {
    /// Events currently waiting.
    pub depth: usize,
    /// Configured cap.
    pub capacity: usize,
    /// Events accepted since creation.
    pub pushed: u64,
    /// Events removed by consumers since creation.
    pub popped: u64,
    /// Events discarded because the queue was full.
    pub dropped: u64,
}

#[derive(Debug, Default)]
struct Inner {
    events: VecDeque<InputEvent>,
    pushed: u64,
    popped: u64,
    dropped: u64,
}

/// Thread-safe bounded queue.
#[derive(Debug)]
pub struct EventQueue {
    inner: Mutex<Inner>,
    capacity: usize,
}

impl EventQueue {
    /// Creates a queue with the given cap (clamped to at least 1).
    pub fn new(capacity: usize) -> EventQueue {
        EventQueue {
            inner: Mutex::new(Inner::default()),
            capacity: capacity.max(1),
        }
    }

    /// Configured cap.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Appends one event, dropping the oldest if the queue is full.
    ///
    /// Returns `true` when the event was appended without evicting an older one.
    pub fn push(&self, event: InputEvent) -> bool {
        let mut inner = self.lock();
        let mut evicted = false;
        while inner.events.len() >= self.capacity {
            inner.events.pop_front();
            inner.dropped += 1;
            evicted = true;
        }
        inner.events.push_back(event);
        inner.pushed += 1;
        !evicted
    }

    /// Removes and returns the oldest event.
    pub fn pop(&self) -> Option<InputEvent> {
        let mut inner = self.lock();
        let event = inner.events.pop_front();
        if event.is_some() {
            inner.popped += 1;
        }
        event
    }

    /// Moves up to `max` events into `out` (which is not cleared), returning the
    /// number moved. `out` is expected to be reused by the caller.
    pub fn drain_into(&self, out: &mut Vec<InputEvent>, max: usize) -> usize {
        let mut inner = self.lock();
        let mut moved = 0;
        while moved < max {
            match inner.events.pop_front() {
                Some(event) => {
                    out.push(event);
                    inner.popped += 1;
                    moved += 1;
                }
                None => break,
            }
        }
        moved
    }

    /// Number of waiting events.
    pub fn len(&self) -> usize {
        self.lock().events.len()
    }

    /// `true` when nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.lock().events.is_empty()
    }

    /// Discards everything (used when the surface goes away: stale touch
    /// coordinates are meaningless for the new window).
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.events.clear();
    }

    /// Counter snapshot for diagnostics.
    pub fn stats(&self) -> QueueStats {
        let inner = self.lock();
        QueueStats {
            depth: inner.events.len(),
            capacity: self.capacity,
            pushed: inner.pushed,
            popped: inner.popped,
            dropped: inner.dropped,
        }
    }

    /// Drops any queued lifecycle notice, used before pushing a fresh one so the
    /// game cannot observe an outdated state.
    pub fn remove_lifecycle_notices(&self) {
        let mut inner = self.lock();
        inner.events.retain(|event| !matches!(event, InputEvent::Lifecycle(_)));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned queue must not take the whole app down: input is not worth
        // aborting for. `unwrap_or_else` recovers the inner value.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        EventQueue::new(DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{DeviceKind, InputEvent, TouchPhase};

    fn touch(timestamp_ms: i64) -> InputEvent {
        InputEvent::Touch {
            pointer_id: 0,
            phase: TouchPhase::Move,
            x: 1.0,
            y: 2.0,
            pressure: 0.5,
            timestamp_ms,
        }
    }

    #[test]
    fn preserves_fifo_order() {
        let queue = EventQueue::new(4);
        for index in 0..3 {
            queue.push(touch(index));
        }
        assert_eq!(queue.len(), 3);
        for index in 0..3 {
            assert_eq!(queue.pop().unwrap().timestamp_ms(), index);
        }
        assert!(queue.is_empty());
    }

    #[test]
    fn drops_oldest_when_full_and_counts_it() {
        let queue = EventQueue::new(3);
        for index in 0..5 {
            queue.push(touch(index));
        }
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.stats().dropped, 2);
        assert_eq!(queue.pop().unwrap().timestamp_ms(), 2, "oldest events are evicted");
        assert_eq!(queue.pop().unwrap().timestamp_ms(), 3);
        assert_eq!(queue.pop().unwrap().timestamp_ms(), 4);
        assert_eq!(queue.stats().pushed, 5);
        assert_eq!(queue.stats().popped, 3);
    }

    #[test]
    fn push_reports_eviction() {
        let queue = EventQueue::new(2);
        assert!(queue.push(touch(0)));
        assert!(queue.push(touch(1)));
        assert!(!queue.push(touch(2)), "the third push evicted the first event");
    }

    #[test]
    fn drain_moves_only_up_to_the_limit_and_keeps_the_rest() {
        let queue = EventQueue::new(8);
        for index in 0..6 {
            queue.push(touch(index));
        }
        let mut buffer = Vec::new();
        assert_eq!(queue.drain_into(&mut buffer, 4), 4);
        assert_eq!(buffer.len(), 4);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.drain_into(&mut buffer, 100), 2);
        assert_eq!(buffer.len(), 6);
        assert_eq!(queue.drain_into(&mut buffer, 100), 0);
    }

    #[test]
    fn clear_drops_everything_but_keeps_counters() {
        let queue = EventQueue::new(4);
        queue.push(touch(0));
        queue.push(touch(1));
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.stats().pushed, 2);
    }

    #[test]
    fn lifecycle_notices_can_be_superseded() {
        let queue = EventQueue::new(8);
        queue.push(touch(0));
        queue.push(InputEvent::Lifecycle(crate::input::event::LifecycleNotice::SurfaceLost {
            timestamp_ms: 1,
        }));
        queue.push(InputEvent::Lifecycle(
            crate::input::event::LifecycleNotice::SurfaceAvailable {
                width: 100,
                height: 200,
                timestamp_ms: 2,
            },
        ));
        queue.remove_lifecycle_notices();
        assert_eq!(queue.len(), 1);
        let remaining = queue.pop().unwrap();
        assert_eq!(remaining.kind_name(), "touch");
    }

    #[test]
    fn capacity_is_at_least_one() {
        let queue = EventQueue::new(0);
        assert_eq!(queue.capacity(), 1);
        queue.push(touch(0));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn a_panicking_consumer_does_not_poison_the_queue() {
        let queue = EventQueue::new(2);
        queue.push(touch(0));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = queue.lock();
            panic!("simulated producer panic");
        }));
        assert!(result.is_err());
        // The queue is still usable: input must never be a fatal path.
        queue.push(touch(1));
        assert_eq!(queue.len(), 2);
        let stats = queue.stats();
        assert_eq!(stats.capacity, 2);
        assert_eq!(stats.pushed, 2);
    }

    #[test]
    fn key_events_round_trip_with_sdl_fields() {
        let queue = EventQueue::new(2);
        queue.push(InputEvent::Key {
            pressed: true,
            repeat: false,
            scancode: 4,
            keycode: 0x61,
            modifiers: 0,
            android_keycode: 29,
            device: DeviceKind::Keyboard,
            timestamp_ms: 5,
        });
        match queue.pop().unwrap() {
            InputEvent::Key { scancode, keycode, .. } => {
                assert_eq!(scancode, 4);
                assert_eq!(keycode, 0x61);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
}
