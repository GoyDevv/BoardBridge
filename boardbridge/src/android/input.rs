// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Adapters from raw JNI arguments to SDL-shaped [`InputEvent`]s.
//!
//! The JNI layer stays thin: it reads the primitive arguments of one Kotlin call
//! (`sendKey`, `sendTouchBatch`, ...), hands them to the functions here, and the
//! runtime pushes the resulting events onto the queue. All Android semantics —
//! which keycode means which physical key, when a gamepad button is a gamepad
//! button — live in this module and in [`crate::input::keymap`].

use crate::input::event::{DeviceKind, InputEvent, TouchPhase};
use crate::input::keymap::{self, ModifierState};
use crate::input::sdl_tables as sdl;

/// Key event arguments as they arrive from Kotlin.
#[derive(Clone, Copy, Debug)]
pub struct KeyInput {
    /// Android `KeyEvent.getKeyCode()`.
    pub android_keycode: i32,
    /// `true` on `ACTION_DOWN`.
    pub pressed: bool,
    /// `true` when Android reported a repeat (`KeyEvent.getRepeatCount() > 0`).
    pub repeat: bool,
    /// `KeyEvent.getUnicodeChar(metaState)`, or 0.
    pub unicode: u32,
    /// Device class resolved by Kotlin from `KeyEvent.getDevice()`.
    pub device: DeviceKind,
    /// Android input device id (0 when unknown).
    pub device_id: i32,
    /// `SDL_KMOD_*` bits *after* applying this event (see [`update_modifiers`]).
    pub modifiers: u16,
    /// Event time in milliseconds.
    pub timestamp_ms: i64,
}

/// Applies one key event to the modifier state and returns the new bits.
///
/// Call this before [`key_event`] so the event carries the modifier state as of
/// that event — the same thing SDL3 does.
pub fn update_modifiers(
    modifiers: &mut ModifierState,
    android_keycode: i32,
    pressed: bool,
    repeat: bool,
) -> u16 {
    let scancode = keymap::scancode_from_android_keycode(android_keycode);
    modifiers.update_key(scancode, pressed, repeat)
}

/// Builds a key event.
///
/// A key event from a *gamepad* whose keycode is an `AKEYCODE_BUTTON_*` becomes
/// a gamepad button event instead, which is what SDL3 exposes for controllers.
/// D-pad presses are deliberately *not* converted: Android reports them as
/// `AKEYCODE_DPAD_*` key events and SDL3's Android backend maps them to the
/// arrow-key scancodes, which is also what a GLFW-based Minecraft expects.
pub fn key_event(input: KeyInput) -> InputEvent {
    if input.device == DeviceKind::Gamepad {
        if let Some(button) = keymap::gamepad_button_from_android_keycode(input.android_keycode) {
            return InputEvent::GamepadButton {
                device_id: input.device_id,
                button,
                pressed: input.pressed,
                timestamp_ms: input.timestamp_ms,
            };
        }
    }
    let scancode = keymap::scancode_from_android_keycode(input.android_keycode);
    InputEvent::Key {
        pressed: input.pressed,
        repeat: input.repeat,
        scancode,
        keycode: keymap::sdl_keycode(scancode, input.unicode),
        modifiers: input.modifiers,
        android_keycode: input.android_keycode,
        device: input.device,
        timestamp_ms: input.timestamp_ms,
    }
}

/// Builds a touch event. `phase` has already been normalized by Kotlin.
pub fn touch_event(
    pointer_id: i32,
    phase: TouchPhase,
    x: f32,
    y: f32,
    pressure: f32,
    timestamp_ms: i64,
) -> InputEvent {
    InputEvent::Touch {
        pointer_id,
        phase,
        x,
        y,
        pressure: pressure.clamp(0.0, 1.0),
        timestamp_ms,
    }
}

/// Builds pointer motion. `relative` marks raw/relative mouse mode, where `x`
/// and `y` are deltas — this is the path Minecraft uses for camera steering.
pub fn mouse_motion_event(
    x: f32,
    y: f32,
    relative: bool,
    buttons: u8,
    timestamp_ms: i64,
) -> InputEvent {
    InputEvent::MouseMotion {
        x,
        y,
        relative,
        buttons,
        timestamp_ms,
    }
}

/// Builds a mouse button event; `None` for buttons the bridge does not model.
pub fn mouse_button_event(
    android_button: i32,
    pressed: bool,
    x: f32,
    y: f32,
    timestamp_ms: i64,
) -> Option<InputEvent> {
    keymap::mouse_button_from_android(android_button).map(|button| InputEvent::MouseButton {
        button,
        pressed,
        x,
        y,
        timestamp_ms,
    })
}

/// Builds a wheel event. SDL's convention is positive = right / away from user.
pub fn mouse_wheel_event(x: f32, y: f32, timestamp_ms: i64) -> InputEvent {
    InputEvent::MouseWheel { x, y, timestamp_ms }
}

