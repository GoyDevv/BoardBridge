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

import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.util.Log
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * The demo/verification activity: one fullscreen [BridgeSurfaceView], one
 * [BridgeRuntime], and no rendering code of its own.
 *
 * It exists to prove the whole chain on a real device —
 * `Surface → JNI → lifecycle machine → EGL → GLES → present` — and to give CI
 * something to launch. The diagnostic renderer inside Rust draws the cyan
 * clear (or the triangle) and logs the frame statistics; this activity only
 * starts the runtime, reports Android lifecycle, and prints the GL strings once
 * the context appears.
 *
 * A launcher replaces this class: it starts the same runtime, then hands the
 * surface to Minecraft instead of letting the internal loop draw (see
 * `docs/MIGRATION.md`).
 */
class MainActivity : ComponentActivity() {

    private lateinit var surfaceView: BridgeSurfaceView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(TAG, "MainActivity onCreate (bridge abi=${NativeBridge.ABI_VERSION})")
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        // Verbose native logs only while the APK is debuggable; a release build
        // gets the info level the bridge defaults to.
        val debuggable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        BridgeRuntime.setLogLevel(
            if (debuggable) NativeBridge.LogLevel.DEBUG else NativeBridge.LogLevel.INFO,
        )

        // Immersive, edge-to-edge fullscreen for the render surface.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowCompat.getInsetsController(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }

        surfaceView = BridgeSurfaceView(this)
        setContentView(surfaceView)
        surfaceView.requestFocus()

        // No surface exists yet: the machine starts in NO_SURFACE and the
        // surfaceCreated callback below moves it on.
        BridgeRuntime.start(
            BridgeRuntime.Config(
                renderer = NativeBridge.Renderer.AUTO,
                loopMode = NativeBridge.LoopMode.INTERNAL,
                diagnosticMode = NativeBridge.RenderMode.SOLID,
                targetFps = 0,
                vsync = true,
                preserveContext = true,
                diagnosticLogs = true,
            )
        )

        logRendererInfoWhenReady(attempt = 0)
    }

    /**
     * The EGL context is created asynchronously on the bridge thread, so the GL
     * strings are polled briefly (the same behaviour as the previous C++ build,
     * which is what the CI log greps expect to see).
     */
    private fun logRendererInfoWhenReady(attempt: Int) {
        when {
            BridgeRuntime.rendererInfoIsReady() -> {
                Log.i(TAG, "renderer: ${BridgeRuntime.rendererInfo()}")
                BridgeRuntime.logStatus()
            }
            attempt < MAX_RENDERER_INFO_ATTEMPTS ->
                surfaceView.postDelayed({ logRendererInfoWhenReady(attempt + 1) }, POLL_INTERVAL_MS)
            else -> Log.w(
                TAG,
                "no renderer info after $attempt attempts; ${BridgeRuntime.status()}",
            )
        }
    }

    override fun onResume() {
        super.onResume()
        BridgeRuntime.resume()
    }

    override fun onPause() {
        BridgeRuntime.pause()
        super.onPause()
    }

    override fun onDestroy() {
        // Joins the bridge thread; the surface has already been released by the
        // holder callback, so nothing touches the window after this point.
        BridgeRuntime.stop()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "BoardBridge"

        /**
         * 60 × 150 ms = 9 s. EGL is created on the first surface bind rather
         * than at [BridgeRuntime.start], so on a slow device the GL strings can
         * legitimately appear seconds after `onCreate`; the old 3 s window gave
         * up too early and logged a misleading warning.
         */
        const val MAX_RENDERER_INFO_ATTEMPTS = 60
        const val POLL_INTERVAL_MS = 150L
    }
}
