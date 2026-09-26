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

import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.text.InputType
import android.util.AttributeSet
import android.util.Log
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection

/**
 * A [SurfaceView] that forwards its `Surface` lifecycle and its input to the
 * Rust bridge.
 *
 * This replaces Boardwalk's `GLSurfaceView` subclass. The framework's
 * `GLSurfaceView` owns EGL and a render thread on the Java side; here the raw
 * [android.view.Surface] is handed to native code ([BridgeRuntime.surfaceCreated])
 * and Rust owns the `ANativeWindow`, EGL and the loop. The view's only jobs are:
 *
 * 1. report `surfaceCreated` / `surfaceChanged` / `surfaceDestroyed`,
 * 2. translate Android input into the bridge's batch calls, and
 * 3. never block: every call below is *checked* but not awaited, except
 *    `surfaceDestroyed`, which is bounded by the native fence (250 ms default).
 *
 * ## Input, honestly
 *
 * Android does not deliver SDL input. What it delivers is a `MotionEvent` /
 * `KeyEvent` with Android keycodes, Android axes and Android button masks. This
 * class therefore does the one part Android *can* do faithfully — normalize the
 * target pointer out of `ACTION_POINTER_*`, tag which device class the event came
 * from, and hand over `KeyEvent.getUnicodeChar(metaState)` — and no more. The
 * Android→SDL scancode/keycode/axis translation itself happens in Rust
 * (`input/keymap.rs`, generated from SDL3's own tables), so the game sees real
 * `SDL_Scancode` / `SDL_GamepadAxis` values, not renamed Android keycodes.
 *
 * The paths implemented here:
 *
 * | Android source | Forwarded as |
 * |---|---|
 * | touchscreen / stylus | `sendTouchBatch` (all pointers of `ACTION_MOVE`; one for `POINTER_*`) |
 * | mouse (hover/drag) | `sendMouseMotion(relative = false, buttons = getButtonState())` |
 * | mouse (captured) | `sendMouseMotion(relative = true, …)` — the camera path |
 * | mouse buttons / wheel | `sendMouseButton` / `sendMouseWheel` |
 * | key / remote | `sendKey(deviceKind, unicodeChar, repeat)` |
 * | gamepad buttons | `sendKey` (Rust turns `BUTTON_*` on a gamepad into an SDL gamepad button) |
 * | gamepad sticks/triggers | `sendGamepadAxis` |
 * | IME | `sendText` (committed text) |
 *
 * Not modelled: pen hover, stylus eraser, and multi-button chords beyond what
 * `getButtonState()` reports. `getStatus()` and `docs/ANDROID.md` say so rather
 * than pretending.
 */
class BridgeSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0,
) : SurfaceView(context, attrs, defStyleAttr), SurfaceHolder.Callback {

    init {
        holder.addCallback(this)
        // Needed to receive hardware key and generic motion (mouse/gamepad) events.
        isFocusable = true
        isFocusableInTouchMode = true
    }

    // ------------------------------------------------------- surface lifecycle

    override fun surfaceCreated(holder: SurfaceHolder) {
        BridgeRuntime.surfaceCreated(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        BridgeRuntime.surfaceChanged(width, height, format)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        // Blocks for at most the native fence; the release completes on the
        // bridge thread even if the fence expires, so Android may reclaim the
        // buffer afterwards without racing EGL.
        BridgeRuntime.surfaceDestroyed()
    }

    // ------------------------------------------------------------------- input

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.isFromSource(InputDevice.SOURCE_MOUSE)) {
            return handleMouseEvent(event)
        }
        return handleTouchEvent(event)
    }

    /**
     * Forwards a single `MotionEvent` as one batch.
     *
     * `ACTION_MOVE` carries every pointer (that is how Android reports them);
     * a `POINTER_DOWN`/`POINTER_UP` carries only the pointer the action names,
     * because the others did not change state — the same split SDL3's Android
     * backend makes. `ACTION_CANCEL` cancels every pointer still down.
     */
    private fun handleTouchEvent(event: MotionEvent): Boolean {
        val action = event.actionMasked
        when (action) {
            MotionEvent.ACTION_DOWN -> event.requestUnbufferedDispatch(event)
            MotionEvent.ACTION_MOVE,
            MotionEvent.ACTION_POINTER_DOWN,
            MotionEvent.ACTION_POINTER_UP,
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> Unit
            // ACTION_HOVER_* (a stylus that is not touching) is not modelled.
            else -> return true
        }

        val count = if (action == MotionEvent.ACTION_MOVE || action == MotionEvent.ACTION_CANCEL) {
            event.pointerCount
        } else {
            1
        }
        if (count <= 0 || count > MAX_TOUCH_POINTERS) {
            // Android allows far fewer pointers in practice; clamp rather than
            // let a malformed event reach the native bound.
            return true
        }

        val ids = touchIds
        val phases = touchPhases
        val xs = touchXs
        val ys = touchYs
        val pressures = touchPressures
        for (index in 0 until count) {
            val pointerIndex = if (action == MotionEvent.ACTION_MOVE || action == MotionEvent.ACTION_CANCEL) {
                index
            } else {
                event.actionIndex
            }
            ids[index] = event.getPointerId(pointerIndex)
            phases[index] = phaseOf(action)
            xs[index] = event.getX(pointerIndex)
            ys[index] = event.getY(pointerIndex)
            pressures[index] = event.getPressure(pointerIndex)
        }
        NativeBridge.sendTouchBatch(ids, phases, xs, ys, pressures, count, event.eventTime)
        return true
    }

    /** Normalized phase for the pointer an action names. */
    private fun phaseOf(action: Int): Int = when (action) {
        MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> NativeBridge.TouchPhase.DOWN
        MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> NativeBridge.TouchPhase.UP
        MotionEvent.ACTION_CANCEL -> NativeBridge.TouchPhase.CANCEL
        else -> NativeBridge.TouchPhase.MOVE
    }

    // ------------------------------------------------------------------- mouse

    /**
     * Handles mouse events that arrive as touch (hover, drag, buttons, wheel).
     *
     * A mouse is not a finger: the same `MotionEvent` stream is re-interpreted
     * here — motion keeps the button mask so the game sees the pointer move while
     * held, and the wheel arrives as `ACTION_SCROLL` with `AXIS_*SCROLL`.
     */
    private fun handleMouseEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_SCROLL -> {
                val dx = event.getAxisValue(MotionEvent.AXIS_HSCROLL)
                val dy = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
                NativeBridge.sendMouseWheel(dx, dy, event.eventTime)
                return true
            }
            MotionEvent.ACTION_DOWN,
            MotionEvent.ACTION_POINTER_DOWN,
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_POINTER_UP,
            -> {
                val pressed = event.actionMasked == MotionEvent.ACTION_DOWN ||
                    event.actionMasked == MotionEvent.ACTION_POINTER_DOWN
                // The position/button mask first, so the game sees where the
                // pointer was before the button event.
                NativeBridge.sendMouseMotion(
                    false,
                    event.x,
                    event.y,
                    event.buttonState,
                    event.eventTime,
                )
                val button = mouseButtonOf(event, pressed)
                if (button != 0) {
                    NativeBridge.sendMouseButton(button, pressed, event.x, event.y, event.eventTime)
                }
                lastMouseButtonState = event.buttonState
                return true
            }
            MotionEvent.ACTION_HOVER_MOVE, MotionEvent.ACTION_MOVE -> {
                NativeBridge.sendMouseMotion(
                    false,
                    event.x,
                    event.y,
                    event.buttonState,
                    event.eventTime,
                )
                return true
            }
            else -> return super.onTouchEvent(event)
        }
    }

    /**
     * Raw/relative pointer motion (Android pointer capture).
     *
     * With capture active Android reports movement as a delta in `x`/`y`, which is
     * exactly what Minecraft's camera wants; the bridge tags the event
     * `relative = true` so the game can tell it apart from an absolute position.
     * Enable it with [setPointerCapture] (a launcher's "grab mouse" toggle).
     */
    override fun onCapturedPointerEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_UP -> {
                val pressed = event.actionMasked == MotionEvent.ACTION_DOWN
                val button = mouseButtonOf(event, pressed)
                if (button != 0) {
                    NativeBridge.sendMouseButton(button, pressed, event.x, event.y, event.eventTime)
                }
                lastMouseButtonState = event.buttonState
            }
            MotionEvent.ACTION_MOVE, MotionEvent.ACTION_HOVER_MOVE -> Unit
            else -> return super.onCapturedPointerEvent(event)
        }
        NativeBridge.sendMouseMotion(true, event.x, event.y, event.buttonState, event.eventTime)
        return true
    }

    /**
     * Which mouse button an `ACTION_DOWN`/`ACTION_UP` names.
     *
     * `MotionEvent.getActionButton()` answers this exactly, but it is only
     * public from API 29; on older releases the answer is derived from the state
     * change instead (`buttonState` before the event versus after), which works
     * on every supported API level. Returning `0` means "no button change" and
     * the caller forwards the motion without a button event.
     */
    private fun mouseButtonOf(event: MotionEvent, pressed: Boolean): Int {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val button = event.actionButton
            if (button != 0) {
                return button
            }
        }
        val previous = lastMouseButtonState
        val current = event.buttonState
        val changed = if (pressed) {
            current and previous.inv()
        } else {
            previous and current.inv()
        }
        return if (changed != 0) changed else current
    }

    /**
     * Enables or disables Android pointer capture (relative mouse).
     *
     * Returns `false` when the platform declined the request (no mouse attached,
     * or another window holds capture). Never throws, so a launcher toggle can
     * call it directly.
     */
    fun setPointerCapture(enabled: Boolean): Boolean = try {
        if (enabled) requestPointerCapture() else releasePointerCapture()
        true
    } catch (error: IllegalStateException) {
        // Documented for requestPointerCapture when the view is not attached or
        // another view already captured the pointer.
        Log.w(TAG, "pointer capture ($enabled) refused: ${error.message}")
        false
    }

    // --------------------------------------------------------------------- other

    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (event.isFromSource(InputDevice.SOURCE_MOUSE)) {
            return handleMouseEvent(event)
        }
        if (event.isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            event.isFromSource(InputDevice.SOURCE_GAMEPAD)
        ) {
            return handleGamepadMotion(event)
        }
        return super.onGenericMotionEvent(event)
    }

    /**
     * Forwards a gamepad/joystick stick or trigger sample.
     *
     * The axes list below is exactly the set `gamepad_axis_from_android_axis`
     * models in Rust (`boardbridge/src/input/sdl_tables.rs`): X/Y/Z/RZ, the two
     * triggers, and the d-pad hat. Sending an unmodelled axis would only make the
     * native side log and drop it, so it is not sent.
     */
    private fun handleGamepadMotion(event: MotionEvent): Boolean {
        if (event.actionMasked != MotionEvent.ACTION_MOVE) {
            return super.onGenericMotionEvent(event)
        }
        val deviceId = event.deviceId
        for (axis in GAMEPAD_AXES) {
            NativeBridge.sendGamepadAxis(
                deviceId,
                axis,
                event.getAxisValue(axis),
                event.eventTime,
            )
        }
        return true
    }

    // ---------------------------------------------------------------------- keys

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // Let the system handle BACK: the user must never be trapped in the view.
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            return super.onKeyDown(keyCode, event)
        }
        return sendKey(event, pressed = true)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            return super.onKeyUp(keyCode, event)
        }
        return sendKey(event, pressed = false)
    }

    @Suppress("DEPRECATION")
    private fun sendKey(event: KeyEvent, pressed: Boolean): Boolean {
        // `KeyEvent.getUnicodeChar(metaState)` is deprecated in favour of the
        // `InputMethodManager` rework in API 29, but it is still the only way to
        // ask "what character would this produce under the active layout?" for a
        // hardware key. The bridge cannot compute it.
        val unicode = event.getUnicodeChar(event.metaState)
        NativeBridge.sendKey(
            event.keyCode,
            pressed,
            pressed && event.repeatCount > 0,
            unicode,
            deviceKindOf(event.device),
            event.deviceId,
            event.eventTime,
        )
        return true
    }

    /**
     * Classifies the device a key came from.
     *
     * Rust turns `AKEYCODE_BUTTON_*` into an SDL *gamepad button* only when the
     * tag says `GAMEPAD`, and leaves d-pad presses as keys (which is what SDL3's
     * Android backend does too). Gamepad is checked first because a controller
     * also reports `SOURCE_KEYBOARD`.
     */
    private fun deviceKindOf(device: InputDevice?): Int {
        if (device == null) {
            return NativeBridge.DeviceKind.UNKNOWN
        }
        val sources = device.sources
        return when {
            (sources and (InputDevice.SOURCE_GAMEPAD or InputDevice.SOURCE_JOYSTICK)) != 0 ->
                NativeBridge.DeviceKind.GAMEPAD
            (sources and InputDevice.SOURCE_MOUSE) != 0 -> NativeBridge.DeviceKind.MOUSE
            (sources and InputDevice.SOURCE_TOUCHSCREEN) != 0 -> NativeBridge.DeviceKind.TOUCHSCREEN
            else -> NativeBridge.DeviceKind.KEYBOARD
        }
    }

    // ----------------------------------------------------------------------- IME

    /**
     * Delivers committed IME text to the game as SDL text input.
     *
     * Hardware keys already carry their character through [sendKey]'s
     * `unicodeChar`; this path is for soft keyboards and IMEs, where the text
     * never appears as a key event.
     */
    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT
        outAttrs.imeOptions = EditorInfo.IME_ACTION_NONE or EditorInfo.IME_FLAG_NO_FULLSCREEN
        return object : BaseInputConnection(this@BridgeSurfaceView, false) {
            override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
                if (!text.isNullOrEmpty()) {
                    NativeBridge.sendText(text.toString(), SystemClock.uptimeMillis())
                }
                return true
            }
        }
    }

    /** Button mask of the previous mouse event, for [mouseButtonOf]. */
    private var lastMouseButtonState = 0

    // Reused across events: a ten-finger ACTION_MOVE at 120 Hz would otherwise
    // allocate five arrays per event. Indexes beyond `count` are never read by
    // the native side, which clamps `count` at 32.
    private val touchIds = IntArray(MAX_TOUCH_POINTERS)
    private val touchPhases = IntArray(MAX_TOUCH_POINTERS)
    private val touchXs = FloatArray(MAX_TOUCH_POINTERS)
    private val touchYs = FloatArray(MAX_TOUCH_POINTERS)
    private val touchPressures = FloatArray(MAX_TOUCH_POINTERS)

    private companion object {
        const val TAG = "BoardBridge"

        /** How many pointers this view forwards; Android's own limit is lower. */
        const val MAX_TOUCH_POINTERS = 16

        /**
         * Axes forwarded for a gamepad, i.e. the ones the SDL3 table maps.
         * Keep in sync with `gamepad_axis_from_android_axis` in
         * `boardbridge/src/input/sdl_tables.rs`.
         */
        val GAMEPAD_AXES = intArrayOf(
            MotionEvent.AXIS_X,
            MotionEvent.AXIS_Y,
            MotionEvent.AXIS_Z,
            MotionEvent.AXIS_RZ,
            MotionEvent.AXIS_LTRIGGER,
            MotionEvent.AXIS_RTRIGGER,
            MotionEvent.AXIS_HAT_X,
            MotionEvent.AXIS_HAT_Y,
        )
    }
}
