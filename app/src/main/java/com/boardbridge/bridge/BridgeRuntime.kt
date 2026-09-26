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

import android.util.Log
import android.view.Surface

/**
 * The Kotlin half of the bridge's shell: one process-wide runtime, started once
 * and stopped once, with every refused native call logged with its cause.
 *
 * This is the object a launcher (or this demo app) talks to. It is deliberately
 * *thin* — it owns no state that Rust also owns, and it never guesses at the
 * surface state. What it does add is the debug value: [NativeBridge] returns a
 * negative code, and a bare `-3` in logcat is useless, while
 * `surfaceCreated → ERR_INVALID_STATE (state=STOPPING)` is a bug report.
 *
 * Lifecycle, from the shell's point of view:
 *
 * ```
 * onCreate   → start(config)             // no surface exists yet: NO_SURFACE
 * surfaceCreated  → surfaceCreated(s)    // queued; EGL binds on the bridge thread
 * surfaceChanged  → surfaceChanged(w,h,f)
 * surfaceDestroyed→ surfaceDestroyed()   // bounded fence (≤ 250 ms), then async release
 * onPause    → pause()      onResume → resume()
 * onDestroy  → stop()                    // joins the bridge thread
 * ```
 *
 * Threading: [start], [stop] and the surface calls come from the Android UI
 * thread; [attachGameThread], [detachGameThread] and [present] must come from the
 * single thread that will own the game's GL work (Minecraft's main thread), never
 * from the UI thread.
 */
object BridgeRuntime {

    private const val TAG = "BoardBridge"

    /** `true` between a successful [start] and the matching [stop]. */
    @Volatile
    var isStarted: Boolean = false
        private set

    /**
     * Starts the runtime if it is not already running.
     *
     * Idempotent on purpose: a configuration change or a re-entered `onCreate`
     * must not create a second runtime (Rust refuses it with
     * [NativeBridge.ERR_ALREADY_INITIALIZED], which would otherwise be an
     * easy-to-miss log line).
     *
     * @return `true` when a runtime is alive after the call.
     */
    fun start(config: Config = Config()): Boolean {
        if (isStarted) {
            Log.w(TAG, "BridgeRuntime.start() ignored: the runtime is already started")
            return true
        }
        val code = NativeBridge.createRuntime(
            config.renderer,
            config.loopMode,
            config.flags(),
            config.diagnosticMode,
            config.targetFps,
        )
        if (code != NativeBridge.OK) {
            Log.e(TAG, "createRuntime(${config.describe()}) → ${describe(code)}")
            return false
        }
        isStarted = true
        Log.i(TAG, "BridgeRuntime started (${config.describe()}, abi=${NativeBridge.ABI_VERSION})")
        return true
    }

    /**
     * Stops the runtime and joins the bridge thread.
     *
     * Safe to call when nothing is running, so it needs no guard in `onDestroy`.
     */
    fun stop() {
        if (!isStarted) {
            Log.d(TAG, "BridgeRuntime.stop() ignored: no runtime is started")
            return
        }
        val code = NativeBridge.destroyRuntime()
        report("destroyRuntime", code)
        isStarted = false
        Log.i(TAG, "BridgeRuntime stopped")
    }

    // -------------------------------------------------------------- surface

    /** Hands a live [Surface] to the bridge. False when the lifecycle refused it. */
    fun surfaceCreated(surface: Surface): Boolean =
        report("surfaceCreated", NativeBridge.surfaceCreated(surface))

    /** Reports the new surface size (best effort; the bridge re-reads it from EGL). */
    fun surfaceChanged(width: Int, height: Int, format: Int): Boolean =
        report("surfaceChanged", NativeBridge.surfaceChanged(width, height, format))

    /**
     * Signals that the surface is going away.
     *
     * Blocks the UI thread for at most the configured fence. A code other than
     * [NativeBridge.OK] here is not a crash condition: Rust logs what it did and
     * still guarantees the window is not touched afterwards.
     */
    fun surfaceDestroyed(): Boolean =
        report("surfaceDestroyed", NativeBridge.surfaceDestroyed())

    // ------------------------------------------------------------- activity

    /** Activity paused (the internal loop idles; the surface may still exist). */
    fun pause(): Boolean = report("onPause", NativeBridge.onPause())

    /** Activity resumed. */
    fun resume(): Boolean = report("onResume", NativeBridge.onResume())

    // ---------------------------------------------------- frame-loop inversion

    /**
     * Makes the EGL context current on the calling thread.
     *
     * Call from the thread that will render the game — for Minecraft, its main
     * thread — once the surface exists, and keep the pairing exactly one attach
     * per detach.
     */
    fun attachGameThread(): Boolean =
        report("attachGameThread", NativeBridge.attachGameThread())

    /** Releases the EGL context from the calling thread. */
    fun detachGameThread(): Boolean =
        report("detachGameThread", NativeBridge.detachGameThread())

    /** Presents the back buffer (the game's `glfwSwapBuffers`/`SDL_GL_SwapWindow`). */
    fun present(): Boolean = report("swapBuffers", NativeBridge.swapBuffers())

