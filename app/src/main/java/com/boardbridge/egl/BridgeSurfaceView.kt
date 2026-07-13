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

import android.content.Context
import android.util.AttributeSet
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView

/**
 * A [SurfaceView] that forwards its [Surface][android.view.Surface] lifecycle
 * and input events to the native EGL bridge.
 *
 * This is the modern counterpart to Boardwalk's `BoardwalkGLSurfaceView`
 * (a thin subclass of the framework `GLSurfaceView`). Instead of letting the
 * framework manage EGL on the Java thread, the raw `Surface` is handed to
 * native code via [NativeBridge], which owns the EGL context and a dedicated
 * render thread.
 */
class BridgeSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0,
) : SurfaceView(context, attrs, defStyleAttr), SurfaceHolder.Callback {

    init {
        holder.addCallback(this)
        // Needed to receive hardware key events.
        isFocusable = true
        isFocusableInTouchMode = true
    }

    // ---- Surface lifecycle (SurfaceHolder.Callback) ----

    override fun surfaceCreated(holder: SurfaceHolder) {
        NativeBridge.onSurfaceCreated(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        NativeBridge.onSurfaceChanged(width, height)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        // Blocks until native has released the ANativeWindow (no use-after-free).
        NativeBridge.onSurfaceDestroyed()
    }

    // ---- Input dispatch ----

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (val action = event.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                // A MOVE batch may carry several pointers; report each.
                for (i in 0 until event.pointerCount) {
                    NativeBridge.onTouch(
                        event.getPointerId(i), action,
                        event.getX(i), event.getY(i), event.eventTime,
                    )
                }
            }
            else -> {
                val i = event.actionIndex
                NativeBridge.onTouch(
                    event.getPointerId(i), action,
                    event.getX(i), event.getY(i), event.eventTime,
                )
            }
        }
        return true
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // Let the system handle BACK so the user is never trapped.
        if (keyCode == KeyEvent.KEYCODE_BACK) return super.onKeyDown(keyCode, event)
        NativeBridge.onKey(keyCode, KeyEvent.ACTION_DOWN, event.unicodeChar, event.eventTime)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) return super.onKeyUp(keyCode, event)
        NativeBridge.onKey(keyCode, KeyEvent.ACTION_UP, event.unicodeChar, event.eventTime)
        return true
    }
}
