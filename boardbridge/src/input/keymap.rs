// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Android → SDL3 translation.
//!
//! This is the layer that makes the bridge's input meaningful to software built
//! against SDL3 (Minecraft 26.3+ through the LWJGL SDL bindings) *and* to the
//! GLFW compatibility path, which is built on the same physical-key notion.
//!
//! The rules implemented here are SDL3's own:
//!
//! * **Scancode** — the physical key position, taken from SDL3's Android
//!   keymap ([`super::sdl_tables`], generated from SDL's
//!   `src/video/android/SDL_androidkeyboard.c`). A key press therefore reports
//!   the same scancode as a desktop SDL3 build for the same physical key.
//! * **Keycode** — the layout-dependent key. SDL derives it from the character
//!   the layout produces (`SDL_Keycode` for text) and otherwise from the
//!   scancode with [`SDLK_SCANCODE_MASK`] set. Android hands us exactly that
//!   character through `KeyEvent.getUnicodeChar(metaState)`, which is why the
//!   Kotlin layer passes it instead of a keycode.
//! * **Modifiers** — `SDL_KMOD_*`, tracked by the bridge because SDL3 key events
//!   carry the modifier state that was active for the event.
//!
//! Nothing here guesses a mapping by numeric proximity: the key table is
//! generated data (see `tools/generate_sdl_tables.py`).

use crate::input::event::MouseButton;
use crate::input::sdl_tables as sdl;

/// SDL's marker for keycodes that have no character representation
/// (`SDL_SCANCODE_TO_KEYCODE(x) == x | SDLK_SCANCODE_MASK`).
pub const SDLK_SCANCODE_MASK: u32 = 1 << 30;

// Android `MotionEvent.BUTTON_*` constants. They are part of the public SDK and
// have been stable since API 14; the Kotlin layer forwards `event.actionButton`
// and `event.buttonState` verbatim.
/// Primary mouse button.
pub const ANDROID_BUTTON_PRIMARY: i32 = 1;
/// Secondary (right) mouse button.
pub const ANDROID_BUTTON_SECONDARY: i32 = 2;
/// Tertiary (middle) mouse button.
pub const ANDROID_BUTTON_TERTIARY: i32 = 4;
/// Back/extra mouse button.
pub const ANDROID_BUTTON_BACK: i32 = 8;
/// Forward/extra mouse button.
pub const ANDROID_BUTTON_FORWARD: i32 = 16;

/// Translates an Android `KeyEvent.keyCode` into an SDL scancode.
///
/// Keycodes outside the table (very new or vendor-specific keys) report
/// `SDL_SCANCODE_UNKNOWN`; the event is still delivered so that text input,
/// shortcuts and the IME keep working.
pub fn scancode_from_android_keycode(keycode: i32) -> u16 {
    if keycode < 0 {
        return sdl::SDL_SCANCODE_UNKNOWN;
    }
    let index = keycode as usize;
    if index >= sdl::ANDROID_KEYCODE_LIMIT {
        return sdl::SDL_SCANCODE_UNKNOWN;
    }
    sdl::ANDROID_KEYCODE_TO_SCANCODE[index]
}

/// Builds the SDL keycode for a key event.
///
/// `unicode` is the character Android produced for the event
/// (`KeyEvent.getUnicodeChar(metaState)`), or `0` when the key has no character.
pub fn sdl_keycode(scancode: u16, unicode: u32) -> u32 {
    if unicode != 0 {
        unicode
    } else {
        (scancode as u32) | SDLK_SCANCODE_MASK
    }
}

/// SDL gamepad button for an Android `AKEYCODE_BUTTON_*`, when there is one.
pub fn gamepad_button_from_android_keycode(keycode: i32) -> Option<i32> {
    sdl::gamepad_button_from_android_keycode(keycode)
}

/// SDL gamepad axis for an Android `AMOTION_EVENT_AXIS_*`, when there is one.
pub fn gamepad_axis_from_android_axis(android_axis: i32) -> Option<i32> {
    sdl::gamepad_axis_from_android(android_axis)
}

