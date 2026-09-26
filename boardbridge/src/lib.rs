// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! # BoardBridge native core
//!
//! The Android platform/runtime bridge that a Minecraft Java Edition launcher is
//! built on. Kotlin owns the Android shell; this crate owns everything native:
//! `ANativeWindow` ownership, EGL, the render/loop model, input translation, and
//! the interfaces for SDL3 and GLFW compatibility.
//!
//! ```text
//! Kotlin (app/)                        this crate (libboardbridge.so)
//! ─────────────                        ─────────────────────────────
//! SurfaceHolder callbacks  ── JNI ──▶  lifecycle machine + command queue
//! MotionEvent/KeyEvent     ── JNI ──▶  Android → SDL translation → EventQueue
//! attachGameThread/swap    ── JNI ──▶  EGL context ownership / present
//!                                       │
//!                                       ▼
//!                                    EGL (libEGL) ─▶ ANativeWindow
//! ```
//!
//! ## Module map
//!
//! | Module | Responsibility | Portable to a host build? |
//! |---|---|---|
//! | [`error`] | error type + stable codes for Kotlin | yes |
//! | [`log`] | logcat (or stderr) with level filtering | yes |
//! | [`lifecycle`] | surface/pause state machine, pure logic | yes (unit-tested) |
//! | [`input`] | SDL-shaped events, bounded queue, Android→SDL keymap | yes (unit-tested) |
//! | [`runtime`] | configuration (portable) + bridge thread and registry (Android) | partly |
//! | `android` | NDK window ownership, JNI→event adapters | no (Android) |
//! | `egl` | `EGLDisplay`/`EGLContext`/`EGLSurface` in Rust | partly (EGL config/types on host) |
//! | `graphics` | backend trait, GLES implementation, Vulkan interface | partly (enums/structs on host) |
//! | `render` | the bridge's own diagnostic renderer | partly (mode enum on host) |
//! | `platform` | who supplies windows/input: native, SDL3, GLFW | no (Android) |
//! | `jni` | the exported entry points Kotlin calls | no (Android) |
//!
//! ## Two loops, one surface
//!
//! * **Internal loop** — the bridge thread draws (diagnostics: a solid clear with
//!   a centre-pixel readback, or a shaded triangle). Used by the demo app, the
//!   self-test and CI.
//! * **Inverted loop** — Minecraft owns the loop: it calls `attachGameThread`,
//!   renders, and calls `swapBuffers` from its own thread, while the bridge
//!   thread only services surface lifecycle changes. Surface loss is delivered as
//!   a queued [`input::LifecycleNotice`] instead of an interruption.
//!
//! Both run on the same surface/EGL ownership rules, so a launcher can start with
//! the internal loop (to prove the device works) and switch to the game loop
//! without changing any native code.
//!
//! ## Threading in one table
//!
//! | Thread | Owns | Must never |
//! |---|---|---|
//! | Android UI | `Surface` callbacks, input callbacks | block beyond the surface fence (≤ 250 ms) |
//! | bridge (this crate) | EGL lifecycle, surface binding, diagnostic loop | touch a window after releasing it, spin |
//! | game (Minecraft) | its own GL work, `attachGameThread`/`swapBuffers` | present after a revoked surface (it is refused, not crashed) |
//! | any | `getStatus`, `getRendererInfo`, input push | call `createRuntime` twice |
//!
//! ## Honesty about what is implemented
//!
//! | Area | Status |
//! |---|---|
//! | Surface lifecycle, `ANativeWindow` ownership, fences | **implemented** |
//! | EGL (display/config/context/surfaces, ES 3.2→3.0 fallback) | **implemented** |
//! | Input translation (SDL scancodes/keycodes/mods/gamepad/mouse) | **implemented**, generated from SDL3's own tables |
//! | Frame-loop inversion (`attach`/`swap`) | **implemented** |
//! | Diagnostic renderer + self-test | **implemented** |
//! | Vulkan backend | interface only (`docs/GRAPHICS.md`) |
//! | SDL3 platform backend | interface only (`docs/SDL3.md`) |
//! | GLFW compatibility backend | interface only (`docs/GLFW_COMPAT.md`) |
//! | LWJGL native glue | not started (`docs/LWJGL.md`) |
//! | Launcher features (downloads, JVM, auth, instances) | out of scope here; separate `launcher-core` |

pub mod error;
pub mod input;
pub mod lifecycle;
pub mod log;
pub mod runtime;

// `egl`, `graphics` and `render` are compiled on every target: their *data*
// types are pure (`ConfigRequest`, `RendererKind`, `GraphicsConfig`,
// `DiagnosticMode`) and the runtime configuration carries them, so decoding
// them from JNI is unit-tested on the CI runner. Each module gates only the
// parts that talk to Android/EGL (`ffi`, `display`, `context`, `surface`,
// `gles`, `vulkan`, `diagnostics`) behind `cfg(target_os = "android")`.
pub mod egl;
pub mod graphics;
pub mod render;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub mod jni;
#[cfg(target_os = "android")]
pub mod platform;

pub use error::{Error, Result};

/// Version of the crate, reported by diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
