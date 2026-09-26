/*
 * Copyright 2026 The BoardBridge Authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.boardbridge.bridge

import android.view.Surface

/**
 * The JNI surface of `libboardbridge.so` (the Rust crate in `boardbridge/`).
 *
 * This object is a *thin, exact* mirror of the native entry points — one Kotlin
 * `external fun` per `Java_com_boardbridge_bridge_NativeBridge_*` export in
 * `boardbridge/src/jni/native_bridge.rs`. Nothing here decides policy: the
 * lifecycle machine, the state and the fences all live in Rust, and every call
 * returns a status code ([OK] or one of the `ERR_*` values).
 *
 * Prefer [BridgeRuntime] in application code — it logs refused calls with their
 * cause and keeps the "one runtime per process" rule. Use this object directly
 * only inside the bridge's own plumbing (the surface view and the launcher's
 * game-thread shim).
 *
 * Contract version: [ABI_VERSION]. A Kotlin/Rust mismatch is loud, not silent —
 * `getStatus()` reports the native ABI version and [ERR_INVALID_STATE] follows
 * any call whose arguments the other side does not understand.
 */
object NativeBridge {

    /**
     * Version of the Kotlin ↔ Rust contract. Must equal `ABI_VERSION` in
     * `boardbridge/src/runtime/mod.rs`; `getStatus()` (and `runSelfTest()`)
     * prints the native value so a stale `.so` in an installed APK is visible
     * in logcat.
     */
    const val ABI_VERSION = 2

    init {
        // libboardbridge.so is built from boardbridge/ by cargo-ndk (see
        // app/build.gradle.kts) and packaged into jniLibs.
        System.loadLibrary("boardbridge")
    }

    // ------------------------------------------------------------ lifetime

    /**
     * Creates the process-wide runtime.
     *
     * @param renderer [Renderer.AUTO], [Renderer.GLES] or [Renderer.VULKAN].
     * @param loopMode [LoopMode.INTERNAL] or [LoopMode.INVERTED].
     * @param flags bitmask of [Flags].
     * @param diagnosticMode [RenderMode.NONE], [RenderMode.SOLID] or
     *   [RenderMode.TRIANGLE].
     * @param targetFps frame cap for the internal loop; `0` lets vsync pace it.
     * @return [OK], or [ERR_ALREADY_INITIALIZED] when one is already alive.
     *
     * Backend failures are asynchronous: they are reported through
     * [getStatus] / `runSelfTest()` and logcat, never through this return value,
     * because the EGL work happens on the bridge thread.
     */
    external fun createRuntime(
        renderer: Int,
        loopMode: Int,
        flags: Int,
        diagnosticMode: Int,
        targetFps: Int,
    ): Int

    /** Stops the bridge thread and releases the surface, EGL and the window. */
    external fun destroyRuntime(): Int

    // ------------------------------------------------------------- surface

    /**
     * A `SurfaceHolder` became valid: acquires the `ANativeWindow` and queues
     * the binding. The surface's size is read from the window itself.
     *
     * Returns as soon as the binding is *queued*; EGL work happens on the
     * bridge thread.
     */
    external fun surfaceCreated(surface: Surface): Int

    /** The surface was resized; the bridge re-reads the size from EGL. */
    external fun surfaceChanged(width: Int, height: Int, format: Int): Int

    /**
     * The surface is going away. Blocks the calling (UI) thread for at most the
     * configured fence (250 ms by default) while the bridge retires the window;
     * the release still completes afterwards if the fence expires.
     */
    external fun surfaceDestroyed(): Int

    // ------------------------------------------------------------ activity

    /** Activity paused; the internal loop stops drawing. */
    external fun onPause(): Int

    /** Activity resumed. */
    external fun onResume(): Int

    // --------------------------------------------------- frame-loop inversion

    /**
     * Makes the EGL context current on the calling (game) thread so Minecraft
     * can drive the loop through the LWJGL backend.
     */
    external fun attachGameThread(): Int

    /** Releases the EGL context from the calling thread. */
    external fun detachGameThread(): Int

    /** Presents the back buffer from the calling (game) thread. */
    external fun swapBuffers(): Int

    // --------------------------------------------------------------- input

