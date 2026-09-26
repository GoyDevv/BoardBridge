// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Input event model.
//!
//! Events are shaped like **SDL3 events**, not like Android events, because the
//! consumer of this queue is either
//!
//! * the diagnostic renderer inside the bridge, or
//! * the game thread, which will read them through the LWJGL SDL bindings
//!   (`SDL_PollEvent`) or the GLFW compatibility backend.
//!
//! So a key event already carries a real SDL scancode/keycode/modifier set at
//! the point where it enters the queue — never a raw Android keycode pretending
//! to be an SDL one. Translation happens in [`crate::input::keymap`].

/// Which physical device the event came from.
///
/// Android does not label key events with the device class the way SDL does;
/// the Kotlin layer inspects `KeyEvent.getDevice()`/`InputDevice.getSources()`
/// and passes this tag along. It decides whether `AKEYCODE_BUTTON_*` becomes an
/// SDL *gamepad* button or a plain key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum DeviceKind {
    /// Unknown device; treated as a keyboard.
    Unknown = 0,
    /// Hardware or software keyboard / remote.
    Keyboard = 1,
    /// Gamepad or joystick.
    Gamepad = 2,
    /// Mouse, trackpad or stylus-with-buttons.
    Mouse = 3,
    /// Touchscreen (multi-touch finger input).
    Touchscreen = 4,
}

impl DeviceKind {
    /// Decodes the tag sent from Kotlin; unknown values are tolerated.
    pub fn from_jni(value: i32) -> DeviceKind {
        match value {
            1 => DeviceKind::Keyboard,
            2 => DeviceKind::Gamepad,
            3 => DeviceKind::Mouse,
            4 => DeviceKind::Touchscreen,
            _ => DeviceKind::Unknown,
        }
    }

    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            DeviceKind::Unknown => "unknown",
            DeviceKind::Keyboard => "keyboard",
            DeviceKind::Gamepad => "gamepad",
            DeviceKind::Mouse => "mouse",
            DeviceKind::Touchscreen => "touchscreen",
        }
    }
}

/// Normalized touch phase.
///
/// Android encodes the target pointer inside the action for
/// `ACTION_POINTER_DOWN`/`UP`, so the Kotlin layer normalizes every pointer in a
/// `MotionEvent` batch to one of these four phases before the batch crosses
/// JNI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TouchPhase {
    /// Finger/pen went down.
    Down = 0,
    /// Finger/pen moved.
    Move = 1,
    /// Finger/pen lifted.
    Up = 2,
    /// Gesture was cancelled (view detached, palm rejection, ...).
    Cancel = 3,
}

impl TouchPhase {
    /// Decodes the normalized phase sent from Kotlin.
    pub fn from_jni(value: i32) -> Option<TouchPhase> {
        match value {
            0 => Some(TouchPhase::Down),
            1 => Some(TouchPhase::Move),
            2 => Some(TouchPhase::Up),
            3 => Some(TouchPhase::Cancel),
            _ => None,
        }
    }

    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            TouchPhase::Down => "down",
            TouchPhase::Move => "move",
            TouchPhase::Up => "up",
            TouchPhase::Cancel => "cancel",
        }
    }
}

/// Mouse buttons, numbered like SDL (`SDL_BUTTON_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseButton {
    /// Primary button.
    Left = 1,
    /// Middle button.
    Middle = 2,
    /// Secondary button.
    Right = 3,
    /// Extra button 1 (usually "back").
    Back = 4,
    /// Extra button 2 (usually "forward").
    Forward = 5,
}

impl MouseButton {
    /// SDL `SDL_BUTTON_*` number.
    pub fn sdl(self) -> u8 {
        self as u8
    }

    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Middle => "middle",
            MouseButton::Right => "right",
            MouseButton::Back => "back",
            MouseButton::Forward => "forward",
        }
    }
}

