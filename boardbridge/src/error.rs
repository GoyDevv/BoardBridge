// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Error type shared by every BoardBridge subsystem.
//!
//! Every entry point that crosses the JNI boundary returns [`Result`]; the JNI
//! layer turns an [`Error`] into a stable negative integer code
//! ([`Error::code`]) so Kotlin can report *why* a lifecycle call was refused
//! without a second JNI round trip.

use core::fmt;

/// Result alias used throughout the crate.
pub type Result<T> = core::result::Result<T, Error>;

/// What went wrong.
///
/// The variants are deliberately coarse: callers on the Kotlin side can only
/// act on a handful of distinctions (not initialized, wrong state, no surface,
/// unsupported), while [`Error::Message`] carries the detail for logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No runtime exists (or it has already been destroyed).
    NotInitialized,
    /// `createRuntime` was called while a runtime was already alive.
    AlreadyInitialized,
    /// A lifecycle call arrived in a state where it cannot be honored.
    InvalidState {
        /// The state the call was refused in (as reported by diagnostics).
        state: &'static str,
        /// Short reason, for logs.
        detail: &'static str,
    },
    /// No `ANativeWindow` is currently bound.
    NoSurface,
    /// A window bind/unbind is already in flight.
    SurfaceBusy,
    /// The surface was revoked (destroyed/rotated) while an in-flight present
    /// referenced it. The game thread must stop presenting and detach.
    SurfaceRevoked,
    /// EGL or GL reported a failure.
    Graphics {
        /// Operation that failed, e.g. `"eglCreateContext"`.
        op: &'static str,
        /// `EGL_SUCCESS`-style code, `0` when the failure is not EGL.
        code: i32,
    },
    /// The requested renderer/backend exists as an interface but is not
    /// implemented yet; the string names the remaining work.
    BackendUnavailable(&'static str),
    /// A JNI/argument error.
    InvalidArgument(&'static str),
    /// Anything else, with a human-readable message.
    Message(String),
}

impl Error {
    /// Convenience for EGL/GL failures.
    pub fn graphics(op: &'static str, code: i32) -> Error {
        Error::Graphics { op, code }
    }

    /// Stable negative integer code handed to Kotlin. `0` means success.
    ///
    /// Keep these values in sync with `NativeBridge.kt`, which mirrors them as
    /// constants for the launcher code.
    pub fn code(&self) -> i32 {
        match self {
            Error::NotInitialized => -1,
            Error::AlreadyInitialized => -2,
            Error::InvalidState { .. } => -3,
            Error::NoSurface => -4,
            Error::SurfaceBusy => -5,
            Error::SurfaceRevoked => -6,
            Error::Graphics { .. } => -7,
            Error::BackendUnavailable(_) => -8,
            Error::InvalidArgument(_) => -9,
            Error::Message(_) => -10,
        }
    }

    /// Short, log-friendly description.
    pub fn detail(&self) -> String {
        match self {
            Error::NotInitialized => "runtime is not initialized".to_string(),
            Error::AlreadyInitialized => "runtime is already initialized".to_string(),
            Error::InvalidState { state, detail } => format!("invalid state {state}: {detail}"),
            Error::NoSurface => "no surface is bound".to_string(),
            Error::SurfaceBusy => "a surface transition is already in progress".to_string(),
            Error::SurfaceRevoked => "surface was revoked; detach the game thread".to_string(),
            Error::Graphics { op, code } => format!("{op} failed (code 0x{code:04x})"),
            Error::BackendUnavailable(what) => format!("backend not implemented: {what}"),
            Error::InvalidArgument(what) => format!("invalid argument: {what}"),
            Error::Message(message) => message.clone(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Message(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let all = [
            Error::NotInitialized,
            Error::AlreadyInitialized,
            Error::InvalidState {
                state: "NO_SURFACE",
                detail: "test",
            },
            Error::NoSurface,
            Error::SurfaceBusy,
            Error::SurfaceRevoked,
            Error::Graphics {
                op: "eglCreateContext",
                code: 0x3003,
            },
            Error::BackendUnavailable("SDL3"),
            Error::InvalidArgument("renderer"),
            Error::Message("boom".to_string()),
        ];
        for (index, error) in all.iter().enumerate() {
            assert_eq!(error.code(), -((index as i32) + 1));
            assert!(!error.detail().is_empty());
        }
    }
}