    /**
     * Forwards one `MotionEvent` batch.
     *
     * All five arrays must be at least [count] long; index `i` describes one
     * pointer. [phases] carries the already-normalized [TouchPhase] values,
     * because Android encodes the target pointer inside the action for
     * `ACTION_POINTER_DOWN`/`ACTION_POINTER_UP`.
     */
    external fun sendTouchBatch(
        pointerIds: IntArray,
        phases: IntArray,
        xs: FloatArray,
        ys: FloatArray,
        pressures: FloatArray,
        count: Int,
        eventTimeMs: Long,
    ): Int

    /**
     * Pointer or raw mouse motion.
     *
     * @param relative `true` when [x]/[y] are deltas (Android pointer capture,
     *   which is the path Minecraft uses for camera steering), `false` for
     *   absolute positions.
     * @param androidButtonState `MotionEvent.getButtonState()`.
     */
    external fun sendMouseMotion(
        relative: Boolean,
        x: Float,
        y: Float,
        androidButtonState: Int,
        eventTimeMs: Long,
    ): Int

    /** Mouse button press/release. @param androidButton `MotionEvent` `BUTTON_*`. */
    external fun sendMouseButton(
        androidButton: Int,
        pressed: Boolean,
        x: Float,
        y: Float,
        eventTimeMs: Long,
    ): Int

    /** Wheel motion; SDL convention (positive dx = right, positive dy = away). */
    external fun sendMouseWheel(dx: Float, dy: Float, eventTimeMs: Long): Int

    /**
     * Key press/release/repeat, translated to SDL scancodes by the bridge.
     *
     * @param unicodeChar `KeyEvent.getUnicodeChar(metaState)`: the bridge cannot
     *   compute it, because it depends on the active layout and `metaState`.
     * @param deviceKind [DeviceKind]; decides whether a `BUTTON_*` keycode
     *   becomes an SDL gamepad button or a plain key.
     */
    external fun sendKey(
        androidKeyCode: Int,
        pressed: Boolean,
        repeat: Boolean,
        unicodeChar: Int,
        deviceKind: Int,
        deviceId: Int,
        eventTimeMs: Long,
    ): Int

    /** Text committed by the IME. */
    external fun sendText(text: String, eventTimeMs: Long): Int

    /**
     * Gamepad axis motion; the bridge translates the axis and normalizes the
     * value (sticks −1..1, triggers 0..1).
     */
    external fun sendGamepadAxis(
        deviceId: Int,
        androidAxis: Int,
        value: Float,
        eventTimeMs: Long,
    ): Int

    // -------------------------------------------------------- configuration

    /**
     * Validates a late renderer request.
     *
     * The backend is fixed for the runtime's lifetime, so this returns
     * [ERR_INVALID_STATE] when the request disagrees with the active renderer
     * instead of pretending the switch happened. Recreate the runtime to switch.
     */
    external fun setRenderer(renderer: Int): Int

    /** Changes the diagnostic render mode ([RenderMode]). */
    external fun setRenderMode(mode: Int): Int

    /** Switches between the internal and inverted loops ([LoopMode]). */
    external fun setRenderLoopMode(mode: Int): Int

    /** `eglSwapInterval`: `1` for vsync, `0` for immediate. */
    external fun setSwapInterval(interval: Int): Int

    /** Sets the minimum Android log priority ([LogLevel]). */
    external fun setLogLevel(level: Int): Int

    // --------------------------------------------------------- diagnostics

    /**
     * `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`, or `""` while the context
     * does not exist yet (the caller retries).
     */
    external fun getRendererInfo(): String

    /** One-line status: lifecycle, loop, graphics counters, input queue, EGL. */
    external fun getStatus(): String

    /** Multi-line self-test report (never throws; reports PARTIAL when absent). */
    external fun runSelfTest(): String

    // ------------------------------------------------------------ constants
    //
    // Mirror boardbridge/src/{error,runtime/config,graphics,render}.rs and
    // boardbridge/src/input/event.rs. Keep in sync; the Rust side is canonical.

