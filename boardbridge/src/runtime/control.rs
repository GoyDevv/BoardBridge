// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! The bridge thread and the `Runtime` handle Kotlin talks to.
//!
//! # Why a bridge thread at all
//!
//! EGL surface lifecycle work must not run on the Android UI thread (it can
//! block on a driver call, and it must be serialized), and it must not run on the
//! game thread either (Minecraft blocks for seconds at a time in world
//! generation). So the bridge owns one thread whose only jobs are:
//!
//! 1. create/destroy the graphics backend;
//! 2. process a FIFO queue of surface commands;
//! 3. optionally run the diagnostic render loop.
//!
//! # The surface fences, precisely
//!
//! `surfaceDestroyed` (UI thread) →
//!
//! ```text
//! 1. lifecycle: SURFACE_ACTIVE → SURFACE_DESTROY_PENDING
//! 2. queue:     Detach                                  (FIFO, always after any Attach)
//! 3. wait:      up to `detach_timeout_ms` for the command to complete
//!    └─ on timeout: log, return anyway (the bridge finishes the release later)
//! ```
//!
//! Bridge thread on `Detach` →
//!
//! ```text
//! 4. mark the binding revoked        (no new present can start)
//! 5. wait ≤ `drain_timeout_ms` for in-flight presents and for the owning thread
//!    ├─ released  → eglDestroySurface, then ANativeWindow_release
//!    └─ still held → keep the window reference, retry when that thread detaches
//! 6. lifecycle: … → SURFACE_DESTROY_PENDING
//! 7. queue input notice SurfaceRevoked  (the game is told, not interrupted)
//! ```
//!
//! There is no "sleep and hope" step anywhere, and no path that releases a
//! window while EGL may still reference it. Rotating as fast as Android can
//! deliver callbacks only ever leaves *stale* work in the queue, which the epoch
//! check drops (`docs/THREADING.md`).

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::android::input::KeyInput;
use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::bb_debug;
use crate::bb_error;
use crate::bb_info;
use crate::bb_warn;
use crate::error::{Error, Result};
use crate::graphics::{self, GraphicsBackend, ThreadRole};
use crate::input::{EventQueue, InputEvent, LifecycleNotice, ModifierState, QueueStats};
use crate::lifecycle::{ActivityState, Lifecycle, LifecycleEvent, Outcome, SurfaceState};
use crate::render::{DiagnosticMode, DiagnosticRenderer};
use crate::runtime::config::{LoopMode, RuntimeConfig};
use crate::runtime::ABI_VERSION;

/// Thread name shown in `/proc/<pid>/task/*/comm` and in crash reports.
const BRIDGE_THREAD_NAME: &str = "BoardBridge";

/// At most this many input events are consumed per diagnostic frame. The queue
/// keeps the rest (and drops the oldest if it overflows), so a burst cannot stall
/// the loop.
const MAX_INPUT_PER_FRAME: usize = 256;

/// One instruction for the bridge thread.
struct Command {
    /// Monotonic id; the issuer waits for `completed >= id`.
    id: u64,
    kind: CommandKind,
}

enum CommandKind {
    /// A new window is available (ownership of the reference is transferred).
    Attach {
        window: OwnedNativeWindow,
        size: SurfaceSize,
        epoch: u64,
    },
    /// The surface was resized.
    Resize { size: SurfaceSize },
    /// The surface went away.
    Detach,
    /// Diagnostic render mode changed.
    SetDiagnosticMode(DiagnosticMode),
    /// Loop ownership changed.
    SetLoopMode(LoopMode),
    /// Swap interval changed.
    SetSwapInterval(i32),
    /// Shut the bridge thread down.
    Shutdown,
}

/// Bridge-thread counters, published for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Frames drawn by the diagnostic renderer.
    pub frames: u64,
    /// Diagnostic frames per second × 100.
    pub fps_hundredths: u32,
    /// Window bindings created by this runtime.
    pub binds: u64,
    /// Bindings created while another binding was already live (rotation).
    pub rebinds: u64,
    /// Surface attaches that were dropped as superseded (rotation storms).
    pub dropped_attaches: u64,
    /// `surfaceDestroyed` fences that had to give up waiting.
    pub detach_timeouts: u64,
    /// Surface releases the backend had to defer for another thread.
    pub deferred_releases: u64,
    /// Last surface size the bridge bound.
    pub last_size: SurfaceSize,
    /// Last diagnostic centre pixel.
    pub center_pixel: [u8; 4],
    /// Current diagnostic mode.
    pub diagnostic_mode: DiagnosticMode,
}

