#!/usr/bin/env python3
# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
#
# Generates `boardbridge/src/input/sdl_tables.rs` from upstream source headers:
#
#   * SDL3 (https://github.com/libsdl-org/SDL, zlib license)
#       src/video/android/SDL_androidkeyboard.c  -> Android_Keycodes[] keymap
#       include/SDL3/SDL_scancode.h              -> SDL_Scancode values
#       include/SDL3/SDL_keycode.h               -> SDL_Keycode / SDL_KMOD_* values
#       include/SDL3/SDL_gamepad.h               -> SDL_Gamepad axes/buttons
#   * Android platform headers (AOSP, Apache-2.0)
#       frameworks/native/include/android/keycodes.h -> AKEYCODE_* values
#       frameworks/native/include/android/input.h    -> AMOTION_EVENT_AXIS_* values
#
# The tables are *derived data*, never hand-written: BoardBridge's input
# translation must agree with what SDL3 itself does on Android, otherwise the
# game sees different scancodes than a real SDL3 build would produce.
#
# Usage:
#   python3 tools/generate_sdl_tables.py                 # fetch + regenerate
#   python3 tools/generate_sdl_tables.py --check          # verify committed file
#   python3 tools/generate_sdl_tables.py --cache /tmp/sdl # reuse downloaded files
#
# Both upstreams track `main`, and AOSP's git server answers `503 Service
# Unavailable` often enough that a single attempt would turn CI red for reasons
# that have nothing to do with this repository. Downloads are therefore retried
# with a backoff, and when the network is still unreachable `--check` reports
# what it could not compare and skips instead of failing.
#
# Requires only the Python standard library.

import argparse
import base64
import hashlib
import os
import re
import sys
import time
import urllib.error
import urllib.request

SDL_REF = "main"
AOSP_REF = "refs/heads/main"

SDL_RAW = "https://raw.githubusercontent.com/libsdl-org/SDL/{ref}/".format(ref=SDL_REF)
AOSP_GIT = "https://android.googlesource.com/platform/frameworks/native/+/" + AOSP_REF + "/"

SOURCES = {
    "SDL_androidkeyboard.c": SDL_RAW + "src/video/android/SDL_androidkeyboard.c",
    "SDL_scancode.h": SDL_RAW + "include/SDL3/SDL_scancode.h",
    "SDL_keycode.h": SDL_RAW + "include/SDL3/SDL_keycode.h",
    "SDL_gamepad.h": SDL_RAW + "include/SDL3/SDL_gamepad.h",
    "keycodes.h": AOSP_GIT + "include/android/keycodes.h?format=TEXT",
    "input.h": AOSP_GIT + "include/android/input.h?format=TEXT",
}

OUTPUT = os.path.join("boardbridge", "src", "input", "sdl_tables.rs")

# How long to wait before each retry, so the number of attempts is
# `len(FETCH_DELAYS_SECONDS) + 1`.
FETCH_DELAYS_SECONDS = (1.0, 4.0, 10.0)


class FetchError(Exception):
    """An upstream header could not be downloaded."""


def download(url):
    """Downloads `url`, retrying only the failures that are worth retrying.

    HTTP 5xx and 429 (and plain network errors) get another chance after a
    backoff. Anything else — a 404, say — is reported immediately: retrying a
    missing file would only hide an upstream path that moved.
    """
    last = None
    for attempt in range(len(FETCH_DELAYS_SECONDS) + 1):
        if attempt:
            delay = FETCH_DELAYS_SECONDS[attempt - 1]
            print("  retrying in %.0fs" % delay, file=sys.stderr)
            time.sleep(delay)
        try:
            return urllib.request.urlopen(url, timeout=120).read()
        except urllib.error.HTTPError as error:
            if error.code < 500 and error.code != 429:
                raise FetchError("%s: HTTP %s" % (url, error.code)) from error
            last = error
        except (urllib.error.URLError, OSError) as error:
            last = error
    raise FetchError("%s: %s" % (url, last)) from last


