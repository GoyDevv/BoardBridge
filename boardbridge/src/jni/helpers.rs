// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Helpers shared by the JNI entry points.

use jni::objects::JString;
use jni::JNIEnv;

use crate::bb_debug;
use crate::bb_error;
use crate::error::Result;

/// Success code returned to Kotlin.
pub const OK: i32 = 0;

/// Runs `body`, turning a panic into `T::default()`.
///
/// A panic must never unwind into the JVM: that is undefined behaviour. In
/// release builds the profile sets `panic = "abort"` so it cannot happen at all;
/// this guard covers debug builds, where the default is unwind, and keeps a
/// programming error in one entry point from taking the process down during
/// development.
pub fn guarded<T: Default>(what: &'static str, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => {
            bb_error!("panic contained in {what}; returning the default value");
            T::default()
        }
    }
}

/// Reads a Java `String` argument; empty when null or unreadable.
pub fn string_arg(env: &mut JNIEnv<'_>, value: &JString<'_>) -> String {
    match env.get_string(value) {
        Ok(text) => text.into(),
        Err(error) => {
            bb_debug!("could not read a Java string: {error}");
            String::new()
        }
    }
}

/// Converts a unit result into the JNI error-code convention, logging failures.
pub fn report(op: &'static str, result: Result<()>) -> i32 {
    match result {
        Ok(()) => OK,
        Err(error) => {
            bb_error!("{op} failed: {error}");
            error.code()
        }
    }
}

/// Creates a Java string, returning a null `jstring` if that fails.
pub fn new_string(env: &JNIEnv<'_>, text: String) -> jni::sys::jstring {
    match env.new_string(text) {
        Ok(value) => value.into_raw(),
        Err(error) => {
            bb_error!("new_string failed: {error}");
            std::ptr::null_mut()
        }
    }
}
