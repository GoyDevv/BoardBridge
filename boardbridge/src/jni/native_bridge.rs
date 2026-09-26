// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! JNI entry points for `com.boardbridge.bridge.NativeBridge`.
//!
//! Each function here has exactly one counterpart in Kotlin. The mapping is
//! mechanical (`Java_com_boardbridge_bridge_NativeBridge_<method>`), which is why
//! the Kotlin object is a plain `object` with `external fun`s: no instance
//! handle, no registration step, no `jclass` caching.
//!
//! | Kotlin | Purpose |
//! |---|---|
//! | `createRuntime` / `destroyRuntime` | runtime lifetime |
//! | `surfaceCreated` / `surfaceChanged` / `surfaceDestroyed` | window lifecycle |
//! | `onPause` / `onResume` | activity visibility |
//! | `attachGameThread` / `detachGameThread` / `swapBuffers` | frame-loop inversion |
//! | `sendTouchBatch` / `sendMouse*` / `sendKey` / `sendText` / `sendGamepadAxis` | input |
//! | `setRenderer` / `setRenderMode` / `setRenderLoopMode` / `setSwapInterval` | configuration |
//! | `setLogLevel` | diagnostics verbosity |
//! | `getRendererInfo` / `getStatus` / `runSelfTest` | diagnostics |

// JNI fixes the shape of an exported function: the argument list is whatever the
// Kotlin `external fun` declares, and the first two parameters are always the
// `JNIEnv` and the class. Splitting `sendKey`/`sendTouchBatch` into structs would
// not change that signature, only hide it, so the lint is silenced here rather
// than worked around.
#![allow(clippy::too_many_arguments)]

use core::ffi::c_void;

use jni::objects::{JClass, JFloatArray, JIntArray, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, jstring};
use jni::JNIEnv;

use crate::android::input::{self as android_input, KeyInput};
use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::bb_debug;
use crate::bb_error;
use crate::bb_info;
use crate::bb_warn;
use crate::error::{Error, Result};
use crate::graphics::RendererKind;
use crate::input::event::{DeviceKind, TouchPhase};
use crate::input::keymap;
use crate::jni::helpers::{self, OK};
use crate::log::Level;
use crate::platform;
use crate::render::DiagnosticMode;
use crate::runtime::config::{LoopMode, RuntimeConfig};
use crate::runtime::{registry, ABI_VERSION};

/// Upper bound on pointers per touch batch; Android allows 10–16 in practice.
/// Bounded so a malformed call cannot make the bridge allocate without limit.
const MAX_TOUCH_POINTERS: i32 = 32;

/// Library load hook: logs the ABI version and tells the JVM which JNI level is
/// needed (`JNI_VERSION_1_6`; the bridge uses no newer feature).
#[no_mangle]
pub extern "system" fn JNI_OnLoad(_vm: *mut c_void, _reserved: *mut c_void) -> jint {
    bb_info!("libboardbridge loaded (abi={ABI_VERSION})");
    0x0001_0006
}

/// Library unload hook: destroys the runtime if Kotlin never did.
#[no_mangle]
pub extern "system" fn JNI_OnUnload(_vm: *mut c_void, _reserved: *mut c_void) {
    if registry::exists() {
        bb_warn!("JNI_OnUnload: destroying a runtime that Kotlin left alive");
        if let Err(error) = registry::destroy() {
            bb_error!("JNI_OnUnload could not destroy the runtime: {error}");
        }
    }
}

// ------------------------------------------------------------------ lifetime

/// `createRuntime(renderer, loopMode, flags, diagnosticMode, targetFps): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_createRuntime<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    renderer: jint,
    loop_mode: jint,
    flags: jint,
    diagnostic_mode: jint,
    target_fps: jint,
) -> jint {
    helpers::guarded("createRuntime", || {
        let config =
            RuntimeConfig::from_jni(renderer, loop_mode, flags, diagnostic_mode, target_fps);
        helpers::report("createRuntime", registry::create(config))
    })
}

/// `destroyRuntime(): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_destroyRuntime<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("destroyRuntime", || {
        helpers::report("destroyRuntime", registry::destroy())
    })
}

// ------------------------------------------------------------------- surface