def fetch(cache_dir, name, url):
    os.makedirs(cache_dir, exist_ok=True)
    path = os.path.join(cache_dir, name)
    if not os.path.exists(path):
        print("  fetching %s" % name, file=sys.stderr)
        raw = download(url)
        if url.endswith("format=TEXT"):
            raw = base64.b64decode(raw)
        with open(path, "wb") as handle:
            handle.write(raw)
    with open(path, "rb") as handle:
        data = handle.read()
    return data.decode("utf-8", "replace"), hashlib.sha256(data).hexdigest()


def parse_sdl_scancodes(text):
    """SDL_SCANCODE_FOO = 4, -> {"SDL_SCANCODE_FOO": 4}"""
    return {
        m.group(1): int(m.group(2))
        for m in re.finditer(r"^\s*(SDL_SCANCODE_[A-Z0-9_]+)\s*=\s*(\d+)\s*,", text, re.M)
    }


def parse_sdl_android_keymap(text):
    """The positional Android_Keycodes[] table: index -> (SDL scancode name, AKEYCODE name)."""
    body = text.split("Android_Keycodes[] = {", 1)
    if len(body) != 2:
        raise SystemExit("could not find Android_Keycodes[] in SDL_androidkeyboard.c")
    body = body[1].split("};", 1)[0]
    entries = []
    for line in body.splitlines():
        match = re.match(r"\s*(SDL_SCANCODE_[A-Z0-9_]+)\s*,\s*//\s*(AKEYCODE_[A-Z0-9_]+)", line)
        if match:
            entries.append((match.group(1), match.group(2)))
    return entries


def parse_android_keycodes(text):
    """AKEYCODE_A = 29, / AKEYCODE_MOVE_HOME = AKEYCODE_HOME, -> name -> int (aliases resolved)."""
    raw = {}
    for match in re.finditer(r"^\s*(AKEYCODE_[A-Z0-9_]+)\s*=\s*([A-Za-z0-9_]+)\s*,", text, re.M):
        raw[match.group(1)] = match.group(2)
    resolved = {}
    for name in raw:
        seen = set()
        value = raw[name]
        while not value.isdigit():
            seen.add(name)
            if value in resolved:
                value = str(resolved[value])
                break
            if value not in raw or value in seen:
                value = None
                break
            value = raw[value]
        resolved[name] = int(value) if value is not None and value.isdigit() else None
    return {name: value for name, value in resolved.items() if value is not None}


def parse_defines(text, prefix):
    """#define PREFIX_FOO 0x0001u -> {"PREFIX_FOO": 1}"""
    out = {}
    for match in re.finditer(r"^\s*#define\s+(" + prefix + r"[A-Z0-9_]+)\s+\(?(0x[0-9a-fA-F]+|\d+)[uUlL]*\)?", text, re.M):
        out[match.group(1)] = int(match.group(2), 0)
    return out


def parse_enum_values(text, prefix):
    """Walks enum members that may or may not carry an explicit value.

    SDL3 declares e.g. `SDL_GAMEPAD_BUTTON_SOUTH,` with implicit values, while
    Android's input.h writes `AMOTION_EVENT_AXIS_X = 0,`. Members of any other
    enum interleaved in between would break the running counter, so callers
    must verify a couple of known values afterwards (see INVARIANTS).
    """
    out = {}
    counter = 0
    for match in re.finditer(
        r"^\s*(" + prefix + r"[A-Z0-9_]+)\s*(?:=\s*(-?\d+))?\s*,", text, re.M
    ):
        if match.group(2) is not None:
            counter = int(match.group(2))
        out[match.group(1)] = counter
        counter += 1
    return out


