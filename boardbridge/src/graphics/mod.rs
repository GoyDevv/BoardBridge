// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Graphics backends.
//!
//! The bridge does not implement a renderer for the game — Minecraft brings its
//! own. What it owns is the *plumbing*: an `EGLSurface` on the Android window,
//! a context that can be made current on whichever thread the game renders from,
//! and the present operation. That is what [`GraphicsBackend`] describes.
//!
//! * [`gles`] — OpenGL ES 3.x via EGL. **Implemented.** *(Android only.)*
//! * [`vulkan`] — Vulkan. **Interface only**; see its documentation for exactly
//!   what remains to be done. Minecraft's own Vulkan migration (announced for
//!   Java Edition) is the reason the seam exists now rather than later.
//!   *(Android only.)*
//!
//! Adding a backend means implementing this trait; nothing else in the bridge
//! needs to change. The runtime only ever sees `Arc<dyn GraphicsBackend>`.
//!
//! The *data* half of this module ([`RendererKind`], [`GraphicsConfig`],
//! [`BackendStatus`], [`RendererInfo`], [`GraphicsStats`], [`ThreadRole`]) is
//! pure and compiled on every target, because `RuntimeConfig` carries it and its
//! decoding is unit-tested on the CI runner. Only the [`GraphicsBackend`] trait
//! itself — which takes ownership of an `ANativeWindow` — and the concrete
//! backends are Android-only.

#[cfg(target_os = "android")]
pub mod ffi;
#[cfg(target_os = "android")]
pub mod gles;
#[cfg(target_os = "android")]
pub mod vulkan;

#[cfg(target_os = "android")]
use std::sync::Arc;

#[cfg(target_os = "android")]
use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::egl::ConfigRequest;
#[cfg(target_os = "android")]
use crate::error::Result;

/// Which graphics API to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RendererKind {
    /// Pick the best available backend (currently OpenGL ES).
    Auto = 0,
    /// OpenGL ES 3.x via EGL.
    Gles = 1,
    /// Vulkan (interface only).
    Vulkan = 2,
}

impl RendererKind {
    /// Decodes the value sent from Kotlin.
    pub fn from_jni(value: i32) -> RendererKind {
        match value {
            1 => RendererKind::Gles,
            2 => RendererKind::Vulkan,
            _ => RendererKind::Auto,
        }
    }

    /// Name for logs and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            RendererKind::Auto => "auto",
            RendererKind::Gles => "OpenGL ES",
            RendererKind::Vulkan => "Vulkan",
        }
    }
}

/// Which thread is doing the GL work.
///
/// The bridge has at most two threads inside EGL: its own bridge thread (the
/// internal diagnostic loop, lifecycle teardown) and the game thread (Minecraft,
/// in frame-loop-inversion mode). Recording the role makes the ownership visible
/// in diagnostics — `owner=game` in logcat immediately explains why the bridge
/// thread is waiting for a surface to be released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    /// The bridge's own thread.
    Bridge,
    /// The game/Minecraft thread (attached through `attachGameThread`).
    Game,
}

impl ThreadRole {
    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            ThreadRole::Bridge => "bridge",
            ThreadRole::Game => "game",
        }
    }
}

/// GL strings and context facts, as reported by the driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererInfo {
    /// `GL_VENDOR`.
    pub vendor: String,
    /// `GL_RENDERER`.
    pub renderer: String,
    /// `GL_VERSION`.
    pub gl_version: String,
    /// `GL_SHADING_LANGUAGE_VERSION`, when reported.
    pub glsl_version: Option<String>,
    /// ES major version read back with `glGetIntegerv(GL_MAJOR_VERSION)`.
    pub es_major: i32,
    /// ES minor version.
    pub es_minor: i32,
    /// The context request that succeeded (for example `"ES 3.2"`).
    pub context_request: String,
    /// `true` when the context was created at the device's best supported level.
    pub best_available: bool,
}

impl RendererInfo {
    /// `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`.
    ///
    /// The exact shape of the old C++ `rendererInfo()` string is kept: it is
    /// what `NativeBridge.getRendererInfo()` returns, what the launcher UI will
    /// show, and what the CI render test greps for.
    pub fn summary(&self) -> String {
        format!(
            "GL_VENDOR={} | GL_RENDERER={} | GL_VERSION={}",
            self.vendor, self.renderer, self.gl_version
        )
    }

