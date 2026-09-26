// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! The process-wide runtime registry.
//!
//! One `Runtime` per process, created by `NativeBridge.createRuntime` and
//! destroyed by `destroyRuntime`. Kotlin owns the lifetime; this module only
//! makes it explicit and hard to get wrong:
//!
//! * `create` refuses a second runtime instead of silently replacing one (which
//!   would leak a bridge thread and an EGL context);
//! * `destroy` removes the runtime from the registry *before* stopping the
//!   thread, so a JNI call racing the destruction sees "not initialized" rather
//!   than a half-stopped runtime;
//! * `with_runtime` serializes the JNI entry points (documented below).
//!
//! # Why a mutex, and why that is not a "global unsafe mutable state"
//!
//! The alternative — a raw `static mut` pointer or an unchecked pointer cast
//! from Java — is what the design notes call out for removal. Here the state is
//! a single `Mutex<Option<Runtime>>`, which is:
//!
//! * initialized before any JNI call can reach it (`Mutex::new` is `const`);
//! * impossible to observe half-initialized (creation happens while the lock is
//!   held, and a failure leaves `None` behind);
//! * poison-tolerant (a panic inside one JNI call cannot wedge the bridge).
//!
//! Holding the registry lock for the duration of a call does serialize the JNI
//! entry points. That is deliberate and cheap: they are all lifecycle-shaped
//! (a handful per rotation), the bridge thread never touches this lock, and the
//! one call that can block (`surfaceDestroyed`) is bounded by
//! `detach_timeout_ms`.

use std::sync::{Mutex, MutexGuard};

use crate::error::{Error, Result};
use crate::runtime::config::RuntimeConfig;
use crate::runtime::control::Runtime;

static REGISTRY: Mutex<Option<Runtime>> = Mutex::new(None);

fn slot() -> MutexGuard<'static, Option<Runtime>> {
    REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Creates the runtime. Fails when one already exists.
pub fn create(config: RuntimeConfig) -> Result<()> {
    let mut guard = slot();
    if guard.is_some() {
        return Err(Error::AlreadyInitialized);
    }
    let runtime = Runtime::start(config)?;
    *guard = Some(runtime);
    Ok(())
}

/// Stops and destroys the runtime.
///
/// The lock is released before the join, so the bridge thread's shutdown (which
/// may wait for another thread to release the surface) cannot deadlock against a
/// concurrent JNI call.
pub fn destroy() -> Result<()> {
    let runtime = {
        let mut guard = slot();
        guard.take()
    };
    match runtime {
        Some(mut runtime) => {
            runtime.stop();
            Ok(())
        }
        None => Err(Error::NotInitialized),
    }
}

/// Whether a runtime exists.
pub fn exists() -> bool {
    slot().is_some()
}

/// Runs `f` with the live runtime, or returns `None` when there is none.
pub fn with_runtime<R>(f: impl FnOnce(&Runtime) -> R) -> Option<R> {
    let guard = slot();
    guard.as_ref().map(f)
}

/// Like [`with_runtime`], but `NotInitialized` when there is no runtime.
pub fn try_with<R>(f: impl FnOnce(&Runtime) -> Result<R>) -> Result<R> {
    match with_runtime(f) {
        Some(result) => result,
        None => Err(Error::NotInitialized),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_destroy_are_explicit_about_the_registry_state() {
        // The registry is process-wide, so this test owns it for its duration.
        assert!(!exists());
        create(RuntimeConfig::default()).expect("first create succeeds");
        assert!(exists());
        let second = create(RuntimeConfig::default()).expect_err("a second runtime is refused");
        assert_eq!(second, Error::AlreadyInitialized);
        assert!(with_runtime(|_| ()).is_some());
        destroy().expect("destroy succeeds");
        assert!(!exists());
        assert_eq!(destroy().expect_err("double destroy is an error"), Error::NotInitialized);
        assert!(with_runtime(|_| ()).is_none());
        assert_eq!(
            try_with(|_| Ok(())).expect_err("no runtime"),
            Error::NotInitialized
        );
    }
}