impl Default for RuntimeStats {
    fn default() -> Self {
        RuntimeStats {
            frames: 0,
            fps_hundredths: 0,
            binds: 0,
            rebinds: 0,
            dropped_attaches: 0,
            detach_timeouts: 0,
            deferred_releases: 0,
            last_size: SurfaceSize::default(),
            center_pixel: [0, 0, 0, 0],
            diagnostic_mode: DiagnosticMode::Solid,
        }
    }
}

impl RuntimeStats {
    /// Compact `k=v` summary.
    pub fn summary(&self) -> String {
        format!(
            "frames={} fps={:.2} binds={} rebinds={} dropped_attaches={} detach_timeouts={} deferred={} size={} pixel={:?}",
            self.frames,
            self.fps_hundredths as f32 / 100.0,
            self.binds,
            self.rebinds,
            self.dropped_attaches,
            self.detach_timeouts,
            self.deferred_releases,
            self.last_size.label(),
            self.center_pixel
        )
    }
}

/// Lifecycle + command state, guarded by one mutex.
struct ControlState {
    lifecycle: Lifecycle,
    commands: VecDeque<Command>,
    next_id: u64,
    completed: u64,
    thread_running: bool,
    stopping: bool,
    loop_mode: LoopMode,
    diagnostic_mode: DiagnosticMode,
    target_fps: u32,
    stats: RuntimeStats,
}

/// Cached strings for diagnostics output.
#[derive(Clone, Debug, Default)]
pub struct RuntimeInfo {
    /// Backend display name, e.g. `"OpenGL ES"`.
    pub backend_name: String,
    /// `"implemented"` or `"interface-only"`.
    pub backend_status: String,
    /// Remaining work for an interface-only backend.
    pub backend_remaining: String,
    /// Backend-specific description (EGL version/vendor/config).
    pub backend_detail: String,
    /// `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`.
    pub renderer_info: String,
    /// `ES 3.2 via ES 3.2 (Mali-G52)`.
    pub renderer_detail: String,
    /// Last error message worth reporting to the launcher.
    pub last_error: String,
}

/// State shared between the JNI threads and the bridge thread.
pub struct Shared {
    config: RuntimeConfig,
    control: Mutex<ControlState>,
    signal: Condvar,
    input: EventQueue,
    modifiers: Mutex<ModifierState>,
    graphics: Mutex<Option<Arc<dyn GraphicsBackend>>>,
    info: Mutex<RuntimeInfo>,
}

impl Shared {
    fn new(config: RuntimeConfig) -> Shared {
        let control = ControlState {
            lifecycle: Lifecycle::new(),
            commands: VecDeque::new(),
            next_id: 0,
            completed: 0,
            thread_running: false,
            stopping: false,
            loop_mode: config.loop_mode,
            diagnostic_mode: config.diagnostic_mode,
            target_fps: config.target_fps,
            stats: RuntimeStats::default(),
        };
        Shared {
            input: EventQueue::new(config.input_capacity),
            modifiers: Mutex::new(ModifierState::new()),
            graphics: Mutex::new(None),
            info: Mutex::new(RuntimeInfo::default()),
            control: Mutex::new(control),
            signal: Condvar::new(),
            config,
        }
    }

    /// Configuration this runtime was created with.
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    fn lock(&self) -> MutexGuard<'_, ControlState> {
        self.control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn info_lock(&self) -> MutexGuard<'_, RuntimeInfo> {
        self.info
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn modifiers_lock(&self) -> MutexGuard<'_, ModifierState> {
        self.modifiers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The graphics backend, once the bridge thread has created it.
    pub fn backend(&self) -> Result<Arc<dyn GraphicsBackend>> {
        let slot = self
            .graphics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match slot.as_ref() {
            Some(backend) => Ok(Arc::clone(backend)),
            None => Err(Error::NotInitialized),
        }
    }

    /// Input queue counter snapshot.
    pub fn queue_stats(&self) -> QueueStats {
        self.input.stats()
    }

    /// The input queue itself.
    ///
    /// Exposed for the platform layer: a game-facing backend (SDL3, GLFW
    /// compatibility) polls events from exactly this queue, which is why the
    /// queue is internally synchronized instead of living behind this struct's
    /// control mutex.
    pub fn input_queue(&self) -> &EventQueue {
        &self.input
    }

