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

import android.view.Surface

/**
 * JNI bridge to the native passive provider (libboardbridge) and the GLFW shim
 * (libglfw). BoardBridge no longer renders on its own: these calls feed the
 * native bridge core (window/size/input/lifecycle), and a native game thread
 * (started by [startClient]) drives the GLFW shim to draw — the same way an
 * LWJGL/Minecraft launcher would.
 */
object NativeBridge {

    init {
        // libboardbridge depends on libglfw; load the dependency first.
        System.loadLibrary("glfw")
        System.loadLibrary("boardbridge")
    }

    /** Spawn the native game thread (drives the GLFW shim). Idempotent. */
    external fun startClient()

    /** Request the game thread to stop and join it. */
    external fun stopClient()

    /** A Surface became available; hand it to the bridge core. */
    external fun onSurfaceCreated(surface: Surface)

    /** The surface size/format changed. */
    external fun onSurfaceChanged(width: Int, height: Int)

    /** The surface is being destroyed; blocks until the game thread releases it. */
    external fun onSurfaceDestroyed()

    /** Resume rendering (activity foregrounded). */
    external fun onResume()

    /** Pause rendering (activity backgrounded). */
    external fun onPause()

    /** Forward one touch pointer sample (action = MotionEvent masked action). */
    external fun onTouch(pointerId: Int, action: Int, x: Float, y: Float, eventTimeMs: Long)

    /** Forward a key event (action = KeyEvent.ACTION_DOWN/UP). */
    external fun onKey(keyCode: Int, action: Int, unicodeChar: Int, eventTimeMs: Long)

    /** GL_VENDOR / GL_RENDERER / GL_VERSION once the game thread has a context. */
    external fun getRendererInfo(): String
}