    /// `ES 3.2 (Mali-G52, OpenGL ES 3.2 v1.r38p1)`, for logs.
    pub fn describe(&self) -> String {
        format!(
            "ES {}.{} via {} ({})",
            self.es_major, self.es_minor, self.context_request, self.renderer
        )
    }
}

/// Whether a backend is actually usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendStatus {
    /// Ready to use.
    Implemented,
    /// The interface exists but the implementation does not; the string names
    /// the remaining work (surfaced through `getStatus`/`getRendererInfo`).
    InterfaceOnly {
        /// What is still missing.
        remaining: &'static str,
    },
}

impl BackendStatus {
    /// `true` for [`BackendStatus::Implemented`].
    pub fn is_implemented(self) -> bool {
        matches!(self, BackendStatus::Implemented)
    }

    /// Short label for diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            BackendStatus::Implemented => "implemented",
            BackendStatus::InterfaceOnly { .. } => "interface-only",
        }
    }

    /// The remaining-work description, when interface-only.
    pub fn remaining(self) -> Option<&'static str> {
        match self {
            BackendStatus::Implemented => None,
            BackendStatus::InterfaceOnly { remaining } => Some(remaining),
        }
    }
}

/// Backend counters, for `getStatus` and logcat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsStats {
    /// Successful presents.
    pub presents: u64,
    /// Failed presents (driver errors).
    pub present_failures: u64,
    /// Presents refused because the surface had been revoked.
    pub rejected_presents: u64,
    /// Window bindings created.
    pub generations: u64,
    /// Bindings whose teardown had to be deferred (a thread still held them).
    pub deferred_releases: u64,
    /// Presents currently in flight (outside the backend lock, waiting on vsync).
    pub in_flight: u32,
    /// Whether a window surface is currently bound.
    pub has_window: bool,
    /// Whether the bound surface has been revoked and may not be presented to.
    pub revoked: bool,
    /// Which thread currently has the context current, if any.
    pub owner: Option<ThreadRole>,
}

impl GraphicsStats {
    /// Compact `k=v` summary.
    pub fn summary(&self) -> String {
        format!(
            "presents={} failures={} rejected={} generations={} deferred={} in_flight={} bound={} revoked={} owner={}",
            self.presents,
            self.present_failures,
            self.rejected_presents,
            self.generations,
            self.deferred_releases,
            self.in_flight,
            self.has_window,
            self.revoked,
            match self.owner {
                Some(role) => role.name(),
                None => "none",
            }
        )
    }
}

/// Configuration handed to a backend at construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicsConfig {
    /// EGL config request (colour/depth/stencil/surface bits).
    pub config_request: ConfigRequest,
    /// Swap interval for threads that attach (`1` = vsync, `0` = immediate).
    pub swap_interval: i32,
    /// Keep the EGL context (and its GL objects) alive across surface loss.
    ///
    /// This is what makes rotation and backgrounding cheap for Minecraft: its
    /// textures, shaders and VAOs survive, and only the window surface is
    /// recreated. When disabled, the context is destroyed with the surface.
    pub preserve_context: bool,
    /// How long teardown waits for in-flight presents / another thread to
    /// release the context before deferring the release.
    pub drain_timeout_ms: u64,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        GraphicsConfig {
            config_request: ConfigRequest::launcher(),
            swap_interval: 1,
            preserve_context: true,
            drain_timeout_ms: 250,
        }
    }
}

/// The seam every graphics API plugs into.
///
/// Contract, in one paragraph: `initialize` may be called once; `bind_window`
/// and `unbind_window` are called *only* from the bridge thread; `make_current`,
/// `release_current` and `present` may be called from the bridge thread and from
/// the game thread, but never concurrently for the same binding; `shutdown`
/// releases everything and is idempotent. Implementations must never release an
/// `ANativeWindow` while an `EGLSurface` created from it exists, and must never
/// let work touch a surface after `unbind_window` returned.
///
/// Android-only: `bind_window` takes ownership of an `ANativeWindow`, which has
/// no counterpart on a host build.
#[cfg(target_os = "android")]
pub trait GraphicsBackend: Send + Sync {
    /// Human-readable backend name (`"OpenGL ES"`).
    fn name(&self) -> &'static str;

    /// Which API this backend speaks.
    fn kind(&self) -> RendererKind;

    /// Whether the backend is implemented or just an interface.
    fn status(&self) -> BackendStatus;

    /// Backend-specific description of its current state, for diagnostics
    /// (`getStatus`/`runSelfTest`): EGL version/vendor/config for the GLES
    /// backend, the remaining-work note for an interface-only one.
    fn describe(&self) -> String;