    /// Feeds a lifecycle event into the shared machine.
    fn on_lifecycle(&self, event: LifecycleEvent) -> Outcome {
        let mut state = self.lock();
        let outcome = state.lifecycle.on(event);
        match outcome {
            Outcome::Applied(state_after) => {
                bb_debug!(
                    "lifecycle: {} -> {} ({})",
                    event.name(),
                    state_after.name(),
                    state.lifecycle.reported_state()
                );
            }
            Outcome::Ignored(reason) => bb_debug!("lifecycle: {} ignored ({reason})", event.name()),
            Outcome::Refused(reason) => bb_warn!("lifecycle: {} refused ({reason})", event.name()),
        }
        outcome
    }

    /// Replaces queued lifecycle notices and appends a new one.
    fn push_lifecycle_notice(&self, notice: LifecycleNotice) {
        self.input.remove_lifecycle_notices();
        self.input.push(InputEvent::Lifecycle(notice));
    }

    /// Mutates the stats block.
    fn record<F: FnOnce(&mut RuntimeStats)>(&self, update: F) {
        let mut state = self.lock();
        update(&mut state.stats);
    }

    fn record_error(&self, error: &Error) {
        let mut info = self.info_lock();
        info.last_error = error.detail();
    }

    fn publish_renderer_info(&self, summary: String, detail: String) {
        let mut info = self.info_lock();
        if info.renderer_info != summary {
            info.renderer_info = summary;
        }
        if info.renderer_detail != detail {
            info.renderer_detail = detail;
        }
    }
}

/// A live bridge runtime.
pub struct Runtime {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    /// Starts the bridge thread.
    pub fn start(config: RuntimeConfig) -> Result<Runtime> {
        let shared = Arc::new(Shared::new(config.clone()));
        let thread_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name(BRIDGE_THREAD_NAME.to_string())
            .spawn(move || bridge_thread_main(thread_shared))
            .map_err(|error| {
                Error::Message(format!("could not start the bridge thread: {error}"))
            })?;
        bb_info!("runtime created ({})", config.describe());
        Ok(Runtime {
            shared,
            thread: Some(handle),
        })
    }

