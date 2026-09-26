// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.
//
//! Logging for the native core.
//!
//! On Android everything goes to logcat under the `BoardBridge` tag through
//! `__android_log_write` (a plain, non-variadic liblog entry point, so no
//! format string ever crosses the language boundary). On every other target
//! the same messages go to stderr, which is what makes the pure-logic modules
//! testable on a normal machine.
//!
//! The level can be raised at runtime from Kotlin
//! ([`crate::jni::native_bridge`] exposes `setLogLevel`), which matters on
//! device: the per-frame diagnostics are `debug`-level precisely so that they
//! cost a single relaxed atomic load per frame when disabled.

use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};

/// logcat tag. Kept in sync with the Kotlin side and with the CI log greps.
pub const TAG: &str = "BoardBridge";

/// Android log priorities (see `<android/log.h>`); also used as the ordering
/// for filtering, since a lower priority number is a *more* verbose message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Verbose = 2,
    Debug = 3,
    Info = 4,
    Warn = 5,
    Error = 6,
}

impl Level {
    /// Short label used by the non-Android fallback writer.
    pub fn label(self) -> &'static str {
        match self {
            Level::Verbose => "V",
            Level::Debug => "D",
            Level::Info => "I",
            Level::Warn => "W",
            Level::Error => "E",
        }
    }

    fn from_u8(value: u8) -> Level {
        match value {
            2 => Level::Verbose,
            3 => Level::Debug,
            5 => Level::Warn,
            6 => Level::Error,
            _ => Level::Info,
        }
    }
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Sets the minimum level that is actually written out.
pub fn set_level(level: Level) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Current minimum level.
pub fn level() -> Level {
    Level::from_u8(LEVEL.load(Ordering::Relaxed))
}

/// Cheap check so callers can skip building expensive messages.
pub fn enabled(level: Level) -> bool {
    (level as u8) <= LEVEL.load(Ordering::Relaxed)
}

/// Writes one already-formatted message. Prefer the [`bb_info!`] family.
pub fn emit(level: Level, args: fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    // `Arguments` implements `Display`, so this is the allocation the message
    // needs and nothing more (there is no `fmt::format` outside `alloc`).
    let message = args.to_string();
    write_line(level, &message);
}

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: i32, tag: *const i8, text: *const i8) -> i32;
}

#[cfg(target_os = "android")]
fn write_line(level: Level, message: &str) {
    use std::ffi::CString;

    // The C API is NUL-terminated; a NUL inside a message (never expected, but
    // possible when a Java string is logged) must not truncate it silently.
    let sanitized = message.replace('\0', "\\0");
    let text = match CString::new(sanitized) {
        Ok(value) => value,
        Err(_) => return,
    };
    // The tag is a compile-time constant without interior NULs.
    static TAG_BYTES: &[u8] = b"BoardBridge\0";
    unsafe {
        __android_log_write(level as i32, TAG_BYTES.as_ptr() as *const i8, text.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn write_line(level: Level, message: &str) {
    eprintln!("{} {}: {}", level.label(), TAG, message);
}

/// `error`-level log line.
#[macro_export]
macro_rules! bb_error {
    ($($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Error, format_args!($($arg)*))
    };
}

/// `warn`-level log line.
#[macro_export]
macro_rules! bb_warn {
    ($($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Warn, format_args!($($arg)*))
    };
}

/// `info`-level log line.
#[macro_export]
macro_rules! bb_info {
    ($($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Info, format_args!($($arg)*))
    };
}

/// `debug`-level log line (per-frame diagnostics live here).
#[macro_export]
macro_rules! bb_debug {
    ($($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Debug, format_args!($($arg)*))
    };
}

/// `verbose`-level log line.
#[macro_export]
macro_rules! bb_verbose {
    ($($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Verbose, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_filtering_follows_android_priority_order() {
        set_level(Level::Info);
        assert!(enabled(Level::Error));
        assert!(enabled(Level::Info));
        assert!(!enabled(Level::Debug));

        set_level(Level::Verbose);
        assert!(enabled(Level::Debug));

        set_level(Level::Warn);
        assert!(!enabled(Level::Info));
        assert!(enabled(Level::Error));
    }

    #[test]
    fn round_trips_through_u8() {
        for level in [Level::Verbose, Level::Debug, Level::Info, Level::Warn, Level::Error] {
            assert_eq!(Level::from_u8(level as u8), level);
        }
    }
}