    /// One-time initialization (display, config, context).
    fn initialize(&self) -> Result<()>;

    /// Binds a window, returning the surface size EGL reports.
    ///
    /// Takes ownership of `window`; the backend releases it in
    /// `unbind_window`/`shutdown` or, on failure, immediately.
    fn bind_window(&self, window: OwnedNativeWindow, requested: SurfaceSize) -> Result<SurfaceSize>;

    /// Retires the current binding (destroy `EGLSurface`, release the window).
    ///
    /// Must be safe to call with no binding. Called by the bridge thread after
    /// `surfaceDestroyed`, and before a new bind.
    fn unbind_window(&self) -> Result<()>;

    /// Whether a binding exists.
    fn has_window(&self) -> bool;

    /// Monotonic id of the current binding (0 before the first bind).
    fn binding_generation(&self) -> u64;

    /// Makes the context current on the calling thread, against the window if
    /// one is bound and against the offscreen surface otherwise.
    fn make_current(&self, role: ThreadRole) -> Result<()>;

    /// Unbinds the context from the calling thread.
    fn release_current(&self) -> Result<()>;

    /// Presents the back buffer. Must be called with the context current on the
    /// calling thread.
    fn present(&self) -> Result<()>;

    /// Size of the current binding (zero when none).
    fn window_size(&self) -> SurfaceSize;

    /// Re-reads the surface size from EGL after a resize.
    fn refresh_window_size(&self) -> SurfaceSize;

    /// GL strings; requires an initialized context.
    fn renderer_info(&self) -> Result<RendererInfo>;

    /// Counters for diagnostics.
    fn stats(&self) -> GraphicsStats;

    /// Changes the swap interval for threads that attach later.
    fn set_swap_interval(&self, interval: i32) -> Result<()>;

    /// Releases everything. Idempotent; never panics on a partially initialized
    /// backend.
    fn shutdown(&self);
}

/// Creates a backend for the requested renderer.
///
/// `Auto` currently resolves to OpenGL ES, which is the only implemented
/// backend; `Vulkan` returns a backend object whose [`GraphicsBackend::status`]
/// is [`BackendStatus::InterfaceOnly`] so diagnostics can report precisely what
/// is missing instead of failing with a bare error.
#[cfg(target_os = "android")]
pub fn create(kind: RendererKind, config: GraphicsConfig) -> Arc<dyn GraphicsBackend> {
    match kind {
        RendererKind::Vulkan => Arc::new(vulkan::VulkanBackend::new(config)),
        RendererKind::Auto | RendererKind::Gles => Arc::new(gles::GlesBackend::new(config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_kind_decoding_is_tolerant() {
        assert_eq!(RendererKind::from_jni(1), RendererKind::Gles);
        assert_eq!(RendererKind::from_jni(2), RendererKind::Vulkan);
        assert_eq!(RendererKind::from_jni(0), RendererKind::Auto);
        assert_eq!(RendererKind::from_jni(99), RendererKind::Auto);
        assert_eq!(RendererKind::Gles.name(), "OpenGL ES");
    }

    #[test]
    fn renderer_info_keeps_the_legacy_string_shape() {
        let info = RendererInfo {
            vendor: "Mali".to_string(),
            renderer: "Mali-G52".to_string(),
            gl_version: "OpenGL ES 3.2".to_string(),
            glsl_version: None,
            es_major: 3,
            es_minor: 2,
            context_request: "ES 3.2".to_string(),
            best_available: true,
        };
        assert_eq!(
            info.summary(),
            "GL_VENDOR=Mali | GL_RENDERER=Mali-G52 | GL_VERSION=OpenGL ES 3.2"
        );
        assert!(info.describe().contains("Mali-G52"));
    }

    #[test]
    fn backend_status_reports_remaining_work() {
        let stub = BackendStatus::InterfaceOnly { remaining: "link SDL3" };
        assert!(!stub.is_implemented());
        assert_eq!(stub.remaining(), Some("link SDL3"));
        assert_eq!(stub.label(), "interface-only");
        assert!(BackendStatus::Implemented.is_implemented());
    }

    #[test]
    fn stats_summary_is_log_friendly() {
        let stats = GraphicsStats {
            presents: 3,
            owner: Some(ThreadRole::Game),
            has_window: true,
            ..GraphicsStats::default()
        };
        let summary = stats.summary();
        assert!(summary.contains("presents=3"));
        assert!(summary.contains("owner=game"));
        assert!(summary.contains("bound=true"));
    }
}