/// Clamps a raw Android axis value to the range SDL documents for that axis.
///
/// Android and SDL agree on stick ranges (-1..1) and trigger ranges (0..1) for
/// the devices seen so far; the clamp is here because some vendor drivers
/// report slightly out-of-range values and SDL never does.
pub fn normalize_gamepad_axis_value(sdl_axis: i32, value: f32) -> f32 {
    match sdl_axis {
        sdl::SDL_GAMEPAD_AXIS_LEFT_TRIGGER | sdl::SDL_GAMEPAD_AXIS_RIGHT_TRIGGER => {
            value.clamp(0.0, 1.0)
        }
        _ => value.clamp(-1.0, 1.0),
    }
}

/// Maps an Android `MotionEvent.actionButton` value to an SDL mouse button.
pub fn mouse_button_from_android(android_button: i32) -> Option<MouseButton> {
    match android_button {
        ANDROID_BUTTON_PRIMARY => Some(MouseButton::Left),
        ANDROID_BUTTON_TERTIARY => Some(MouseButton::Middle),
        ANDROID_BUTTON_SECONDARY => Some(MouseButton::Right),
        ANDROID_BUTTON_BACK => Some(MouseButton::Back),
        ANDROID_BUTTON_FORWARD => Some(MouseButton::Forward),
        _ => None,
    }
}

/// Converts Android's `buttonState` bitmask into SDL's `SDL_BUTTON_*` mask.
///
/// SDL numbers buttons from 1 and uses `1 << (button - 1)` in masks, which is
/// what a game passes to `SDL_GetMouseState`-style APIs.
pub fn mouse_button_mask_from_android_state(button_state: i32) -> u8 {
    let mut mask = 0u8;
    for (android, button) in [
        (ANDROID_BUTTON_PRIMARY, MouseButton::Left),
        (ANDROID_BUTTON_TERTIARY, MouseButton::Middle),
        (ANDROID_BUTTON_SECONDARY, MouseButton::Right),
        (ANDROID_BUTTON_BACK, MouseButton::Back),
        (ANDROID_BUTTON_FORWARD, MouseButton::Forward),
    ] {
        if button_state & android != 0 {
            mask |= 1u8 << (button.sdl() - 1);
        }
    }
    mask
}

/// Tracks `SDL_KMOD_*` state from key events.
///
/// SDL3 reports the modifier state *as of the event*, so the bridge has to keep
/// its own view; the Kotlin layer's `metaState` alone is not enough because it
/// does not distinguish left from right modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifierState {
    bits: u16,
}

impl ModifierState {
    /// Empty state.
    pub const fn new() -> ModifierState {
        ModifierState { bits: 0 }
    }

    /// Current `SDL_KMOD_*` bits.
    pub fn bits(&self) -> u16 {
        self.bits
    }

    /// Clears every modifier (used when the window loses focus).
    pub fn reset(&mut self) {
        self.bits = 0;
    }

    /// Applies one key event and returns the resulting modifier state.
    ///
    /// `repeat` matters for the lock keys: Android repeats `ACTION_DOWN` while a
    /// key is held, and a repeat must not toggle Caps Lock again.
    pub fn update_key(&mut self, scancode: u16, pressed: bool, repeat: bool) -> u16 {
        match scancode {
            sdl::SDL_SCANCODE_LSHIFT => self.set(sdl::SDL_KMOD_LSHIFT, pressed),
            sdl::SDL_SCANCODE_RSHIFT => self.set(sdl::SDL_KMOD_RSHIFT, pressed),
            sdl::SDL_SCANCODE_LCTRL => self.set(sdl::SDL_KMOD_LCTRL, pressed),
            sdl::SDL_SCANCODE_RCTRL => self.set(sdl::SDL_KMOD_RCTRL, pressed),
            sdl::SDL_SCANCODE_LALT => self.set(sdl::SDL_KMOD_LALT, pressed),
            sdl::SDL_SCANCODE_RALT => self.set(sdl::SDL_KMOD_RALT, pressed),
            sdl::SDL_SCANCODE_LGUI => self.set(sdl::SDL_KMOD_LGUI, pressed),
            sdl::SDL_SCANCODE_RGUI => self.set(sdl::SDL_KMOD_RGUI, pressed),
            sdl::SDL_SCANCODE_MODE => self.set(sdl::SDL_KMOD_MODE, pressed),
            sdl::SDL_SCANCODE_CAPSLOCK => self.toggle_on_press(sdl::SDL_KMOD_CAPS, pressed, repeat),
            sdl::SDL_SCANCODE_NUMLOCKCLEAR => {
                self.toggle_on_press(sdl::SDL_KMOD_NUM, pressed, repeat)
            }
            sdl::SDL_SCANCODE_SCROLLLOCK => {
                self.toggle_on_press(sdl::SDL_KMOD_SCROLL, pressed, repeat)
            }
            _ => {}
        }
        self.bits
    }