    /// Shared state (also used by the platform layer).
    pub fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.shared)
    }

    /// Stops the bridge thread and waits for it (bounded by the backend's
    /// drain timeout, never unbounded).
    pub fn stop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.stopping = true;
            state.next_id += 1;
            let id = state.next_id;
            state.commands.push_back(Command {
                id,
                kind: CommandKind::Shutdown,
            });
        }
        self.shared.signal.notify_all();
        if let Some(handle) = self.thread.take() {
            if handle.join().is_err() {
                bb_error!("bridge thread panicked");
            }
        }
        bb_info!("runtime stopped");
    }

    // ---------------------------------------------------------------- surface

    /// `surfaceCreated`: queues the window for binding.
    ///
    /// Takes ownership of the window reference; it is released either by the
    /// backend or, when the event is refused, right here.
    pub fn surface_created(&self, window: OwnedNativeWindow, size: SurfaceSize) -> Result<()> {
        let epoch;
        {
            let mut state = self.shared.lock();
            match state.lifecycle.on(LifecycleEvent::SurfaceCreated) {
                Outcome::Applied(_) => {}
                Outcome::Ignored(reason) => {
                    bb_debug!("surfaceCreated ignored: {reason}");
                    return Ok(());
                }
                Outcome::Refused(reason) => {
                    let state_name = state.lifecycle.reported_state();
                    bb_warn!("surfaceCreated refused: {reason}");
                    return Err(Error::InvalidState {
                        state: state_name,
                        detail: reason,
                    });
                }
            }
            epoch = state.lifecycle.surface_epoch();
            state.next_id += 1;
            let id = state.next_id;
            state.commands.push_back(Command {
                id,
                kind: CommandKind::Attach {
                    window,
                    size,
                    epoch,
                },
            });
        }
        self.shared.signal.notify_all();
        bb_info!(
            "surface created: {} queued for binding (epoch {epoch})",
            size.label()
        );
        Ok(())
    }

    /// `surfaceChanged`: records the new size.
    pub fn surface_changed(&self, size: SurfaceSize) {
        self.queue(CommandKind::Resize { size });
        bb_debug!("surface changed: {}", size.label());
    }

    /// `surfaceDestroyed`: retires the window with a bounded fence.
    ///
    /// Returns as soon as the bridge confirms, or after
    /// `detach_timeout_ms` — whichever comes first. A timeout is logged, not
    /// treated as an error: the release completes on the bridge thread, and the
    /// window reference stays valid until it does.
    pub fn surface_destroyed(&self) -> Result<()> {
        let id;
        {
            let mut state = self.shared.lock();
            // The Detach command is queued in every case: even when the machine
            // is already shutting down, the window that is still held must be
            // released by the bridge thread.
            if let Outcome::Ignored(reason) = state.lifecycle.on(LifecycleEvent::SurfaceDestroyed) {
                bb_debug!("surfaceDestroyed: {reason}");
            }
            state.next_id += 1;
            id = state.next_id;
            state.commands.push_back(Command {
                id,
                kind: CommandKind::Detach,
            });
        }
        self.shared.signal.notify_all();
        self.wait_for(id, self.shared.config.detach_timeout_ms)
    }

    // --------------------------------------------------------------- activity

    /// `onPause`.
    pub fn pause(&self) {
        self.shared.on_lifecycle(LifecycleEvent::Pause);
        self.shared.push_lifecycle_notice(LifecycleNotice::Paused {
            timestamp_ms: now_ms(),
        });
    }

    /// `onResume`.
    pub fn resume(&self) {
        self.shared.on_lifecycle(LifecycleEvent::Resume);
        self.shared.push_lifecycle_notice(LifecycleNotice::Resumed {
            timestamp_ms: now_ms(),
        });
    }

    // ------------------------------------------------------------ game thread

    /// Attaches the calling (game) thread: makes the context current.
    pub fn attach_game_thread(&self) -> Result<()> {
        self.shared.backend()?.make_current(ThreadRole::Game)
    }

    /// Detaches the calling (game) thread from EGL.
    pub fn detach_game_thread(&self) -> Result<()> {
        self.shared.backend()?.release_current()
    }

    /// Presents from the calling (game) thread.
    pub fn present(&self) -> Result<()> {
        self.shared.backend()?.present()
    }

    // ------------------------------------------------------------ diagnostics

    /// Changes the diagnostic render mode.
    pub fn set_diagnostic_mode(&self, mode: DiagnosticMode) {
        self.queue(CommandKind::SetDiagnosticMode(mode));
    }

    /// Changes which loop draws.
    ///
    /// Switching hands the context over: the bridge thread releases it before
    /// acknowledging, so a game thread that attaches afterwards is not refused.
    pub fn set_loop_mode(&self, mode: LoopMode) {
        self.queue(CommandKind::SetLoopMode(mode));
    }

    /// Changes the swap interval.
    ///
    /// Applies it to the backend immediately (which is what the calling thread and
    /// every later attach use) and tells the bridge thread so its own loop sees it.
    pub fn set_swap_interval(&self, interval: i32) -> Result<()> {
        if let Ok(backend) = self.shared.backend() {
            backend.set_swap_interval(interval)?;
        }
        self.queue(CommandKind::SetSwapInterval(interval));
        Ok(())
    }

    /// Which graphics backend the bridge thread actually created.
    pub fn renderer_kind(&self) -> Result<crate::graphics::RendererKind> {
        self.shared.backend().map(|backend| backend.kind())
    }

    /// `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`, or `""` before the context
    /// exists (the Kotlin side retries, as it did before).
    pub fn renderer_info(&self) -> String {
        if let Ok(backend) = self.shared.backend() {
            match backend.renderer_info() {
                Ok(info) => {
                    let summary = info.summary();
                    self.shared
                        .publish_renderer_info(summary.clone(), info.describe());
                    return summary;
                }
                Err(error) => {
                    bb_debug!("renderer info not ready: {error}");
                }
            }
        }
        self.shared.info_lock().renderer_info.clone()
    }

    /// One-line status for `NativeBridge.getStatus()`.
    pub fn status_line(&self) -> String {
        let (lifecycle, loop_mode, diagnostic_mode, stats, running) = {
            let state = self.shared.lock();
            (
                state.lifecycle.summary(),
                state.loop_mode,
                state.diagnostic_mode,
                state.stats,
                state.thread_running,
            )
        };
        let graphics = match self.shared.backend() {
            Ok(backend) => backend.stats().summary(),
            Err(_) => "uninitialized".to_string(),
        };
        let queue = self.shared.queue_stats();
        let info = self.shared.info_lock().clone();
        format!(
            "abi={ABI_VERSION} thread_running={running} {lifecycle} loop={} diag={} graphics=[{graphics}] input=[depth={} pushed={} popped={} dropped={} cap={}] stats=[{}] backend={} backend_status={} egl=[{}] renderer=[{}]",
            loop_mode.name(),
            diagnostic_mode.name(),
            queue.depth,
            queue.pushed,
            queue.popped,
            queue.dropped,
            queue.capacity,
            stats.summary(),
            if info.backend_name.is_empty() {
                "?"
            } else {
                &info.backend_name
            },
            info.backend_status,
            info.backend_detail,
            info.renderer_info
        )
    }

    /// Read-only diagnostic audit.
    ///
    /// This is deliberately an *audit* rather than a scripted lifecycle test: the
    /// lifecycle paths that matter (surface recreation, pause/resume) are driven
    /// by Android through the emulator workflow in
    /// `.github/workflows/render-test.yml`, where rotation and backgrounding are
    /// real. What this can prove is that every subsystem is up and consistent.
    pub fn self_test(&self) -> String {
        let (lifecycle, loop_mode, diagnostic_mode, stats, running) = {
            let state = self.shared.lock();
            (
                state.lifecycle.clone(),
                state.loop_mode,
                state.diagnostic_mode,
                state.stats,
                state.thread_running,
            )
        };
        let queue = self.shared.queue_stats();
        let info = self.shared.info_lock().clone();
        let backend = self.shared.backend();
        let (graphics_summary, backend_ok, egl_ok, renderer_ok) = match backend.as_ref() {
            Ok(backend) => {
                let status = backend.status();
                let renderer = backend.renderer_info().is_ok();
                (
                    backend.stats().summary(),
                    status.is_implemented(),
                    status.is_implemented(),
                    renderer,
                )
            }
            Err(_) => ("uninitialized".to_string(), false, false, false),
        };

        let verdict = if backend_ok && egl_ok && renderer_ok && running {
            "PASS"
        } else {
            "PARTIAL"
        };

        let mut report = String::new();
        report.push_str(&format!("BoardBridge self-test (abi={ABI_VERSION})\n"));
        report.push_str(&format!("  thread_running={running}\n"));
        report.push_str(&format!("  lifecycle: {}\n", lifecycle.summary()));
        report.push_str(&format!(
            "  loop={} diag={}\n",
            loop_mode.name(),
            diagnostic_mode.name()
        ));
        report.push_str(&format!(
            "  backend: {} ({}){}\n",
            if info.backend_name.is_empty() {
                "?"
            } else {
                &info.backend_name
            },
            info.backend_status,
            if info.backend_remaining.is_empty() {
                String::new()
            } else {
                format!(" remaining={}", info.backend_remaining)
            }
        ));
        report.push_str(&format!("  egl: {}\n", info.backend_detail));
        report.push_str(&format!(
            "  renderer: {}{}\n",
            if info.renderer_info.is_empty() {
                "(not available yet)"
            } else {
                &info.renderer_info
            },
            if info.renderer_detail.is_empty() {
                String::new()
            } else {
                format!(" [{}]", info.renderer_detail)
            }
        ));
        report.push_str(&format!("  graphics: {graphics_summary}\n"));
        report.push_str(&format!(
            "  input: depth={} pushed={} popped={} dropped={}\n",
            queue.depth, queue.pushed, queue.popped, queue.dropped
        ));
        report.push_str(&format!("  stats: {}\n", stats.summary()));
        report.push_str(&format!(
            "  checks: thread={} backend={} egl={} renderer_info={} surface={} input_queue={}\n",
            pass(running),
            pass(backend_ok),
            pass(egl_ok),
            pass(renderer_ok),
            if lifecycle.is_bound() {
                "bound"
            } else {
                "none"
            },
            if queue.capacity > 0 {
                "ok"
            } else {
                "misconfigured"
            }
        ));
        if !info.last_error.is_empty() {
            report.push_str(&format!("  last_error: {}\n", info.last_error));
        }
        report.push_str(&format!("result={verdict}"));
        report
    }

    // ------------------------------------------------------------------ input

    /// Applies a key event to the modifier state and builds the SDL event.
    pub fn key_input(&self, mut key: KeyInput) -> InputEvent {
        {
            let mut modifiers = self.shared.modifiers_lock();
            key.modifiers = crate::android::input::update_modifiers(
                &mut modifiers,
                key.android_keycode,
                key.pressed,
                key.repeat,
            );
        }
        crate::android::input::key_event(key)
    }

    /// Pushes one event onto the input queue; `false` means it evicted an older
    /// event (the queue is bounded on purpose).
    pub fn push_input(&self, event: InputEvent) -> bool {
        self.shared.input.push(event)
    }

    /// Current modifier state (`SDL_KMOD_*`).
    pub fn modifiers(&self) -> u16 {
        self.shared.modifiers_lock().bits()
    }

    fn queue(&self, kind: CommandKind) {
        {
            let mut state = self.shared.lock();
            state.next_id += 1;
            let id = state.next_id;
            state.commands.push_back(Command { id, kind });
        }
        self.shared.signal.notify_all();
    }

    /// Waits (bounded) for a command id to complete.
    fn wait_for(&self, id: u64, timeout_ms: u64) -> Result<()> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.lock();
        while state.completed < id {
            let now = Instant::now();
            if now >= deadline {
                state.stats.detach_timeouts += 1;
                let reported = state.lifecycle.reported_state();
                bb_warn!(
                    "surface fence timed out after {timeout_ms} ms in {reported}; the bridge will finish the release asynchronously"
                );
                return Ok(());
            }
            let remaining = deadline - now;
            let (guard, _) = self
                .shared
                .signal
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = guard;
        }
        Ok(())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop();
        }
    }
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "ok"
    } else {
        "failed"
    }
}

