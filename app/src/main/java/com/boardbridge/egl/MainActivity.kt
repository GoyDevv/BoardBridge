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
package com.boardbridge.egl

import android.os.Bundle
import android.util.Log
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * The single Empty Activity that hosts [BridgeSurfaceView]. Rendering, surface
 * lifecycle, and input all run through [NativeBridge] into native C++.
 */
class MainActivity : ComponentActivity() {

    private lateinit var surfaceView: BridgeSurfaceView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(TAG, "MainActivity onCreate")
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

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

        // Exercise the native getRendererInfo() JNI entry point: the EGL context
        // is created asynchronously on the render thread, so poll briefly until
        // it reports GL_VENDOR / GL_RENDERER / GL_VERSION, then log it.
        logRendererInfoWhenReady(attempt = 0)
    }

    private fun logRendererInfoWhenReady(attempt: Int) {
        val info = NativeBridge.getRendererInfo()
        when {
            info.isNotEmpty() -> Log.i(TAG, "Renderer info: $info")
            attempt < MAX_RENDERER_INFO_ATTEMPTS ->
                surfaceView.postDelayed({ logRendererInfoWhenReady(attempt + 1) }, 300L)
            else -> Log.w(TAG, "Renderer info still empty after $attempt attempts")
        }
    }

    override fun onResume() {
        super.onResume()
        NativeBridge.onResume()
    }

    override fun onPause() {
        NativeBridge.onPause()
        super.onPause()
    }

    override fun onDestroy() {
        NativeBridge.onDestroy()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "BoardBridge"
        const val MAX_RENDERER_INFO_ATTEMPTS = 20
    }
}