/// `surfaceCreated(surface: Surface): Int`
///
/// Acquires the `ANativeWindow` reference here, on the calling (UI) thread, and
/// hands ownership to the runtime. If the lifecycle refuses the event, the
/// reference is released by dropping it.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_surfaceCreated<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    surface: JObject<'local>,
) -> jint {
    helpers::guarded("surfaceCreated", || {
        let window = unsafe {
            OwnedNativeWindow::from_surface(
                env.get_raw() as crate::android::ffi::JniEnvPtr,
                surface.into_raw() as crate::android::ffi::JObjectPtr,
            )
        };
        let window = match window {
            Some(window) => window,
            None => {
                bb_error!("ANativeWindow_fromSurface returned null (is the Surface already gone?)");
                return helpers::report("surfaceCreated", Err(Error::NoSurface));
            }
        };
        let size = window.size();
        helpers::report(
            "surfaceCreated",
            registry::try_with(|runtime| runtime.surface_created(window, size)),
        )
    })
}

/// `surfaceChanged(width, height, format): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_surfaceChanged<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    width: jint,
    height: jint,
    format: jint,
) -> jint {
    helpers::guarded("surfaceChanged", || {
        let size = SurfaceSize::new(width, height);
        bb_debug!("surfaceChanged: {} (buffer format 0x{format:x})", size.label());
        match registry::with_runtime(|runtime| runtime.surface_changed(size)) {
            Some(()) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `surfaceDestroyed(): Int`
///
/// Blocks the calling thread for at most the configured fence
/// (`detach_timeout_ms`, 250 ms by default) while the bridge retires the window.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_surfaceDestroyed<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("surfaceDestroyed", || {
        helpers::report(
            "surfaceDestroyed",
            registry::try_with(|runtime| runtime.surface_destroyed()),
        )
    })
}

// ------------------------------------------------------------------ activity

/// `onPause(): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_onPause<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("onPause", || {
        match registry::with_runtime(|runtime| runtime.pause()) {
            Some(()) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `onResume(): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_onResume<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("onResume", || {
        match registry::with_runtime(|runtime| runtime.resume()) {
            Some(()) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

// ------------------------------------------------------- frame-loop inversion

/// `attachGameThread(): Int` — make the EGL context current on this thread.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_attachGameThread<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("attachGameThread", || {
        helpers::report(
            "attachGameThread",
            registry::try_with(|runtime| runtime.attach_game_thread()),
        )
    })
}

/// `detachGameThread(): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_detachGameThread<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("detachGameThread", || {
        helpers::report(
            "detachGameThread",
            registry::try_with(|runtime| runtime.detach_game_thread()),
        )
    })
}

/// `swapBuffers(): Int` — present from the calling (game) thread.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_swapBuffers<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    helpers::guarded("swapBuffers", || {
        helpers::report(
            "swapBuffers",
            registry::try_with(|runtime| runtime.present()),
        )
    })
}

// --------------------------------------------------------------------- input

/// `sendTouchBatch(pointerIds, phases, xs, ys, pressures, count, eventTimeMs): Int`
///
/// One JNI call per `MotionEvent` instead of one per pointer: a ten-finger
/// `ACTION_MOVE` batch used to be ten round trips.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendTouchBatch<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    pointer_ids: JIntArray<'local>,
    phases: JIntArray<'local>,
    xs: JFloatArray<'local>,
    ys: JFloatArray<'local>,
    pressures: JFloatArray<'local>,
    count: jint,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendTouchBatch", || {
        let count = count.clamp(0, MAX_TOUCH_POINTERS) as usize;
        if count == 0 {
            return OK;
        }
        let mut ids = vec![0i32; count];
        let mut phase_values = vec![0i32; count];
        let mut x_values = vec![0f32; count];
        let mut y_values = vec![0f32; count];
        let mut pressure_values = vec![0f32; count];

        if let Err(error) = env.get_int_array_region(&pointer_ids, 0, &mut ids) {
            return invalid_argument("pointerIds", format!("{error}"));
        }
        if let Err(error) = env.get_int_array_region(&phases, 0, &mut phase_values) {
            return invalid_argument("phases", format!("{error}"));
        }
        if let Err(error) = env.get_float_array_region(&xs, 0, &mut x_values) {
            return invalid_argument("xs", format!("{error}"));
        }
        if let Err(error) = env.get_float_array_region(&ys, 0, &mut y_values) {
            return invalid_argument("ys", format!("{error}"));
        }
        if let Err(error) = env.get_float_array_region(&pressures, 0, &mut pressure_values) {
            return invalid_argument("pressures", format!("{error}"));
        }

        registry::with_runtime(|runtime| {
            for index in 0..count {
                let phase = match TouchPhase::from_jni(phase_values[index]) {
                    Some(phase) => phase,
                    None => continue,
                };
                let event = android_input::touch_event(
                    ids[index],
                    phase,
                    x_values[index],
                    y_values[index],
                    pressure_values[index],
                    event_time_ms,
                );
                runtime.push_input(event);
            }
        });
        OK
    })
}

