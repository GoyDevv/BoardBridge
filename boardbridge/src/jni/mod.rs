// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! The JNI boundary.
//!
//! [`native_bridge`] exports exactly the entry points declared by
//! `app/src/main/java/com/boardbridge/bridge/NativeBridge.kt`, and nothing else.
//! The contract, in one place so the two sides cannot drift:
//!
//! * every `Int`-returning function returns `0` on success or a negative
//!   [`crate::error::Error::code`];
//! * `getRendererInfo`/`getStatus`/`runSelfTest` return strings and never fail
//!   (an empty renderer string means "context not ready yet", which the Kotlin
//!   side already retries);
//! * `surfaceCreated` takes ownership of the `ANativeWindow` reference it
//!   acquires, whether or not the lifecycle accepts it;
//! * no entry point blocks for longer than the configured fences (≤ 250 ms by
//!   default), and none of them ever waits on the game thread without a deadline;
//! * every entry point is panic-guarded ([`helpers::guarded`]) so a bug surfaces
//!   as a logged error code instead of a native crash.
//!
//! Two things this layer deliberately does *not* do: call back into Java (no
//! cached `jclass`/`jobject`, no `AttachCurrentThread`), and interpret Android
//! input semantics (that lives in [`crate::android::input`] and
//! [`crate::input::keymap`]).

pub mod helpers;
pub mod native_bridge;

pub use helpers::{guarded, new_string, report, string_arg, OK};