    /** Success. */
    const val OK = 0
    /** No runtime exists (or it was already destroyed). */
    const val ERR_NOT_INITIALIZED = -1
    /** `createRuntime` while a runtime was alive. */
    const val ERR_ALREADY_INITIALIZED = -2
    /** The lifecycle refused the call in its current state. */
    const val ERR_INVALID_STATE = -3
    /** No `ANativeWindow` is bound. */
    const val ERR_NO_SURFACE = -4
    /** A window bind/unbind is already in flight. */
    const val ERR_SURFACE_BUSY = -5
    /** The surface was revoked; the game thread must stop presenting. */
    const val ERR_SURFACE_REVOKED = -6
    /** EGL or GL reported a failure. */
    const val ERR_GRAPHICS = -7
    /** The requested backend is interface-only (`Vulkan`). */
    const val ERR_BACKEND_UNAVAILABLE = -8
    /** A JNI argument was rejected. */
    const val ERR_INVALID_ARGUMENT = -9
    /** Any other failure; see logcat for the message. */
    const val ERR_MESSAGE = -10

    /** Symbolic name for a status code, for logs. */
    fun codeName(code: Int): String = when (code) {
        OK -> "OK"
        ERR_NOT_INITIALIZED -> "ERR_NOT_INITIALIZED"
        ERR_ALREADY_INITIALIZED -> "ERR_ALREADY_INITIALIZED"
        ERR_INVALID_STATE -> "ERR_INVALID_STATE"
        ERR_NO_SURFACE -> "ERR_NO_SURFACE"
        ERR_SURFACE_BUSY -> "ERR_SURFACE_BUSY"
        ERR_SURFACE_REVOKED -> "ERR_SURFACE_REVOKED"
        ERR_GRAPHICS -> "ERR_GRAPHICS"
        ERR_BACKEND_UNAVAILABLE -> "ERR_BACKEND_UNAVAILABLE"
        ERR_INVALID_ARGUMENT -> "ERR_INVALID_ARGUMENT"
        ERR_MESSAGE -> "ERR_MESSAGE"
        else -> "ERR_UNKNOWN($code)"
    }

    /** Graphics backend. Mirrors `graphics::RendererKind`. */
    object Renderer {
        /** Best available backend (currently OpenGL ES). */
        const val AUTO = 0
        /** OpenGL ES 3.x via EGL. */
        const val GLES = 1
        /**
         * Vulkan — **interface only**: the backend reports
         * [ERR_BACKEND_UNAVAILABLE] from `initialize()` on the bridge thread, so
         * the failure is visible in `getStatus()` and logcat rather than in
         * `createRuntime`'s return value. See docs/GRAPHICS.md.
         */
        const val VULKAN = 2
    }

    /** Who drives the frame loop. Mirrors `runtime::config::LoopMode`. */
    object LoopMode {
        /** The bridge thread draws (diagnostics, CI, "prove the device works"). */
        const val INTERNAL = 0
        /** The game owns the loop (`attachGameThread` / `swapBuffers`). */
        const val INVERTED = 1
    }

    /** Diagnostic render mode. Mirrors `render::DiagnosticMode`. */
    object RenderMode {
        /** Draw nothing but a black clear. */
        const val NONE = 0
        /** Clear to the fixed test colour (the CI mode). */
        const val SOLID = 1
        /** Shaded spinning triangle. */
        const val TRIANGLE = 2
    }

    /** `createRuntime` flags. Mirrors `runtime::config::FLAG_*`. */
    object Flags {
        /** Request vsync. */
        const val VSYNC = 1 shl 0
        /** Keep the EGL context (and the game's GL objects) across surface loss. */
        const val PRESERVE_CONTEXT = 1 shl 1
        /** Emit the per-second diagnostics line. */
        const val DIAGNOSTIC_LOGS = 1 shl 2
    }

    /** Where a key event came from. Mirrors `input::event::DeviceKind`. */
    object DeviceKind {
        const val UNKNOWN = 0
        const val KEYBOARD = 1
        const val GAMEPAD = 2
        const val MOUSE = 3
        const val TOUCHSCREEN = 4
    }

    /** Normalized touch phase. Mirrors `input::event::TouchPhase`. */
    object TouchPhase {
        const val DOWN = 0
        const val MOVE = 1
        const val UP = 2
        const val CANCEL = 3
    }

    /** Android log priorities accepted by [setLogLevel]. */
    object LogLevel {
        const val VERBOSE = 2
        const val DEBUG = 3
        const val INFO = 4
        const val WARN = 5
        const val ERROR = 6
    }
}
