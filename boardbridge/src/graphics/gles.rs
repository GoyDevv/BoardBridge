// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! OpenGL ES backend (replaces the former `egl_core.cpp` + `render_thread.cpp`
//! GL state).
//!
//! # What this type guarantees
//!
//! 1. **One owner per binding.** [`GlesBackend::present`] refuses to present
//!    unless the calling thread made the context current and no other thread
//!    owns it.
//! 2. **No use-after-free.** A teardown marks the binding *revoked* before it
//!    touches anything, then waits — with a deadline, never forever — for
//!    in-flight presents to drain and for the owning thread to release the
//!    context. Only then is the `EGLSurface` destroyed and the `ANativeWindow`
//!    released ([`crate::egl::surface::WindowBinding`]'s field order).
//! 3. **No UI-thread stall.** If the deadline expires the teardown is *deferred*
//!    and retried when the owning thread detaches. The window reference stays
//!    with the backend, so nothing can dangle; the worst case is a delayed
//!    release, never a crash.
//! 4. **No lock held across the GPU.** `eglSwapBuffers` — the only call that can
//!    wait for vsync — runs with the backend lock released; an
//!    [`InFlight`] guard keeps the counters correct even if the call unwinds.
//! 5. **EGL state that can be invalidated is expected to be.** The default EGL
//!    display is *process-wide*: any other EGL user in the process calling
//!    `eglTerminate` on it empties libEGL's object table, after which every
//!    `EGLSurface`/`EGLContext` we already hold is non-null but unresolvable
//!    (every use fails with `EGL_BAD_SURFACE`). Two things follow, and both are
//!    deliberate: EGL is created *late* — on the first bind, never at
//!    `initialize` — so a display, its context and the window surface are only
//!    ever a few instructions apart; and a failed `eglMakeCurrent` is treated as
//!    "the display is gone", not as a per-frame error: the stale handles are
//!    forgotten (never handed back to EGL), EGL is rebuilt, and the surface is
//!    recreated from the `ANativeWindow` we still own.
//!
//! # Why late creation, concretely
//!
//! `createRuntime` runs in `Activity.onCreate`, seconds before the `SurfaceView`
//! has a surface. Creating the display there and binding a window later leaves a
//! window in which the activity's own HWUI render thread can tear its EGL state
//! down and take our display's objects with it. The former C++ core created EGL
//! inside the render thread that starts on `surfaceCreated`, which is why the
//! same sequence is restored here.
//!
//! To be exact about what fixed what: the black screen this crate shipped was
//! **not** an invalidated display — it was `WindowSurface::create` returning a
//! handle whose `Drop` had already run (see that function). Late creation and the
//! recovery path below are defences against an external `eglTerminate`, which no
//! log has shown yet; they narrow the window and make a failure recoverable, and
//! they are worth keeping for a game that brings its own GL setup.
//!
//! # Threading
//!
//! | Operation | Allowed caller |
//! |---|---|
//! | `initialize`, `shutdown`, `bind_window`, `unbind_window` | bridge thread only |
//! | `make_current`, `release_current`, `present` | bridge thread *or* game thread, one at a time |
//! | `stats`, `renderer_info`, `window_size` | any thread |
//!
//! `bind_window`/`unbind_window` come from a single producer (the runtime's
//! command queue), which is what makes the FIFO ordering of "retire the old
//! binding, then bind the new one" trustworthy during rotation.

use std::ffi::CStr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::bb_debug;
use crate::bb_error;
use crate::bb_info;
use crate::bb_warn;
use crate::egl::ffi as eglffi;
use crate::egl::surface::{PbufferSurface, WindowBinding};
use crate::egl::{last_error, Config, Context, CurrentTarget, Display, ES3_FALLBACK_CHAIN};
use crate::error::{Error, Result};
use crate::graphics::ffi as gl;
use crate::graphics::{
    BackendStatus, GraphicsBackend, GraphicsConfig, GraphicsStats, RendererInfo, RendererKind,
    ThreadRole,
};