    fn set(&mut self, mask: u16, pressed: bool) {
        if pressed {
            self.bits |= mask;
        } else {
            self.bits &= !mask;
        }
    }

    fn toggle_on_press(&mut self, mask: u16, pressed: bool, repeat: bool) {
        if pressed && !repeat {
            self.bits ^= mask;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_digits_use_sdl_physical_positions() {
        // AKEYCODE_A = 29 -> SDL_SCANCODE_A = 4 (USB HID usage, as SDL uses).
        assert_eq!(scancode_from_android_keycode(29), sdl::SDL_SCANCODE_A);
        assert_eq!(scancode_from_android_keycode(29), 4);
        assert_eq!(scancode_from_android_keycode(54), sdl::SDL_SCANCODE_Z);
        // AKEYCODE_0 = 7 -> SDL_SCANCODE_0 = 39 (SDL is *not* numeric-order here).
        assert_eq!(scancode_from_android_keycode(7), sdl::SDL_SCANCODE_0);
        assert_eq!(scancode_from_android_keycode(7), 39);
    }

    #[test]
    fn named_keys_map_to_their_sdl_scancodes() {
        assert_eq!(scancode_from_android_keycode(111), sdl::SDL_SCANCODE_ESCAPE);
        assert_eq!(scancode_from_android_keycode(66), sdl::SDL_SCANCODE_RETURN);
        assert_eq!(scancode_from_android_keycode(61), sdl::SDL_SCANCODE_TAB);
        assert_eq!(scancode_from_android_keycode(62), sdl::SDL_SCANCODE_SPACE);
        assert_eq!(scancode_from_android_keycode(112), sdl::SDL_SCANCODE_DELETE);
        assert_eq!(scancode_from_android_keycode(19), sdl::SDL_SCANCODE_UP);
        assert_eq!(scancode_from_android_keycode(59), sdl::SDL_SCANCODE_LSHIFT);
        assert_eq!(scancode_from_android_keycode(60), sdl::SDL_SCANCODE_RSHIFT);
        assert_eq!(scancode_from_android_keycode(113), sdl::SDL_SCANCODE_LCTRL);
        assert_eq!(scancode_from_android_keycode(131), sdl::SDL_SCANCODE_F1);
    }

    #[test]
    fn unmapped_and_out_of_range_keycodes_degrade_to_unknown() {
        // AKEYCODE_CAMERA is a phone-only key SDL deliberately leaves unmapped.
        assert_eq!(scancode_from_android_keycode(27), sdl::SDL_SCANCODE_UNKNOWN);
        assert_eq!(
            scancode_from_android_keycode(9_999),
            sdl::SDL_SCANCODE_UNKNOWN
        );
        assert_eq!(scancode_from_android_keycode(-5), sdl::SDL_SCANCODE_UNKNOWN);
    }

    // The generated tables must really leave some keycodes unmapped, otherwise
    // the three assertions above would pass for the wrong reason. Checked at
    // compile time, so clippy does not have to treat it as a constant assertion.
    const _: () = assert!(sdl::ANDROID_KEYCODE_UNMAPPED_COUNT > 0);

    #[test]
    fn keycodes_are_characters_when_the_layout_produces_one() {
        // Pressing 'A' with no shift: Android reports the character 'a'.
        assert_eq!(sdl_keycode(sdl::SDL_SCANCODE_A, 'a' as u32), 'a' as u32);
        // Shift+A reports 'A'.
        assert_eq!(sdl_keycode(sdl::SDL_SCANCODE_A, 'A' as u32), 'A' as u32);
        // A key without a character falls back to scancode|mask, exactly like SDL.
        assert_eq!(
            sdl_keycode(sdl::SDL_SCANCODE_LEFT, 0),
            (sdl::SDL_SCANCODE_LEFT as u32) | SDLK_SCANCODE_MASK
        );
        assert_eq!(
            sdl_keycode(sdl::SDL_SCANCODE_DELETE, 0),
            (sdl::SDL_SCANCODE_DELETE as u32) | SDLK_SCANCODE_MASK
        );
    }

    #[test]
    fn modifier_state_tracks_left_and_right_separately() {
        let mut modifiers = ModifierState::new();
        assert_eq!(modifiers.bits(), 0);

        modifiers.update_key(sdl::SDL_SCANCODE_LSHIFT, true, false);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_LSHIFT);
        modifiers.update_key(sdl::SDL_SCANCODE_RSHIFT, true, false);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_SHIFT);
        modifiers.update_key(sdl::SDL_SCANCODE_LSHIFT, false, false);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_RSHIFT);