fn now_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

/// The bridge thread.
fn bridge_thread_main(shared: Arc<Shared>) {
    let config = shared.config.clone();
    let backend = graphics::create(config.renderer, config.graphics_config());
    let status = backend.status();
    {
        let mut info = shared.info_lock();
        info.backend_name = backend.name().to_string();
        info.backend_status = status.label().to_string();
        info.backend_remaining = status.remaining().unwrap_or("").to_string();
    }
    bb_info!(
        "graphics backend = {} [{}]; platform = android-native; input backend = native queue",
        backend.name(),
        status.label()
    );
    if let Some(remaining) = status.remaining() {
        bb_error!("{}", remaining);
    }

    if let Err(error) = backend.initialize() {
        bb_error!("graphics initialization failed: {error}");
        shared.record_error(&error);
        {
            let mut state = shared.lock();
            let _ = state.lifecycle.on(LifecycleEvent::StopRequested);
            let _ = state.lifecycle.on(LifecycleEvent::Stopped);
            state.thread_running = false;
            state.completed = state.next_id;
            // Any queued window must be released: it will never be bound.
            while let Some(command) = state.commands.pop_front() {
                drop_command(command);
            }
        }
        shared.signal.notify_all();
        bb_error!("bridge thread exiting after initialization failure");
        return;
    }

    publish_backend_detail(&shared, backend.describe());
    {
        let mut slot = shared
            .graphics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(Arc::clone(&backend));
    }
    {
        let mut state = shared.lock();
        state.thread_running = true;
    }
    shared.signal.notify_all();

    let mut renderer = DiagnosticRenderer::new(config.diagnostic_mode);
    let mut events: Vec<InputEvent> = Vec::with_capacity(MAX_INPUT_PER_FRAME);
    let mut last_frame: Option<Instant> = None;
    let mut stopping = false;

    while !stopping {
        // ---- 1. Drain the command queue (FIFO). ----
        let commands = {
            let mut state = shared.lock();
            let mut drained = VecDeque::new();
            std::mem::swap(&mut drained, &mut state.commands);
            drained
        };
        let latest_attach = commands
            .iter()
            .filter_map(|command| match command.kind {
                CommandKind::Attach { epoch, .. } => Some(epoch),
                _ => None,
            })
            .max();

        for command in commands {
            let id = command.id;
            match command.kind {
                CommandKind::Attach {
                    window,
                    size,
                    epoch,
                } => {
                    if let Some(latest) = latest_attach {
                        if epoch < latest {
                            // Rotation storm: a newer surface is already queued.
                            bb_debug!(
                                "dropping superseded surface attach (epoch {epoch} < {latest})"
                            );
                            drop(window);
                            shared.record(|stats| stats.dropped_attaches += 1);
                            complete(&shared, id);
                            continue;
                        }
                    }
                    handle_attach(&shared, &backend, window, size);
                }
                CommandKind::Resize { size } => {
                    let actual = backend.refresh_window_size();
                    bb_debug!(
                        "surface resize: {} (EGL reports {})",
                        size.label(),
                        actual.label()
                    );
                    shared.record(|stats| stats.last_size = actual);
                }
                CommandKind::Detach => {
                    handle_detach(&shared, &backend, &mut renderer, &config);
                }
                CommandKind::SetDiagnosticMode(mode) => {
                    renderer.set_mode(mode);
                    let mut state = shared.lock();
                    state.diagnostic_mode = mode;
                }
                CommandKind::SetLoopMode(mode) => {
                    let previous = shared.lock().loop_mode;
                    bb_info!("loop mode: {} -> {}", previous.name(), mode.name());
                    // Hand the context over: the bridge is done rendering.
                    let _ = backend.release_current();
                    let mut state = shared.lock();
                    state.loop_mode = mode;
                }
                CommandKind::SetSwapInterval(interval) => {
                    let _ = backend.set_swap_interval(interval);
                }
                CommandKind::Shutdown => {
                    stopping = true;
                }
            }
            complete(&shared, id);
        }
        if stopping {
            break;
        }

        // ---- 2. Render a diagnostic frame when the bridge owns the loop. ----
        let (loop_mode, state_now, activity, target_fps) = {
            let state = shared.lock();
            (
                state.loop_mode,
                state.lifecycle.state(),
                state.lifecycle.activity(),
                state.target_fps,
            )
        };
        let should_render = !loop_mode.is_inverted()
            && state_now == SurfaceState::SurfaceActive
            && activity == ActivityState::Resumed
            && backend.has_window();

        if should_render {
            drain_input(&shared, &mut renderer, &mut events);
            // Cached size: refreshed only when a Resize command arrives, so the
            // frame path stays free of EGL queries.
            let size = backend.window_size();
            match backend.make_current(ThreadRole::Bridge) {
                Ok(()) => {
                    renderer.draw(size);
                    if let Some(line) = renderer.take_first_frame_log(size) {
                        bb_info!("{line}");
                    }
                    match backend.present() {
                        Ok(()) => {}
                        Err(Error::SurfaceRevoked) => {
                            bb_debug!("present skipped: the surface was revoked mid-frame");
                        }
                        Err(error) => bb_warn!("present failed: {error}"),
                    }
                    // Counters are published every window, whether or not the
                    // line is logged: `getStatus` must not go stale.
                    if let Some(line) = renderer.take_stats_log(size) {
                        if config.diagnostic_logs {
                            bb_info!("{line}");
                        }
                        publish_stats(&shared, &renderer, size);
                    }
                }
                Err(error) => {
                    bb_warn!("could not make the context current on the bridge thread: {error}");
                }
            }
            pace(&shared, &mut last_frame, target_fps);
        } else {
            // ---- 3. Nothing to draw: sleep until something changes. ----
            let mut state = shared.lock();
            if state.commands.is_empty() && !state.stopping {
                let _ = shared.signal.wait(state);
            }
        }
    }

    // ---- 4. Shutdown: release GL objects, the surface and the backend. ----
    if backend.has_window() && backend.make_current(ThreadRole::Bridge).is_ok() {
        renderer.release_gl();
    }
    let _ = backend.unbind_window();
    backend.shutdown();
    {
        let mut slot = shared
            .graphics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = None;
    }
    {
        let mut state = shared.lock();
        let _ = state.lifecycle.on(LifecycleEvent::Stopped);
        state.thread_running = false;
        state.stopping = true;
        state.completed = state.next_id;
    }
    shared.signal.notify_all();
    bb_info!("bridge thread exited");
}