/// A single input event, as consumed by the diagnostic renderer or the game.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// One pointer of a touch batch.
    Touch {
        /// Stable per-gesture pointer id.
        pointer_id: i32,
        /// Normalized phase.
        phase: TouchPhase,
        /// X in surface pixels.
        x: f32,
        /// Y in surface pixels.
        y: f32,
        /// Pressure, 0..1 (0 when the device reports none).
        pressure: f32,
        /// Event time in milliseconds, as reported by Android.
        timestamp_ms: i64,
    },
    /// Pointer motion. `relative` is set for raw/relative mouse mode, which is
    /// what Minecraft uses to steer the camera.
    MouseMotion {
        /// X delta (relative mode) or absolute X.
        x: f32,
        /// Y delta (relative mode) or absolute Y.
        y: f32,
        /// Whether `x`/`y` are deltas.
        relative: bool,
        /// Bitmask of held buttons, `SDL_BUTTON_*`.
        buttons: u8,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Mouse button press/release.
    MouseButton {
        /// Which button.
        button: MouseButton,
        /// `true` on press.
        pressed: bool,
        /// Absolute pointer position when known.
        x: f32,
        /// Absolute pointer position when known.
        y: f32,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Horizontal/vertical wheel motion.
    MouseWheel {
        /// Horizontal amount (positive = right).
        x: f32,
        /// Vertical amount (positive = away from the user).
        y: f32,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Key press/release/repeat, already translated to SDL semantics.
    Key {
        /// `true` for press (including repeats), `false` for release.
        pressed: bool,
        /// `true` when Android reported this as a key repeat.
        repeat: bool,
        /// SDL physical key position (`SDL_Scancode`).
        scancode: u16,
        /// SDL layout-dependent key code (`SDL_Keycode`).
        keycode: u32,
        /// Modifier state (`SDL_KMOD_*`) valid for this event.
        modifiers: u16,
        /// Original Android keycode, kept for logs and diagnostics.
        android_keycode: i32,
        /// Device class the key came from.
        device: DeviceKind,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Text committed by the IME (or synthesized from a key press when the IME
    /// is not involved). Carries UTF-8 as a Rust `String`.
    Text {
        /// The committed text.
        text: String,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Gamepad axis motion, translated to `SDL_GamepadAxis`.
    GamepadAxis {
        /// Android device id.
        device_id: i32,
        /// `SDL_GAMEPAD_AXIS_*`.
        axis: i32,
        /// Value in the SDL range (sticks -1..1, triggers 0..1).
        value: f32,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Gamepad button press/release, translated to `SDL_GamepadButton`.
    GamepadButton {
        /// Android device id.
        device_id: i32,
        /// `SDL_GAMEPAD_BUTTON_*`.
        button: i32,
        /// `true` on press.
        pressed: bool,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// A lifecycle change that the game thread must see, delivered through the
    /// same queue so a game event pump does not need a second JNI entry point.
    Lifecycle(LifecycleNotice),
}

impl InputEvent {
    /// Short kind name, for diagnostics.
    pub fn kind_name(&self) -> &'static str {
        match self {
            InputEvent::Touch { .. } => "touch",
            InputEvent::MouseMotion { .. } => "mouse_motion",
            InputEvent::MouseButton { .. } => "mouse_button",
            InputEvent::MouseWheel { .. } => "mouse_wheel",
            InputEvent::Key { .. } => "key",
            InputEvent::Text { .. } => "text",
            InputEvent::GamepadAxis { .. } => "gamepad_axis",
            InputEvent::GamepadButton { .. } => "gamepad_button",
            InputEvent::Lifecycle(_) => "lifecycle",
        }
    }

    /// Event timestamp in milliseconds.
    pub fn timestamp_ms(&self) -> i64 {
        match self {
            InputEvent::Touch { timestamp_ms, .. }
            | InputEvent::MouseMotion { timestamp_ms, .. }
            | InputEvent::MouseButton { timestamp_ms, .. }
            | InputEvent::MouseWheel { timestamp_ms, .. }
            | InputEvent::Key { timestamp_ms, .. }
            | InputEvent::Text { timestamp_ms, .. }
            | InputEvent::GamepadAxis { timestamp_ms, .. }
            | InputEvent::GamepadButton { timestamp_ms, .. } => *timestamp_ms,
            InputEvent::Lifecycle(notice) => notice.timestamp_ms(),
        }
    }
}

/// Lifecycle notifications delivered through the input queue.
///
/// This is the mechanism behind frame-loop inversion (§7): the game owns the
/// loop, so the bridge cannot call into it. Instead the next `poll` after a
/// surface is torn down reports [`LifecycleNotice::SurfaceRevoked`], and the
/// game is expected to stop presenting and release the context on its thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleNotice {
    /// A window became available (first frame may render).
    SurfaceAvailable {
        /// Surface width in pixels.
        width: i32,
        /// Surface height in pixels.
        height: i32,
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// The window went away; presenting must stop until further notice.
    SurfaceLost {
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// The surface this thread was presenting to was destroyed. The EGL
    /// context may still be current on the thread (a pbuffer keeps it alive
    /// when `preserve_context` is enabled), but the window surface is gone.
    SurfaceRevoked {
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Activity paused.
    Paused {
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
    /// Activity resumed.
    Resumed {
        /// Event time in milliseconds.
        timestamp_ms: i64,
    },
}

impl LifecycleNotice {
    /// Event timestamp in milliseconds.
    pub fn timestamp_ms(self) -> i64 {
        match self {
            LifecycleNotice::SurfaceAvailable { timestamp_ms, .. }
            | LifecycleNotice::SurfaceLost { timestamp_ms }
            | LifecycleNotice::SurfaceRevoked { timestamp_ms }
            | LifecycleNotice::Paused { timestamp_ms }
            | LifecycleNotice::Resumed { timestamp_ms } => timestamp_ms,
        }
    }

    /// Name for logs.
    pub fn name(self) -> &'static str {
        match self {
            LifecycleNotice::SurfaceAvailable { .. } => "surface_available",
            LifecycleNotice::SurfaceLost { .. } => "surface_lost",
            LifecycleNotice::SurfaceRevoked { .. } => "surface_revoked",
            LifecycleNotice::Paused { .. } => "paused",
            LifecycleNotice::Resumed { .. } => "resumed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_kind_decoding_is_tolerant() {
        assert_eq!(DeviceKind::from_jni(2), DeviceKind::Gamepad);
        assert_eq!(DeviceKind::from_jni(42), DeviceKind::Unknown);
        assert_eq!(DeviceKind::from_jni(-1), DeviceKind::Unknown);
    }

    #[test]
    fn touch_phase_decoding_rejects_garbage() {
        assert_eq!(TouchPhase::from_jni(0), Some(TouchPhase::Down));
        assert_eq!(TouchPhase::from_jni(3), Some(TouchPhase::Cancel));
        assert_eq!(TouchPhase::from_jni(7), None);
    }

    #[test]
    fn mouse_buttons_match_sdl_numbering() {
        assert_eq!(MouseButton::Left.sdl(), 1);
        assert_eq!(MouseButton::Middle.sdl(), 2);
        assert_eq!(MouseButton::Right.sdl(), 3);
        assert_eq!(MouseButton::Back.sdl(), 4);
        assert_eq!(MouseButton::Forward.sdl(), 5);
    }

    #[test]
    fn timestamps_are_reachable_for_every_variant() {
        let key = InputEvent::Key {
            pressed: true,
            repeat: false,
            scancode: 4,
            keycode: 0x61,
            modifiers: 0,
            android_keycode: 29,
            device: DeviceKind::Keyboard,
            timestamp_ms: 7,
        };
        assert_eq!(key.timestamp_ms(), 7);
        assert_eq!(key.kind_name(), "key");

        let notice = InputEvent::Lifecycle(LifecycleNotice::SurfaceLost { timestamp_ms: 9 });
        assert_eq!(notice.timestamp_ms(), 9);
        assert_eq!(notice.kind_name(), "lifecycle");

        let text = InputEvent::Text { text: "hi".to_string(), timestamp_ms: 11 };
        assert_eq!(text.timestamp_ms(), 11);
    }
}