/// How many times one binding generation may rebuild EGL after a failed
/// `eglMakeCurrent` before the surface is abandoned. Bounded on purpose: a
/// rebuild is cheap enough to try a few times, but a display that keeps being
/// terminated under us is not something to fight in a loop — the binding is
/// revoked instead, the render loop parks on its command queue, and the next
/// `surfaceCreated` (a rotation, a relaunch) starts from a clean generation.
const MAX_EGL_REBUILDS: u64 = 3;

/// OpenGL ES 3.x backend.
pub struct GlesBackend {
    inner: Mutex<GlesInner>,
    /// Signalled when `in_flight` or `owner` changes.
    pending: Condvar,
    /// Construction-time configuration.
    config: GraphicsConfig,
    /// Swap interval applied to threads when they attach.
    swap_interval: AtomicI32,
}

/// Mutable backend state.
///
/// **Field order is load-bearing** (Rust drops fields in declaration order):
/// the context, offscreen surface, binding and deferred bindings are declared
/// before the display and config, so they are destroyed *before*
/// `eglTerminate`. Reordering these fields would terminate a display that still
/// has live surfaces.
struct GlesInner {
    context: Option<Context>,
    pbuffer: Option<PbufferSurface>,
    binding: Option<WindowBinding>,
    deferred: Vec<WindowBinding>,
    display: Option<Display>,
    config: Option<Config>,
    info: Option<RendererInfo>,
    generation: u64,
    /// EGL rebuilds already attempted for the current generation.
    recoveries: u64,
    revoked: bool,
    owner: Option<ThreadId>,
    owner_role: Option<ThreadRole>,
    in_flight: u32,
    presents: u64,
    present_failures: u64,
    rejected_presents: u64,
    deferred_releases: u64,
    requested_size: SurfaceSize,
}

// SAFETY: `GlesInner` holds EGL handles and one `ANativeWindow`. EGL handles are
// documented as usable from any thread (a context may be current on only one
// thread at a time, which this type enforces itself through `owner`), and the
// window is a reference-counted NDK object moved between threads by design. The
// raw pointers inside are therefore not a thread-safety hazard; the mutex does
// the real work.
unsafe impl Send for GlesInner {}

/// RAII guard for an in-flight present.
struct InFlight<'a> {
    backend: &'a GlesBackend,
    active: bool,
}

impl<'a> InFlight<'a> {
    fn new(backend: &'a GlesBackend) -> InFlight<'a> {
        InFlight {
            backend,
            active: true,
        }
    }

    fn complete(&mut self, success: bool) {
        if !self.active {
            return;
        }
        self.active = false;
        let mut inner = self.backend.lock();
        inner.in_flight = inner.in_flight.saturating_sub(1);
        if success {
            inner.presents += 1;
        } else {
            inner.present_failures += 1;
        }
        self.backend.pending.notify_all();
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        if self.active {
            // Reached only if `eglSwapBuffers` unwound; keep the counters honest.
            self.complete(false);
        }
    }
}

impl GlesBackend {
    /// Creates the backend; no EGL work happens until [`GraphicsBackend::initialize`].
    pub fn new(config: GraphicsConfig) -> GlesBackend {
        GlesBackend {
            inner: Mutex::new(GlesInner {
                context: None,
                pbuffer: None,
                binding: None,
                deferred: Vec::new(),
                display: None,
                config: None,
                info: None,
                generation: 0,
                recoveries: 0,
                revoked: true,
                owner: None,
                owner_role: None,
                in_flight: 0,
                presents: 0,
                present_failures: 0,
                rejected_presents: 0,
                deferred_releases: 0,
                requested_size: SurfaceSize::default(),
            }),
            pending: Condvar::new(),
            swap_interval: AtomicI32::new(config.swap_interval),
            config,
        }
    }