    // -------------------------------------------------------- configuration

    /** Diagnostic render mode; uniform with the input toggling in the demo. */
    fun setRenderMode(mode: Int): Boolean =
        report("setRenderMode", NativeBridge.setRenderMode(mode))

    /** Switches between the internal and inverted loops. */
    fun setLoopMode(mode: Int): Boolean =
        report("setRenderLoopMode", NativeBridge.setRenderLoopMode(mode))

    /** `eglSwapInterval`; `1` for vsync. */
    fun setSwapInterval(interval: Int): Boolean =
        report("setSwapInterval", NativeBridge.setSwapInterval(interval))

    /** Minimum priority for native logs (`2`=verbose … `6`=error). */
    fun setLogLevel(level: Int): Boolean =
        report("setLogLevel", NativeBridge.setLogLevel(level))

    // ---------------------------------------------------------- diagnostics

    /** `GL_VENDOR=… | GL_RENDERER=… | GL_VERSION=…`, or `""` before the context exists. */
    fun rendererInfo(): String = NativeBridge.getRendererInfo()

    /** One-line status, including the native ABI version. */
    fun status(): String = NativeBridge.getStatus()

    /** Multi-line audit report; safe to call at any time. */
    fun selfTest(): String = NativeBridge.runSelfTest()

    /** Logs the one-line status at info level, for bug reports. */
    fun logStatus() {
        Log.i(TAG, "status: ${status()}")
    }

    /**
     * Polls [rendererInfo] until the EGL context reports the GL strings.
     *
     * The context is created asynchronously on the bridge thread, so the demo
     * retries from `MainActivity` via `View.postDelayed`; a launcher would show
     * the same string in its settings screen instead.
     */
    fun rendererInfoIsReady(): Boolean = rendererInfo().isNotEmpty()

    /**
     * Records a refused call.
     *
     * [NativeBridge.ERR_NOT_INITIALIZED] is debug-level: it is the expected code
     * when a lifecycle callback arrives after [stop]. Everything else is an
     * error, because it means the shell and the state machine disagree.
     */
    private fun report(op: String, code: Int): Boolean {
        if (code == NativeBridge.OK) {
            return true
        }
        val message = "$op → ${describe(code)}"
        if (code == NativeBridge.ERR_NOT_INITIALIZED) {
            Log.d(TAG, message)
        } else {
            Log.e(TAG, message)
        }
        return false
    }

    /** `ERR_INVALID_STATE (state=SURFACE_ACTIVE)`-style description. */
    fun describe(code: Int): String {
        val name = NativeBridge.codeName(code)
        if (code == NativeBridge.OK) {
            return name
        }
        return "$name (state=${lifecycleState()})"
    }

    /** The state the native lifecycle machine reports, or `"?"` before it exists. */
    private fun lifecycleState(): String {
        // `getStatus()` embeds `Lifecycle::summary()`, which begins `state=<NAME>`.
        val status = status()
        val start = status.indexOf("state=")
        if (start < 0) return "?"
        val rest = status.substring(start + "state=".length)
        val end = rest.indexOf(' ')
        return if (end < 0) rest else rest.substring(0, end)
    }

    /**
     * What the shell can decide about the bridge.
     *
     * Defaults are the demo/CI configuration: OpenGL ES, the internal loop, the
     * solid diagnostic clear, vsync, and the EGL context preserved across
     * surface loss so a rotation does not throw away the game's GL objects.
     */
    data class Config(
        /** [NativeBridge.Renderer] value. */
        val renderer: Int = NativeBridge.Renderer.AUTO,
        /** [NativeBridge.LoopMode] value. */
        val loopMode: Int = NativeBridge.LoopMode.INTERNAL,
        /** [NativeBridge.RenderMode] value. */
        val diagnosticMode: Int = NativeBridge.RenderMode.SOLID,
        /** Internal-loop frame cap; `0` = paced by vsync. */
        val targetFps: Int = 0,
        /** Request vsync (`eglSwapInterval(1)`). */
        val vsync: Boolean = true,
        /** Keep the EGL context alive across surface loss. */
        val preserveContext: Boolean = true,
        /** Emit the per-second diagnostics line. */
        val diagnosticLogs: Boolean = true,
    ) {
        /** Packed `createRuntime` flags. Mirrors `RuntimeConfig::flags()`. */
        fun flags(): Int {
            var bits = 0
            if (vsync) bits = bits or NativeBridge.Flags.VSYNC
            if (preserveContext) bits = bits or NativeBridge.Flags.PRESERVE_CONTEXT
            if (diagnosticLogs) bits = bits or NativeBridge.Flags.DIAGNOSTIC_LOGS
            return bits
        }

        /** Compact description for logs. */
        fun describe(): String =
            "renderer=$renderer loop=$loopMode diag=$diagnosticMode fps_cap=$targetFps " +
                "vsync=$vsync preserve=$preserveContext logs=$diagnosticLogs"
    }
}