fn publish_backend_detail(shared: &Shared, detail: String) {
    let mut info = shared.info_lock();
    info.backend_detail = detail;
}

/// Binds a window and reports the result to the lifecycle machine.
fn handle_attach(
    shared: &Shared,
    backend: &Arc<dyn GraphicsBackend>,
    window: OwnedNativeWindow,
    size: SurfaceSize,
) {
    let had_window = backend.has_window();
    match backend.bind_window(window, size) {
        Ok(actual) => {
            let outcome = shared.on_lifecycle(LifecycleEvent::SurfaceBound);
            if outcome.is_applied() {
                shared.input.remove_lifecycle_notices();
                shared.push_lifecycle_notice(LifecycleNotice::SurfaceAvailable {
                    width: actual.width,
                    height: actual.height,
                    timestamp_ms: now_ms(),
                });
                shared.record(|stats| {
                    stats.binds += 1;
                    if had_window {
                        stats.rebinds += 1;
                    }
                    stats.last_size = actual;
                });
            } else {
                // The machine did not want this binding (shutdown raced us):
                // give it back instead of keeping a stray EGL surface.
                bb_warn!(
                    "lifecycle refused the surface bind ({}); releasing it",
                    outcome.kind()
                );
                let _ = backend.unbind_window();
            }
        }
        Err(error) => {
            shared.on_lifecycle(LifecycleEvent::SurfaceBindFailed);
            shared.record_error(&error);
            bb_error!("failed to bind the surface: {error}");
        }
    }
}