    /// Lock that survives a poisoned mutex: a panic in one GL path must not make
    /// the whole bridge unusable.
    fn lock(&self) -> MutexGuard<'_, GlesInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Retires the current binding under the documented fence.
    ///
    /// Called by `unbind_window`, by `bind_window` before installing a new
    /// binding, and by `shutdown`.
    fn retire(&self, reason: &'static str) -> Result<()> {
        let mut inner = self.lock();

        let binding = match inner.binding.take() {
            Some(binding) => binding,
            None => {
                inner.revoked = true;
                self.finish_deferred(&mut inner);
                return Ok(());
            }
        };
        let generation = binding.generation();

        // 1. Revoked first: no new present can start for this binding.
        inner.revoked = true;

        // 2. Bounded fence: wait for presents and for the owning thread.
        let deadline = Instant::now() + Duration::from_millis(self.config.drain_timeout_ms.max(1));
        loop {
            if inner.in_flight == 0 && !self.owner_is_other(&inner) {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            let (guard, timeout) = self
                .pending
                .wait_timeout(inner, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inner = guard;
            if timeout.timed_out() {
                break;
            }
        }

        // 3a. Still in use by another thread: defer (safety over punctuality).
        if inner.in_flight > 0 || self.owner_is_other(&inner) {
            bb_warn!(
                "surface generation {generation} still in use (in_flight={}, owner={}); deferring release [{reason}]",
                inner.in_flight,
                inner
                    .owner_role
                    .map(ThreadRole::name)
                    .unwrap_or("none")
            );
            inner.deferred.push(binding);
            inner.deferred_releases += 1;
            self.finish_deferred(&mut inner);
            return Ok(());
        }

        // 3b. Nobody is using it: unbind this thread, then destroy in order.
        self.detach_owner(&mut inner);
        drop(binding);
        bb_info!("EGL surface destroyed (generation {generation}) [{reason}]");

        if !self.config.preserve_context {
            // Without context preservation the window teardown is a full
            // teardown, matching the old `EglCore::release` behavior.
            inner.pbuffer = None;
            inner.context = None;
            bb_debug!("EGL context released with the surface (preserve_context=false)");
        }
        Ok(())
    }

    /// Releases bindings that were deferred while another thread held them.
    fn finish_deferred(&self, inner: &mut GlesInner) {
        if inner.deferred.is_empty() {
            return;
        }
        if inner.in_flight > 0 || self.owner_is_other(inner) {
            return;
        }
        self.detach_owner(inner);
        while let Some(binding) = inner.deferred.pop() {
            let generation = binding.generation();
            drop(binding);
            bb_info!("deferred EGL surface release completed (generation {generation})");
        }
    }

    /// `true` when a thread other than the caller currently owns the context.
    fn owner_is_other(&self, inner: &GlesInner) -> bool {
        match inner.owner {
            Some(owner) => owner != thread::current().id(),
            None => false,
        }
    }

    /// Unbinds EGL from the caller and clears the owner bookkeeping.
    fn detach_owner(&self, inner: &mut GlesInner) {
        if let (Some(display), Some(context)) = (inner.display.as_ref(), inner.context.as_ref()) {
            // `CurrentTarget::None` only affects the calling thread; that is
            // exactly the thread this function is called from (bridge thread),
            // which is the one allowed to destroy the surface.
            let _ = context.make_current(display, CurrentTarget::None);
        }
        inner.owner = None;
        inner.owner_role = None;
    }

    /// Reads GL strings; requires a current context.
    fn query_renderer_info(context: &Context) -> Option<RendererInfo> {
        let vendor = gl_string(gl::GL_VENDOR)?;
        let renderer = gl_string(gl::GL_RENDERER)?;
        let gl_version = gl_string(gl::GL_VERSION)?;

        let mut major: gl::GLint = 0;
        let mut minor: gl::GLint = 0;
        unsafe {
            gl::glGetIntegerv(gl::GL_MAJOR_VERSION, &mut major);
            gl::glGetIntegerv(gl::GL_MINOR_VERSION, &mut minor);
        }
        let (es_major, es_minor) = if major > 0 {
            (major, minor)
        } else {
            parse_es_version(&gl_version).unwrap_or((3, 0))
        };

        let request = context.request();
        let best_available = request == ES3_FALLBACK_CHAIN[0];
        Some(RendererInfo {
            vendor,
            renderer,
            gl_version,
            glsl_version: gl_string(gl::GL_SHADING_LANGUAGE_VERSION),
            es_major,
            es_minor,
            context_request: request.describe(),
            best_available,
        })
    }

    /// Creates the EGL display, config, context and offscreen surface if they do
    /// not exist (or were forgotten after an invalidation).
    ///
    /// Cheap and idempotent after the first call, which is what lets
    /// `bind_window` and `make_current` call it unconditionally. Deliberately
    /// *not* called from [`GraphicsBackend::initialize`]: see the module docs.
    fn ensure_display(&self, inner: &mut GlesInner) -> Result<()> {
        if inner.display.is_some() && inner.config.is_some() && inner.context.is_some() {
            return Ok(());
        }

        let display = Display::initialize()?;
        bb_info!("{}", display.info().describe());

        let config = display.choose_config(&self.config.config_request)?;
        bb_info!("EGL config: {}", config.describe());

        let context = Context::create_best(&display, &config)?;

        // A 1x1 pbuffer costs nothing and buys the surface-loss path: the game's
        // context stays current (and its GL objects stay valid) while Android
        // has taken the window away. A driver that cannot back one is not fatal.
        let pbuffer = match PbufferSurface::create(&display, &config, SurfaceSize::new(1, 1)) {
            Ok(pbuffer) => Some(pbuffer),
            Err(error) => {
                bb_debug!("no pbuffer surface: {error}");
                None
            }
        };

        inner.display = Some(display);
        inner.config = Some(config);
        inner.context = Some(context);
        inner.pbuffer = pbuffer;
        // The GL strings belong to the context and are read on the first real
        // attach (`attach_current`), when a window surface is current.
        inner.info = None;
        Ok(())
    }

    /// Forgets every EGL handle without calling into EGL, in teardown order.
    ///
    /// This is the "the display is gone" path: the handles are non-null but no
    /// longer resolvable, so `eglDestroySurface`/`eglDestroyContext`/
    /// `eglTerminate` must not be called on them. Disarming drops each value as
    /// a no-op, and the binding keeps its `ANativeWindow`, which is ours and
    /// still valid.
    fn forget_display(&self, inner: &mut GlesInner) {
        if let Some(binding) = inner.binding.as_mut() {
            binding.surface_mut().disarm();
        }
        if let Some(mut pbuffer) = inner.pbuffer.take() {
            pbuffer.disarm();
        }
        if let Some(mut context) = inner.context.take() {
            context.disarm();
        }
        inner.config = None;
        if let Some(mut display) = inner.display.take() {
            display.disarm();
        }
        inner.owner = None;
        inner.owner_role = None;
        inner.info = None;
    }

    /// Rebuilds EGL after a failed `eglMakeCurrent`, recreating the window
    /// surface from the `ANativeWindow` the binding still owns.
    fn rebuild_display(&self, inner: &mut GlesInner) -> Result<()> {
        self.forget_display(inner);
        self.ensure_display(inner)?;
        if let (Some(binding), Some(display), Some(config)) = (
            inner.binding.as_mut(),
            inner.display.as_ref(),
            inner.config.as_ref(),
        ) {
            binding.recreate_surface(display, config)?;
        }
        inner.revoked = false;
        Ok(())
    }

    /// Makes the context current on `thread` against the bound window (or, with
    /// no window, against the offscreen surface).
    ///
    /// Assumes `inner` is locked and [`GlesBackend::ensure_display`] has run.
    fn attach_current(
        &self,
        inner: &mut GlesInner,
        thread: ThreadId,
        role: ThreadRole,
    ) -> Result<()> {
        // Extract everything the EGL call needs as plain values, so no borrow of
        // `inner` is alive across the EGL call.
        let (display_raw, context_raw) = match (inner.display.as_ref(), inner.context.as_ref()) {
            (Some(display), Some(context)) => (display.raw(), context.raw()),
            _ => return Err(Error::NotInitialized),
        };
        let target_surface = match inner.binding.as_ref() {
            Some(binding) if !inner.revoked => Some((binding.surface().raw(), false)),
            _ => inner.pbuffer.as_ref().map(|pbuffer| (pbuffer.raw(), true)),
        };
        let (surface_raw, is_pbuffer) = match target_surface {
            Some(target) => target,
            None => return Err(Error::NoSurface),
        };

        let ok =
            unsafe { eglffi::eglMakeCurrent(display_raw, surface_raw, surface_raw, context_raw) };
        if ok != eglffi::EGL_TRUE {
            return Err(Error::graphics("eglMakeCurrent", last_error()));
        }

        inner.owner = Some(thread);
        inner.owner_role = Some(role);

        // vsync applies per thread in EGL, so set it on every attach.
        let interval = self.swap_interval.load(Ordering::Relaxed);
        if interval >= 0 {
            let ok = unsafe { eglffi::eglSwapInterval(display_raw, interval) };
            if ok != eglffi::EGL_TRUE {
                bb_debug!(
                    "eglSwapInterval({interval}) refused on attach: 0x{:04x}",
                    last_error()
                );
            }
        }

        // GL strings need a current context; the first successful attach is also
        // where the C++ core logged `Surface bound. …`.
        if inner.info.is_none() {
            let info = inner
                .context
                .as_ref()
                .and_then(GlesBackend::query_renderer_info);
            if let Some(info) = info.as_ref() {
                // `summary()` carries `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`,
                // the shape the C++ core logged with `Surface bound. …` and the
                // shape `NativeBridge.getRendererInfo()` still returns.
                bb_info!(
                    "Surface bound: {} [{}] (best available: {})",
                    info.summary(),
                    info.describe(),
                    info.best_available
                );
            }
            inner.info = info;
        }

        if role == ThreadRole::Game {
            bb_info!(
                "game thread attached ({} context, generation {})",
                if is_pbuffer { "offscreen" } else { "window" },
                inner.generation
            );
        }
        Ok(())
    }

    /// Gives up on the current binding: forgets the dead EGL state and revokes
    /// the binding so the render loop parks instead of hammering a display that
    /// is not coming back.
    fn abandon_binding(&self, inner: &mut GlesInner, reason: Error) -> Error {
        self.forget_display(inner);
        inner.revoked = true;
        bb_error!(
            "surface generation {} is unusable and was abandoned: {reason}; a new surfaceCreated (rotation, relaunch) is needed",
            inner.generation
        );
        reason
    }

    /// Recreates the window surface while keeping the display and context.
    ///
    /// This is the recovery for the failure mode that was actually observed: the
    /// `EGLContext` is still valid (libEGL's `eglMakeCurrent` checks the context
    /// *first* and reports `EGL_BAD_CONTEXT` when that is the missing object) but
    /// our `EGLSurface` is gone. Keeping the context is what keeps the game's
    /// textures, shaders and VAOs alive, which is the promise
    /// `GraphicsConfig::preserve_context` makes.
    fn recreate_surface_only(&self, inner: &mut GlesInner) -> Result<()> {
        match (
            inner.binding.as_mut(),
            inner.display.as_ref(),
            inner.config.as_ref(),
        ) {
            (Some(binding), Some(display), Some(config)) => {
                binding.recreate_surface(display, config)
            }
            _ => Err(Error::NoSurface),
        }
    }

    /// Tries to recover from a failed attach, within the documented bound.
    fn rebuild_and_retry(
        &self,
        inner: &mut GlesInner,
        thread: ThreadId,
        role: ThreadRole,
        first_error: Error,
    ) -> Result<()> {
        if inner.recoveries >= MAX_EGL_REBUILDS {
            return Err(self.abandon_binding(inner, first_error));
        }
        inner.recoveries += 1;
        let generation = inner.generation;

        // 1. Cheap path: the context survived, only the surface was destroyed
        //    behind our back. Recreate just that and keep the GL objects.
        if self.recreate_surface_only(inner).is_ok()
            && self.attach_current(inner, thread, role).is_ok()
        {
            bb_warn!(
                "EGL surface was destroyed behind the bridge; rebuilt it from the window (generation {generation})"
            );
            return Ok(());
        }

        // 2. The display itself is gone: forget the (dead) handles without
        //    calling into EGL and build a fresh display, context and surface.
        bb_warn!(
            "EGL is no longer usable ({first_error}); rebuilding it (attempt {}/{MAX_EGL_REBUILDS})",
            inner.recoveries
        );
        if let Err(error) = self.rebuild_display(inner) {
            return Err(self.abandon_binding(inner, error));
        }
        self.attach_current(inner, thread, role)
    }
}

impl GraphicsBackend for GlesBackend {
    fn name(&self) -> &'static str {
        "OpenGL ES"
    }

    fn kind(&self) -> RendererKind {
        RendererKind::Gles
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::Implemented
    }

    fn describe(&self) -> String {
        let inner = self.lock();
        match (inner.display.as_ref(), inner.config.as_ref()) {
            (Some(display), Some(config)) => {
                format!(
                    "{}; config {}",
                    display.info().describe(),
                    config.describe()
                )
            }
            (Some(display), None) => display.info().describe(),
            _ => "EGL display not initialized".to_string(),
        }
    }

    fn initialize(&self) -> Result<()> {
        let mut inner = self.lock();
        // Deliberately no EGL work here. The display, its context and the window
        // surface are created together on the first bind (`ensure_display`), so
        // the window in which another EGL user in this process can terminate the
        // shared default display under us is as small as it can be. This is a
        // behaviour change from the first Rust rewrite, which created EGL at
        // `createRuntime` — a second before the window existed — and is exactly
        // how a valid `EGLSurface` ended up unresolvable on the first frame.
        inner.revoked = true; // no window bound yet
        bb_debug!("EGL is created on the first surface bind (see docs/GRAPHICS.md)");
        Ok(())
    }

    fn bind_window(
        &self,
        window: OwnedNativeWindow,
        requested: SurfaceSize,
    ) -> Result<SurfaceSize> {
        // FIFO: a rotation delivers `surfaceCreated` before the previous
        // `surfaceDestroyed`, so the previous binding is retired here first.
        self.retire("replace")?;

        let mut inner = self.lock();
        // Create (or re-create) EGL here rather than at `initialize`: the
        // display, the context and the surface then belong to one short
        // sequence, which is the whole point of the late-creation rule.
        self.ensure_display(&mut inner)?;
        inner.generation += 1;
        let generation = inner.generation;
        inner.recoveries = 0;

        let created = match (inner.display.as_ref(), inner.config.as_ref()) {
            (Some(display), Some(config)) => {
                WindowBinding::new(display, config, window, generation)
            }
            _ => return Err(Error::NotInitialized),
        };

        match created {
            Ok(binding) => {
                let size = binding.size();
                inner.binding = Some(binding);
                inner.revoked = false;
                inner.requested_size = requested;
                bb_info!(
                    "ANativeWindow acquired, EGL surface bound (generation {generation}, {})",
                    size.label()
                );
                if requested.is_valid() && requested.differs_from(&size) {
                    bb_debug!(
                        "surface size differs: Kotlin reported {}, EGL reports {}",
                        requested.label(),
                        size.label()
                    );
                }
                Ok(size)
            }
            Err(error) => {
                inner.revoked = true;
                Err(error)
            }
        }
    }

    fn unbind_window(&self) -> Result<()> {
        self.retire("surfaceDestroyed")
    }

    fn has_window(&self) -> bool {
        let inner = self.lock();
        inner.binding.is_some() && !inner.revoked
    }

    fn binding_generation(&self) -> u64 {
        self.lock().generation
    }

    fn make_current(&self, role: ThreadRole) -> Result<()> {
        let mut inner = self.lock();
        let thread = thread::current().id();
        if let Some(owner) = inner.owner {
            if owner != thread {
                return Err(Error::SurfaceBusy);
            }
        }

        self.ensure_display(&mut inner)?;
        match self.attach_current(&mut inner, thread, role) {
            Ok(()) => Ok(()),
            // A failed `eglMakeCurrent` against a surface we own means the
            // display underneath us is gone (see the module docs), not that the
            // frame was unlucky: rebuild once and retry.
            Err(error) => self.rebuild_and_retry(&mut inner, thread, role, error),
        }
    }

    fn release_current(&self) -> Result<()> {
        let mut inner = self.lock();
        let thread = thread::current().id();
        if inner.owner != Some(thread) {
            // Not ours; still a chance to finish a deferred release.
            self.finish_deferred(&mut inner);
            return Ok(());
        }

        if let Some(display_raw) = inner.display.as_ref().map(|display| display.raw()) {
            let ok = unsafe {
                eglffi::eglMakeCurrent(
                    display_raw,
                    eglffi::EGL_NO_SURFACE,
                    eglffi::EGL_NO_SURFACE,
                    eglffi::EGL_NO_CONTEXT,
                )
            };
            if ok != eglffi::EGL_TRUE {
                bb_warn!("eglMakeCurrent(release) failed: 0x{:04x}", last_error());
            }
            // The thread is done with EGL until it attaches again.
            Context::release_thread();
        }

        inner.owner = None;
        inner.owner_role = None;
        self.finish_deferred(&mut inner);
        self.pending.notify_all();
        Ok(())
    }

    fn present(&self) -> Result<()> {
        let thread = thread::current().id();

        // Short critical section: validate and count, then drop the lock.
        let (display_raw, surface_raw) = {
            let mut inner = self.lock();
            let surface_raw = match inner.binding.as_ref() {
                Some(binding) => binding.surface().raw(),
                None => return Err(Error::NoSurface),
            };
            if inner.revoked {
                // The binding was retired while this frame was in progress.
                inner.rejected_presents += 1;
                return Err(Error::SurfaceRevoked);
            }
            let display_raw = match inner.display.as_ref() {
                Some(display) => display.raw(),
                None => return Err(Error::NotInitialized),
            };
            if inner.owner != Some(thread) {
                return Err(Error::InvalidState {
                    state: "PRESENT",
                    detail: "the EGL context is not current on the calling thread",
                });
            }
            inner.in_flight += 1;
            (display_raw, surface_raw)
        };

        // The GPU wait happens with the backend lock released.
        let mut in_flight = InFlight::new(self);
        let ok = unsafe { eglffi::eglSwapBuffers(display_raw, surface_raw) };
        in_flight.complete(ok == eglffi::EGL_TRUE);

        if ok == eglffi::EGL_TRUE {
            Ok(())
        } else {
            Err(Error::graphics("eglSwapBuffers", last_error()))
        }
    }

    fn window_size(&self) -> SurfaceSize {
        let inner = self.lock();
        match inner.binding.as_ref() {
            Some(binding) => binding.size(),
            None => inner.requested_size,
        }
    }

    fn refresh_window_size(&self) -> SurfaceSize {
        let mut inner = self.lock();
        match inner.binding.as_mut() {
            Some(binding) => binding.surface_mut().refresh_size(),
            None => inner.requested_size,
        }
    }

    fn renderer_info(&self) -> Result<RendererInfo> {
        let inner = self.lock();
        match inner.info.as_ref() {
            Some(info) => Ok(info.clone()),
            None => Err(Error::Message(
                "renderer info is not available yet (EGL context not created)".to_string(),
            )),
        }
    }

    fn stats(&self) -> GraphicsStats {
        let inner = self.lock();
        GraphicsStats {
            presents: inner.presents,
            present_failures: inner.present_failures,
            rejected_presents: inner.rejected_presents,
            generations: inner.generation,
            deferred_releases: inner.deferred_releases,
            in_flight: inner.in_flight,
            has_window: inner.binding.is_some() && !inner.revoked,
            revoked: inner.revoked,
            owner: inner.owner_role,
        }
    }

    fn set_swap_interval(&self, interval: i32) -> Result<()> {
        self.swap_interval.store(interval, Ordering::Relaxed);
        let inner = self.lock();
        if let Some(display) = inner.display.as_ref() {
            // Only the calling thread is affected; threads that attach later pick
            // the new value up in `make_current`.
            let ok = unsafe { eglffi::eglSwapInterval(display.raw(), interval) };
            if ok != eglffi::EGL_TRUE {
                bb_debug!(
                    "eglSwapInterval({interval}) refused: 0x{:04x}",
                    last_error()
                );
            }
        }
        Ok(())
    }

    fn shutdown(&self) {
        let _ = self.retire("shutdown");

        let mut inner = self.lock();
        // Explicit ordering: the GL surface resources go first, then the display.
        inner.binding = None;
        inner.pbuffer = None;
        inner.context = None;
        inner.deferred.clear();
        inner.config = None;
        inner.display = None;
        inner.owner = None;
        inner.owner_role = None;
        inner.revoked = true;
        self.pending.notify_all();
        bb_info!("GLES backend shut down");
    }
}

/// `glGetString` result as a Rust string.
fn gl_string(name: gl::GLenum) -> Option<String> {
    let ptr = unsafe { gl::glGetString(name) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: GL returns a NUL-terminated string owned by the driver for the
    // lifetime of the context.
    let value = unsafe { CStr::from_ptr(ptr as *const core::ffi::c_char) };
    Some(value.to_string_lossy().into_owned())
}

/// Parses `"OpenGL ES 3.2 …"` when `GL_MAJOR_VERSION` is not available.
fn parse_es_version(gl_version: &str) -> Option<(i32, i32)> {
    let marker = "OpenGL ES ";
    let start = gl_version.find(marker)? + marker.len();
    let rest = &gl_version[start..];
    let mut parts = rest.split(['.', ' ', 'v']);
    let major = parts.next()?.parse::<i32>().ok()?;
    let minor = parts.next()?.parse::<i32>().ok()?;
    Some((major, minor))
}

impl Drop for GlesBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_es_versions_from_gl_strings() {
        assert_eq!(parse_es_version("OpenGL ES 3.2 v1.r38p1"), Some((3, 2)));
        assert_eq!(parse_es_version("OpenGL ES 3.0"), Some((3, 0)));
        assert_eq!(parse_es_version("garbage"), None);
        assert_eq!(parse_es_version("OpenGL ES 3"), None);
    }

    #[test]
    fn fresh_backend_is_unbound_and_has_no_window() {
        let backend = GlesBackend::new(GraphicsConfig::default());
        let stats = backend.stats();
        assert_eq!(stats.presents, 0);
        assert!(!stats.has_window);
        assert!(stats.revoked);
        assert_eq!(stats.owner, None);
        assert_eq!(backend.binding_generation(), 0);
        assert_eq!(backend.window_size(), SurfaceSize::default());
    }

    #[test]
    fn presenting_before_binding_is_refused() {
        let backend = GlesBackend::new(GraphicsConfig::default());
        // No EGL, no binding: `present` must report "no surface", never crash.
        let error = backend
            .present()
            .expect_err("present without a surface must fail");
        assert_eq!(error, Error::NoSurface);
        // make_current without initialization must say so.
        let error = backend
            .make_current(ThreadRole::Game)
            .expect_err("make_current before initialize must fail");
        assert_eq!(error, Error::NotInitialized);
    }

    #[test]
    fn shutdown_is_idempotent() {
        let backend = GlesBackend::new(GraphicsConfig::default());
        backend.shutdown();
        backend.shutdown();
        assert!(!backend.has_window());
    }
}