# Guard rails: if an upstream header is reformatted we must fail loudly rather
# than silently emit a wrong translation table.
INVARIANTS = [
    # (source key, parsed dict, expected value)
    ("SDL_SCANCODE_ESCAPE", "scancodes", 41),
    ("SDL_SCANCODE_0", "scancodes", 39),
    ("SDL_SCANCODE_A", "scancodes", 4),
    ("SDL_SCANCODE_LCTRL", "scancodes", 224),
    ("SDL_SCANCODE_LSHIFT", "scancodes", 225),
    ("SDL_SCANCODE_LGUI", "scancodes", 227),
    ("SDL_SCANCODE_CAPSLOCK", "scancodes", 57),
    ("SDL_GAMEPAD_AXIS_LEFTX", "gp_axes", 0),
    ("SDL_GAMEPAD_AXIS_LEFT_TRIGGER", "gp_axes", 4),
    ("SDL_GAMEPAD_BUTTON_SOUTH", "gp_buttons", 0),
    ("SDL_GAMEPAD_BUTTON_NORTH", "gp_buttons", 3),
    ("SDL_GAMEPAD_BUTTON_LEFT_SHOULDER", "gp_buttons", 9),
    ("SDL_GAMEPAD_BUTTON_DPAD_UP", "gp_buttons", 11),
    ("AMOTION_EVENT_AXIS_X", "axes", 0),
    ("AMOTION_EVENT_AXIS_RZ", "axes", 14),
    ("AMOTION_EVENT_AXIS_LTRIGGER", "axes", 17),
    ("SDL_KMOD_LSHIFT", "kmods", 0x0001),
    ("SDL_KMOD_RGUI", "kmods", 0x0800),
    ("AKEYCODE_A", "akeycodes", 29),
    ("AKEYCODE_BUTTON_A", "akeycodes", 96),
]


def parse_axis_defines(text):
    return parse_defines(text, "AMOTION_EVENT_AXIS_")


# Android gamepad buttons (AKEYCODE_BUTTON_*) -> SDL_Gamepad buttons. L2/R2 are
# deliberately absent: Android reports them both as buttons and as the
# LTRIGGER/RTRIGGER axes, and SDL models them as axes.
GAMEPAD_BUTTONS = [
    ("BUTTON_A", "SDL_GAMEPAD_BUTTON_SOUTH"),
    ("BUTTON_B", "SDL_GAMEPAD_BUTTON_EAST"),
    ("BUTTON_X", "SDL_GAMEPAD_BUTTON_WEST"),
    ("BUTTON_Y", "SDL_GAMEPAD_BUTTON_NORTH"),
    ("BUTTON_L1", "SDL_GAMEPAD_BUTTON_LEFT_SHOULDER"),
    ("BUTTON_R1", "SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER"),
    ("BUTTON_THUMBL", "SDL_GAMEPAD_BUTTON_LEFT_STICK"),
    ("BUTTON_THUMBR", "SDL_GAMEPAD_BUTTON_RIGHT_STICK"),
    ("BUTTON_START", "SDL_GAMEPAD_BUTTON_START"),
    ("BUTTON_SELECT", "SDL_GAMEPAD_BUTTON_BACK"),
    ("BUTTON_MODE", "SDL_GAMEPAD_BUTTON_GUIDE"),
    ("BUTTON_DPAD_UP", "SDL_GAMEPAD_BUTTON_DPAD_UP"),
    ("BUTTON_DPAD_DOWN", "SDL_GAMEPAD_BUTTON_DPAD_DOWN"),
    ("BUTTON_DPAD_LEFT", "SDL_GAMEPAD_BUTTON_DPAD_LEFT"),
    ("BUTTON_DPAD_RIGHT", "SDL_GAMEPAD_BUTTON_DPAD_RIGHT"),
    ("BUTTON_1", "SDL_GAMEPAD_BUTTON_MISC1"),
    ("BUTTON_2", "SDL_GAMEPAD_BUTTON_MISC2"),
    ("BUTTON_3", "SDL_GAMEPAD_BUTTON_MISC3"),
    ("BUTTON_4", "SDL_GAMEPAD_BUTTON_MISC4"),
    ("BUTTON_5", "SDL_GAMEPAD_BUTTON_MISC5"),
    ("BUTTON_6", "SDL_GAMEPAD_BUTTON_MISC6"),
    ("BUTTON_7", "SDL_GAMEPAD_BUTTON_PADDLE1"),
    ("BUTTON_8", "SDL_GAMEPAD_BUTTON_PADDLE2"),
    ("BUTTON_9", "SDL_GAMEPAD_BUTTON_PADDLE3"),
    ("BUTTON_10", "SDL_GAMEPAD_BUTTON_PADDLE4"),
]