        modifiers.reset();
        assert_eq!(modifiers.bits(), 0);
    }

    #[test]
    fn lock_keys_toggle_once_per_press_not_per_repeat() {
        let mut modifiers = ModifierState::new();
        modifiers.update_key(sdl::SDL_SCANCODE_CAPSLOCK, true, false);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_CAPS);
        // Auto-repeat while held must not toggle it back.
        modifiers.update_key(sdl::SDL_SCANCODE_CAPSLOCK, true, true);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_CAPS);
        // Release does not toggle either.
        modifiers.update_key(sdl::SDL_SCANCODE_CAPSLOCK, false, false);
        assert_eq!(modifiers.bits(), sdl::SDL_KMOD_CAPS);
        // The next real press clears it.
        modifiers.update_key(sdl::SDL_SCANCODE_CAPSLOCK, true, false);
        assert_eq!(modifiers.bits(), 0);
    }

    #[test]
    fn mouse_buttons_follow_sdl_numbering() {
        assert_eq!(mouse_button_from_android(1), Some(MouseButton::Left));
        assert_eq!(mouse_button_from_android(2), Some(MouseButton::Right));
        assert_eq!(mouse_button_from_android(4), Some(MouseButton::Middle));
        assert_eq!(mouse_button_from_android(8), Some(MouseButton::Back));
        assert_eq!(mouse_button_from_android(16), Some(MouseButton::Forward));
        assert_eq!(mouse_button_from_android(64), None);

        // Left+Right held -> SDL mask 1 | 4 = 5.
        assert_eq!(mouse_button_mask_from_android_state(1 | 2), 1 | 4);
        assert_eq!(mouse_button_mask_from_android_state(0), 0);
    }

    #[test]
    fn gamepad_translation_uses_the_generated_tables() {
        assert_eq!(
            gamepad_axis_from_android_axis(sdl::AMOTION_EVENT_AXIS_X),
            Some(sdl::SDL_GAMEPAD_AXIS_LEFTX)
        );
        assert_eq!(
            gamepad_axis_from_android_axis(sdl::AMOTION_EVENT_AXIS_LTRIGGER),
            Some(sdl::SDL_GAMEPAD_AXIS_LEFT_TRIGGER)
        );
        assert_eq!(
            gamepad_axis_from_android_axis(sdl::AMOTION_EVENT_AXIS_VSCROLL),
            None
        );

        // AKEYCODE_BUTTON_A = 96 -> SDL_GAMEPAD_BUTTON_SOUTH.
        assert_eq!(
            gamepad_button_from_android_keycode(96),
            Some(sdl::SDL_GAMEPAD_BUTTON_SOUTH)
        );
        assert_eq!(gamepad_button_from_android_keycode(29), None);
    }

    #[test]
    fn axis_values_are_clamped_per_axis_kind() {
        assert_eq!(
            normalize_gamepad_axis_value(sdl::SDL_GAMEPAD_AXIS_LEFTX, 1.5),
            1.0
        );
        assert_eq!(
            normalize_gamepad_axis_value(sdl::SDL_GAMEPAD_AXIS_LEFTX, -1.5),
            -1.0
        );
        assert_eq!(
            normalize_gamepad_axis_value(sdl::SDL_GAMEPAD_AXIS_LEFT_TRIGGER, -0.4),
            0.0
        );
        assert_eq!(
            normalize_gamepad_axis_value(sdl::SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, 0.7),
            0.7
        );
    }

    #[test]
    fn scancode_names_are_available_for_logs() {
        assert_eq!(sdl::scancode_name(sdl::SDL_SCANCODE_A), "A");
        assert_eq!(sdl::scancode_name(sdl::SDL_SCANCODE_ESCAPE), "ESCAPE");
        assert_eq!(sdl::scancode_name(9999), "UNKNOWN");
    }
}