/// Retires the current binding and tells the game about it.
fn handle_detach(
    shared: &Shared,
    backend: &Arc<dyn GraphicsBackend>,
    renderer: &mut DiagnosticRenderer,
    config: &RuntimeConfig,
) {
    let generation = backend.binding_generation();
    if backend.has_window() {
        if !config.preserve_context && backend.make_current(ThreadRole::Bridge).is_ok() {
            // Without context preservation the GL objects die with the context,
            // so release them while it is still current.
            renderer.release_gl();
        }
        match backend.unbind_window() {
            Ok(()) => bb_info!(
                "surface released (generation {generation}); ANativeWindow reference returned"
            ),
            Err(error) => bb_warn!("surface release reported: {error}"),
        }
        shared.on_lifecycle(LifecycleEvent::SurfaceUnbound);
    } else {
        bb_debug!("detach with no bound surface (generation {generation})");
    }

    // The old window's coordinates mean nothing; the game is told to stop
    // presenting through the input queue rather than interrupted.
    shared.input.clear();
    shared.push_lifecycle_notice(LifecycleNotice::SurfaceRevoked {
        timestamp_ms: now_ms(),
    });
    let deferred = backend.stats().deferred_releases;
    shared.record(|stats| stats.deferred_releases = deferred);
}

fn drain_input(shared: &Shared, renderer: &mut DiagnosticRenderer, events: &mut Vec<InputEvent>) {
    events.clear();
    let moved = shared.input.drain_into(events, MAX_INPUT_PER_FRAME);
    if moved > 0 {
        renderer.consume_input(events);
    }
}