/// `sendMouseMotion(relative, x, y, androidButtonState, eventTimeMs): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendMouseMotion<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    relative: jboolean,
    x: jfloat,
    y: jfloat,
    android_button_state: jint,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendMouseMotion", || {
        let buttons = keymap::mouse_button_mask_from_android_state(android_button_state);
        let event = android_input::mouse_motion_event(x, y, relative != 0, buttons, event_time_ms);
        match registry::with_runtime(|runtime| runtime.push_input(event)) {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `sendMouseButton(androidButton, pressed, x, y, eventTimeMs): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendMouseButton<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    android_button: jint,
    pressed: jboolean,
    x: jfloat,
    y: jfloat,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendMouseButton", || {
        let event = match android_input::mouse_button_event(
            android_button,
            pressed != 0,
            x,
            y,
            event_time_ms,
        ) {
            Some(event) => event,
            None => {
                bb_debug!("mouse button {android_button} is not modelled; ignored");
                return OK;
            }
        };
        match registry::with_runtime(|runtime| runtime.push_input(event)) {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `sendMouseWheel(dx, dy, eventTimeMs): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendMouseWheel<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    dx: jfloat,
    dy: jfloat,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendMouseWheel", || {
        let event = android_input::mouse_wheel_event(dx, dy, event_time_ms);
        match registry::with_runtime(|runtime| runtime.push_input(event)) {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `sendKey(androidKeyCode, pressed, repeat, unicodeChar, deviceKind, deviceId, eventTimeMs): Int`
///
/// `unicodeChar` is `KeyEvent.getUnicodeChar(metaState)` from Kotlin: the bridge
/// cannot compute it, because it depends on the active layout and on
/// `metaState`.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendKey<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    android_key_code: jint,
    pressed: jboolean,
    repeat: jboolean,
    unicode_char: jint,
    device_kind: jint,
    device_id: jint,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendKey", || {
        let accepted = registry::with_runtime(|runtime| {
            let key = KeyInput {
                android_keycode: android_key_code,
                pressed: pressed != 0,
                repeat: repeat != 0,
                unicode: if unicode_char > 0 { unicode_char as u32 } else { 0 },
                device: DeviceKind::from_jni(device_kind),
                device_id,
                // Filled in by `Runtime::key_input` from the tracked state.
                modifiers: 0,
                timestamp_ms: event_time_ms,
            };
            let event = runtime.key_input(key);
            runtime.push_input(event)
        });
        match accepted {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `sendText(text, eventTimeMs): Int` — IME commits.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendText<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    text: JString<'local>,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendText", || {
        let text = helpers::string_arg(&mut env, &text);
        if text.is_empty() {
            return OK;
        }
        let event = android_input::text_event(text, event_time_ms);
        match registry::with_runtime(|runtime| runtime.push_input(event)) {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `sendGamepadAxis(deviceId, androidAxis, value, eventTimeMs): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_sendGamepadAxis<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    device_id: jint,
    android_axis: jint,
    value: jfloat,
    event_time_ms: jlong,
) -> jint {
    helpers::guarded("sendGamepadAxis", || {
        let event =
            match android_input::gamepad_axis_event(device_id, android_axis, value, event_time_ms) {
                Some(event) => event,
                None => {
                    bb_debug!("gamepad axis {android_axis} (0x{android_axis:x}) is not mapped");
                    return OK;
                }
            };
        match registry::with_runtime(|runtime| runtime.push_input(event)) {
            Some(_) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

// -------------------------------------------------------------- configuration

/// `setRenderer(renderer): Int`
///
/// The renderer is chosen at `createRuntime`; this only *validates* a late
/// request, so a launcher cannot believe it switched APIs when it did not.
/// `Auto` and "the renderer already active" are accepted as no-ops.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_setRenderer<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    renderer: jint,
) -> jint {
    helpers::guarded("setRenderer", || {
        let requested = RendererKind::from_jni(renderer);
        helpers::report(
            "setRenderer",
            registry::try_with(|runtime| {
                let active = runtime.renderer_kind()?;
                if requested == RendererKind::Auto || requested == active {
                    bb_debug!("setRenderer({}) is already active", requested.name());
                    Ok(())
                } else {
                    describe_renderer_mismatch(requested, active)
                }
            }),
        )
    })
}

/// `setRenderMode(mode): Int` — diagnostic render mode.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_setRenderMode<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    mode: jint,
) -> jint {
    helpers::guarded("setRenderMode", || {
        let mode = DiagnosticMode::from_jni(mode);
        match registry::with_runtime(|runtime| runtime.set_diagnostic_mode(mode)) {
            Some(()) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `setRenderLoopMode(mode): Int` — internal vs inverted loop.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_setRenderLoopMode<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    mode: jint,
) -> jint {
    helpers::guarded("setRenderLoopMode", || {
        let mode = LoopMode::from_jni(mode);
        match registry::with_runtime(|runtime| runtime.set_loop_mode(mode)) {
            Some(()) => OK,
            None => Error::NotInitialized.code(),
        }
    })
}

/// `setSwapInterval(interval): Int`
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_setSwapInterval<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    interval: jint,
) -> jint {
    helpers::guarded("setSwapInterval", || {
        helpers::report(
            "setSwapInterval",
            registry::try_with(|runtime| runtime.set_swap_interval(interval)),
        )
    })
}

/// `setLogLevel(level): Int` — Android log priorities (2=verbose … 6=error).
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_setLogLevel<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    level: jint,
) -> jint {
    helpers::guarded("setLogLevel", || {
        let level = match level {
            2 => Level::Verbose,
            3 => Level::Debug,
            4 => Level::Info,
            5 => Level::Warn,
            6 => Level::Error,
            _ => Level::Info,
        };
        crate::log::set_level(level);
        bb_debug!("log level set to {}", level.label());
        OK
    })
}

// ---------------------------------------------------------------- diagnostics

/// `getRendererInfo(): String` — `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`,
/// or `""` when the context does not exist yet.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_getRendererInfo<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    helpers::guarded("getRendererInfo", || {
        let text = registry::with_runtime(|runtime| runtime.renderer_info()).unwrap_or_default();
        helpers::new_string(&env, text)
    })
}

/// `getStatus(): String` — one line: lifecycle, loop, backend, graphics, input.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_getStatus<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    helpers::guarded("getStatus", || {
        let text = match registry::with_runtime(|runtime| runtime.status_line()) {
            Some(line) => format!("{line} platform_backends=[{}]", platform::describe_all()),
            None => "runtime=absent".to_string(),
        };
        helpers::new_string(&env, text)
    })
}

/// `runSelfTest(): String` — multi-line audit report.
#[no_mangle]
pub extern "system" fn Java_com_boardbridge_bridge_NativeBridge_runSelfTest<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    helpers::guarded("runSelfTest", || {
        let report = match registry::with_runtime(|runtime| runtime.self_test()) {
            Some(report) => format!("{report}\n  platform_backends=[{}]", platform::describe_all()),
            None => format!(
                "BoardBridge self-test (abi={ABI_VERSION})\n  runtime=absent\nresult=PARTIAL"
            ),
        };
        helpers::new_string(&env, report)
    })
}

fn invalid_argument(what: &'static str, detail: String) -> jint {
    bb_error!("invalid argument {what}: {detail}");
    Error::InvalidArgument(what).code()
}

/// Rejects an unusable renderer request without pretending it was honored.
fn describe_renderer_mismatch(requested: RendererKind, active: RendererKind) -> Result<()> {
    bb_warn!(
        "setRenderer({}) ignored: the active renderer is {}; recreate the runtime to switch",
        requested.name(),
        active.name()
    );
    Err(Error::InvalidState {
        state: "RUNTIME_ACTIVE",
        detail: "the graphics backend is fixed for the runtime lifetime",
    })
}