/// Builds a gamepad axis event; `None` for axes SDL models differently (for
/// example the d-pad, which SDL exposes as buttons).
pub fn gamepad_axis_event(
    device_id: i32,
    android_axis: i32,
    value: f32,
    timestamp_ms: i64,
) -> Option<InputEvent> {
    keymap::gamepad_axis_from_android_axis(android_axis).map(|axis| InputEvent::GamepadAxis {
        device_id,
        axis,
        value: keymap::normalize_gamepad_axis_value(axis, value),
        timestamp_ms,
    })
}

/// Builds a text event from IME or synthesized input.
pub fn text_event(text: String, timestamp_ms: i64) -> InputEvent {
    InputEvent::Text { text, timestamp_ms }
}

/// The SDL scancode for an Android keycode, for logging.
pub fn scancode_name_for(android_keycode: i32) -> &'static str {
    sdl::scancode_name(keymap::scancode_from_android_keycode(android_keycode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{MouseButton, TouchPhase};

    #[test]
    fn keyboard_keys_carry_sdl_scancodes_and_modifiers() {
        let mut modifiers = ModifierState::new();
        let bits = update_modifiers(&mut modifiers, 59, true, false); // AKEYCODE_SHIFT_LEFT
        let event = key_event(KeyInput {
            android_keycode: 29, // AKEYCODE_A
            pressed: true,
            repeat: false,
            unicode: 'A' as u32,
            device: DeviceKind::Keyboard,
            device_id: 1,
            modifiers: bits,
            timestamp_ms: 10,
        });
        match event {
            InputEvent::Key { scancode, keycode, modifiers, .. } => {
                assert_eq!(scancode, sdl::SDL_SCANCODE_A);
                assert_eq!(keycode, 'A' as u32);
                assert_eq!(modifiers, sdl::SDL_KMOD_LSHIFT);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn gamepad_buttons_become_sdl_gamepad_buttons() {
        let event = key_event(KeyInput {
            android_keycode: 96, // AKEYCODE_BUTTON_A
            pressed: true,
            repeat: false,
            unicode: 0,
            device: DeviceKind::Gamepad,
            device_id: 42,
            modifiers: 0,
            timestamp_ms: 11,
        });
        assert_eq!(
            event,
            InputEvent::GamepadButton {
                device_id: 42,
                button: sdl::SDL_GAMEPAD_BUTTON_SOUTH,
                pressed: true,
                timestamp_ms: 11,
            }
        );
    }

    #[test]
    fn dpad_on_a_gamepad_stays_a_key_event() {
        let event = key_event(KeyInput {
            android_keycode: 19, // AKEYCODE_DPAD_UP
            pressed: true,
            repeat: false,
            unicode: 0,
            device: DeviceKind::Gamepad,
            device_id: 42,
            modifiers: 0,
            timestamp_ms: 12,
        });
        match event {
            InputEvent::Key { scancode, .. } => assert_eq!(scancode, sdl::SDL_SCANCODE_UP),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn gamepad_axes_are_translated_and_clamped() {
        let event = gamepad_axis_event(7, sdl::AMOTION_EVENT_AXIS_LTRIGGER, -0.5, 13).unwrap();
        assert_eq!(
            event,
            InputEvent::GamepadAxis {
                device_id: 7,
                axis: sdl::SDL_GAMEPAD_AXIS_LEFT_TRIGGER,
                value: 0.0,
                timestamp_ms: 13,
            }
        );
        assert!(gamepad_axis_event(7, sdl::AMOTION_EVENT_AXIS_VSCROLL, 1.0, 14).is_none());
    }

    #[test]
    fn mouse_events_are_shaped_like_sdl() {
        let motion = mouse_motion_event(3.0, -4.0, true, 1, 15);
        assert_eq!(
            motion,
            InputEvent::MouseMotion {
                x: 3.0,
                y: -4.0,
                relative: true,
                buttons: 1,
                timestamp_ms: 15,
            }
        );
        let button = mouse_button_event(2, true, 10.0, 20.0, 16).unwrap();
        match button {
            InputEvent::MouseButton { button, pressed, .. } => {
                assert_eq!(button, MouseButton::Right);
                assert!(pressed);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(mouse_button_event(64, true, 0.0, 0.0, 17).is_none());
    }

    #[test]
    fn touch_pressure_is_clamped() {
        let event = touch_event(0, TouchPhase::Down, 1.0, 2.0, 5.0, 18);
        match event {
            InputEvent::Touch { pressure, phase, .. } => {
                assert_eq!(pressure, 1.0);
                assert_eq!(phase, TouchPhase::Down);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn scancode_names_are_loggable() {
        assert_eq!(scancode_name_for(29), "A");
        assert_eq!(scancode_name_for(111), "ESCAPE");
        assert_eq!(scancode_name_for(27), "UNKNOWN");
    }
}