fn publish_stats(shared: &Shared, renderer: &DiagnosticRenderer, size: SurfaceSize) {
    shared.record(|stats| {
        stats.frames = renderer.frame_count();
        stats.fps_hundredths = (renderer.fps() * 100.0) as u32;
        stats.center_pixel = renderer.center_pixel();
        stats.diagnostic_mode = renderer.mode();
        stats.last_size = size;
    });
}

/// Frame pacing for a capped frame rate.
///
/// Waits on the command condvar, so a command still wakes the loop immediately;
/// there is no spin and no `sleep(1/fps)` that ignores lifecycle changes.
fn pace(shared: &Shared, last_frame: &mut Option<Instant>, target_fps: u32) {
    let now = Instant::now();
    if target_fps > 0 {
        if let Some(previous) = *last_frame {
            let budget = Duration::from_micros(1_000_000u64 / target_fps.max(1) as u64);
            let elapsed = now.duration_since(previous);
            if elapsed < budget {
                let state = shared.lock();
                let _ = shared.signal.wait_timeout(state, budget - elapsed);
            }
        }
    }
    *last_frame = Some(now);
}

/// Marks a command id as complete and wakes waiters.
fn complete(shared: &Shared, id: u64) {
    {
        let mut state = shared.lock();
        if id > state.completed {
            state.completed = id;
        }
    }
    shared.signal.notify_all();
}

/// Drops a queued command, releasing any window reference it carries.
fn drop_command(command: Command) {
    if let CommandKind::Attach { window, .. } = command.kind {
        drop(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_summary_is_log_friendly() {
        let stats = RuntimeStats {
            frames: 120,
            fps_hundredths: 5990,
            binds: 2,
            rebinds: 1,
            dropped_attaches: 3,
            detach_timeouts: 0,
            deferred_releases: 1,
            last_size: SurfaceSize::new(1080, 2400),
            center_pixel: [0, 158, 166, 255],
            diagnostic_mode: DiagnosticMode::Solid,
        };
        let summary = stats.summary();
        assert!(summary.contains("frames=120"));
        assert!(summary.contains("fps=59.90"));
        assert!(summary.contains("size=1080x2400"));
        assert!(summary.contains("dropped_attaches=3"));
    }

    #[test]
    fn shared_starts_with_an_empty_queue_and_no_backend() {
        let shared = Shared::new(RuntimeConfig::default());
        assert!(shared.backend().is_err());
        assert!(shared.input.is_empty());
        assert_eq!(
            shared.queue_stats().capacity,
            RuntimeConfig::default().input_capacity
        );
        assert_eq!(shared.modifiers_lock().bits(), 0);
    }

    #[test]
    fn a_fresh_runtime_reports_its_lifecycle_and_abi() {
        let runtime = Runtime::start(RuntimeConfig::default()).expect("bridge thread starts");
        let status = runtime.status_line();
        assert!(status.contains("abi=2"), "status was {status}");
        assert!(status.contains("NO_SURFACE"));
        assert!(status.contains("loop=internal"));
        assert!(runtime.renderer_info().is_empty());
        let report = runtime.self_test();
        // The bridge thread needs a moment to fail/succeed at EGL; on a CI host
        // without EGL both outcomes are legitimate, so only the shape is checked.
        assert!(report.starts_with("BoardBridge self-test"));
        assert!(report.contains("result="));
    }
}
