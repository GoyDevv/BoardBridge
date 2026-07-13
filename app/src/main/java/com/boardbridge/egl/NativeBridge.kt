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
 * JNI bridge to the native EGL/Surface renderer implemented in
 * `app/src/main/cpp` and packaged as `libboardbridge.so`.
 *
 * This is the Java/Kotlin half of BoardBridge's modernization of the
 * Surface-to-OpenGL binding that Boardwalk (by zhuowei) delegated to the
 * framework `GLSurfaceView`. Here the raw [Surface] is handed to native code
 * which owns the EGL context, render thread, and input queue.
 */
object NativeBridge {

    init {
        System.loadLibrary("boardbridge")
    }

    /** A [Surface] became available; its [android.view.SurfaceHolder] is alive. */
    external fun onSurfaceCreated(surface: Surface)

    /** The surface's size (and/or format) changed. */
    external fun onSurfaceChanged(width: Int, height: Int)

    /**
     * The surface is being destroyed. This call blocks until the native render
     * thread has unbound and released the underlying `ANativeWindow`, which
     * prevents a use-after-free once Android reclaims the buffer.
     */
    external fun onSurfaceDestroyed()

    /** Resume the native render loop (activity foregrounded). */
    external fun onResume()

    /** Pause the native render loop (activity backgrounded). */
    external fun onPause()

    /** Stop and join the native render thread and tear down EGL. */
    external fun onDestroy()

    /**
     * Forward one touch pointer sample.
     * @param action the [android.view.MotionEvent] masked action.
     */
    external fun onTouch(pointerId: Int, action: Int, x: Float, y: Float, eventTimeMs: Long)

    /**
     * Forward a key event.
     * @param action [android.view.KeyEvent.ACTION_DOWN] or `ACTION_UP`.
     */
    external fun onKey(keyCode: Int, action: Int, unicodeChar: Int, eventTimeMs: Long)

    /** GL_VENDOR / GL_RENDERER / GL_VERSION once the context exists, else "". */
    external fun getRendererInfo(): String
}