# AMOTION_EVENT_AXIS_* -> SDL_Gamepad axes.
GAMEPAD_AXES = [
    ("AMOTION_EVENT_AXIS_X", "SDL_GAMEPAD_AXIS_LEFTX"),
    ("AMOTION_EVENT_AXIS_Y", "SDL_GAMEPAD_AXIS_LEFTY"),
    ("AMOTION_EVENT_AXIS_Z", "SDL_GAMEPAD_AXIS_RIGHTX"),
    ("AMOTION_EVENT_AXIS_RZ", "SDL_GAMEPAD_AXIS_RIGHTY"),
    ("AMOTION_EVENT_AXIS_LTRIGGER", "SDL_GAMEPAD_AXIS_LEFT_TRIGGER"),
    ("AMOTION_EVENT_AXIS_RTRIGGER", "SDL_GAMEPAD_AXIS_RIGHT_TRIGGER"),
    ("AMOTION_EVENT_AXIS_HAT_X", "SDL_GAMEPAD_AXIS_LEFTX"),
    ("AMOTION_EVENT_AXIS_HAT_Y", "SDL_GAMEPAD_AXIS_LEFTY"),
]


def emit(files, hashes):
    scancodes = files["scancodes"]
    keymap = files["keymap"]
    akeycodes = files["akeycodes"]
    axes = files["axes"]
    gp_axes = files["gp_axes"]
    gp_buttons = files["gp_buttons"]
    kmods = files["kmods"]

    out = []
    add = out.append
    add("// Copyright 2026 The BoardBridge Authors")
    add("// Licensed under the Apache License, Version 2.0.")
    add("//")
    add("// @generated by tools/generate_sdl_tables.py -- DO NOT EDIT BY HAND.")
    add("//")
    add("// Derived from SDL3 (zlib license) and Android platform headers")
    add("// (Apache-2.0). See NOTICE for attribution. Regenerate with:")
    add("//")
    add("//     python3 tools/generate_sdl_tables.py")
    add("//")
    add("// Source digests (sha256 of the upstream files this table was generated from):")
    for name in sorted(hashes):
        add("//   %-24s %s" % (name, hashes[name]))
    add("")
    add("/// Android keycodes that can be translated (indices 0..N-1 of")
    add("/// [`ANDROID_KEYCODE_TO_SCANCODE`]); anything larger is unmapped and is")
    add("/// reported to the caller as `SDL_SCANCODE_UNKNOWN`.")
    add("pub const ANDROID_KEYCODE_LIMIT: usize = %d;" % len(keymap))
    add("")

    add("// ---- SDL_Scancode (SDL3, include/SDL3/SDL_scancode.h) ----")
    add("")
    for name in sorted(scancodes, key=lambda n: scancodes[n]):
        add("pub const %s: u16 = %d;" % (name, scancodes[name]))
    add("")
    add("/// Every `SDL_Scancode` value that has a name, for diagnostics.")
    add("pub const SDL_SCANCODE_NAMES: [(u16, &str); %d] = [" % len(scancodes))
    for name in sorted(scancodes, key=lambda n: scancodes[n]):
        add('    (%d, "%s"),' % (scancodes[name], name[len("SDL_SCANCODE_"):]))
    add("];")
    add("")
    add("/// Human-readable name of a scancode, for logging.")
    add("pub fn scancode_name(scancode: u16) -> &'static str {")
    add("    match scancode {")
    for name in sorted(scancodes, key=lambda n: scancodes[n]):
        add('        %d => "%s",' % (scancodes[name], name[len("SDL_SCANCODE_"):]))
    add('        _ => "UNKNOWN",')
    add("    }")
    add("}")
    add("")

    add("// ---- Android KeyEvent keycode -> SDL_Scancode ----")
    add("//")
    add("// This is SDL3's own Android keymap (src/video/android/SDL_androidkeyboard.c).")
    add("// Using the same mapping is what makes the bridge report the same physical")
    add("// key *positions* (SDL scancodes) as a desktop SDL3 build would.")
    add("")
    unknown = scancodes["SDL_SCANCODE_UNKNOWN"]
    missing = sorted({scancode for scancode, _ in keymap if scancode not in scancodes})
    if missing:
        raise SystemExit(
            "SDL_Scancode names used by the Android keymap but not parsed from "
            "SDL_scancode.h (update parse_sdl_scancodes): %s" % ", ".join(missing)
        )

    add("pub static ANDROID_KEYCODE_TO_SCANCODE: [u16; %d] = [" % len(keymap))
    unmapped = 0
    for index, (scancode, akeycode) in enumerate(keymap):
        value = scancodes[scancode]
        if (value == unknown) and akeycode != "AKEYCODE_UNKNOWN":
            unmapped += 1
        add("    %d, // %s -> %s" % (value, akeycode, scancode[len("SDL_SCANCODE_"):]))
    add("];")
    add("")
    add("/// Android keycodes that SDL3 itself leaves unmapped: keys with no")
    add("/// equivalent physical key on a desktop keyboard (media keys, vendor")
    add("/// buttons, ...). They are still delivered as key events with")
    add("/// `SDL_SCANCODE_UNKNOWN` so that text input keeps working.")
    add("pub const ANDROID_KEYCODE_UNMAPPED_COUNT: usize = %d;" % unmapped)
    add("")

    add("// ---- SDL key modifier state (SDL3, include/SDL3/SDL_keycode.h) ----")
    add("")
    for name in sorted(kmods):
        add("pub const %s: u16 = 0x%04x;" % (name, kmods[name]))
    for combined in ("SHIFT", "CTRL", "ALT", "GUI"):
        parts = [kmods[n] for n in kmods if n in ("SDL_KMOD_L" + combined, "SDL_KMOD_R" + combined)]
        if parts:
            add("pub const SDL_KMOD_%s: u16 = 0x%04x;" % (combined, parts[0] | parts[1]))
    add("")

    add("// ---- SDL_Gamepad axes / buttons (SDL3, include/SDL3/SDL_gamepad.h) ----")
    add("")
    for name in sorted(gp_axes, key=lambda n: gp_axes[n]):
        add("pub const %s: i32 = %d;" % (name, gp_axes[name]))
    for name in sorted(gp_buttons, key=lambda n: gp_buttons[n]):
        add("pub const %s: i32 = %d;" % (name, gp_buttons[name]))
    add("")

    add("// ---- Android gamepad axes (AOSP, include/android/input.h) ----")
    add("")
    for name in sorted(axes):
        add("pub const %s: i32 = %d;" % (name, axes[name]))
    add("")

    add("/// Translates an Android joystick axis id into an SDL_Gamepad axis.")
    add("///")
    add("/// Android reports gamepad sticks/triggers through `AMOTION_EVENT_AXIS_*`")
    add("/// ids; SDL uses a small fixed set of `SDL_GAMEPAD_AXIS_*` axes. The HAT")
    add("/// axes are folded onto the left stick because SDL exposes the d-pad as")
    add("/// buttons, not axes.")
    add("pub fn gamepad_axis_from_android(android_axis: i32) -> Option<i32> {")
    add("    match android_axis {")
    for a_axis, s_axis in GAMEPAD_AXES:
        if a_axis not in axes:
            continue
        add("        %s => Some(%s)," % (a_axis, s_axis))
    add("        _ => None,")
    add("    }")
    add("}")
    add("")

    add("/// Translates an Android gamepad button keycode (`AKEYCODE_BUTTON_*`) into")
    add("/// an SDL_Gamepad button. Android reports L2/R2 as buttons *and* as the")
    add("/// LTRIGGER/RTRIGGER axes; SDL models them as axes only, so those two")
    add("/// report `None` here and arrive through [`gamepad_axis_from_android`].")
    add("pub fn gamepad_button_from_android_keycode(keycode: i32) -> Option<i32> {")
    add("    match keycode {")
    for suffix, s_button in GAMEPAD_BUTTONS:
        a_name = "AKEYCODE_" + suffix
        if a_name not in akeycodes or s_button not in gp_buttons:
            continue
        add("        %d => Some(%s), // %s" % (akeycodes[a_name], s_button, a_name))
    add("        _ => None,")
    add("    }")
    add("}")
    add("")
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", default=".sdl_tables_cache", help="download cache directory")
    parser.add_argument("--check", action="store_true", help="fail if the committed file is stale")
    args = parser.parse_args()

    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(root)

    files = {}
    hashes = {}
    try:
        for name, url in SOURCES.items():
            files[name], hashes[name] = fetch(args.cache, name, url)
    except FetchError as error:
        # Without upstream there is nothing to compare *against*. Saying so
        # loudly (GitHub renders `::warning::` as an annotation on the step) is
        # more honest than a red build that blames this repository for an
        # upstream outage — and more useful than passing silently.
        if args.check:
            print("::warning::%s" % error)
            print(
                "  skipping the freshness check: %s" % error,
                file=sys.stderr,
            )
            return 0
        print("error: %s" % error, file=sys.stderr)
        print(
            "  nothing was written; the committed tables are unchanged",
            file=sys.stderr,
        )
        return 2

    scancodes = parse_sdl_scancodes(files["SDL_scancode.h"])
    keymap = parse_sdl_android_keymap(files["SDL_androidkeyboard.c"])
    akeycodes = parse_android_keycodes(files["keycodes.h"])

    mismatches = [
        (index, akeycode, akeycodes.get(akeycode))
        for index, (_scancode, akeycode) in enumerate(keymap)
        if akeycodes.get(akeycode) != index
    ]
    if mismatches:
        print("warning: SDL keymap is not dense in AKEYCODE order:", file=sys.stderr)
        for index, akeycode, value in mismatches[:10]:
            print("  index %d: %s = %s" % (index, akeycode, value), file=sys.stderr)

    parsed = {
        "scancodes": scancodes,
        "keymap": keymap,
        "akeycodes": akeycodes,
        "axes": parse_enum_values(files["input.h"], "AMOTION_EVENT_AXIS_"),
        "gp_axes": parse_enum_values(files["SDL_gamepad.h"], "SDL_GAMEPAD_AXIS_"),
        "gp_buttons": parse_enum_values(files["SDL_gamepad.h"], "SDL_GAMEPAD_BUTTON_"),
        "kmods": parse_defines(files["SDL_keycode.h"], "SDL_KMOD_"),
    }
    for name, source, expected in INVARIANTS:
        actual = parsed[source].get(name)
        if actual != expected:
            raise SystemExit(
                "invariant failed: %s should be %s but parsed as %s from an "
                "upstream header; the parser needs updating" % (name, expected, actual)
            )

    rendered = emit(parsed, hashes)

    if args.check:
        with open(OUTPUT, "r", encoding="utf-8") as handle:
            if handle.read() != rendered:
                print("%s is stale; re-run tools/generate_sdl_tables.py" % OUTPUT, file=sys.stderr)
                return 1
        print("%s is up to date" % OUTPUT)
        return 0

    with open(OUTPUT, "w", encoding="utf-8") as handle:
        handle.write(rendered)
    print("wrote %s (%d lines)" % (OUTPUT, rendered.count("\n")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
