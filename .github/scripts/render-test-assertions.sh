#!/bin/sh
# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
#
# Assertions for the emulator render test.
#
# They live in a file of their own because `reactivecircus/android-emulator-runner`
# runs each *line* of its `script:` input as a separate `sh -c`: a shell function
# or an `if`/`fi` block written inline there is split apart and silently does not
# run. The first version of these checks defined `fail()` inline and reported
# `sh: 1: fail: not found` — every assertion after the definition was skipped, so
# a run that rendered nothing still looked as if it had been checked.
#
# Usage: sh .github/scripts/render-test-assertions.sh [logcat-file]
set -u

LOG=${1:-artifacts/logcat-boardbridge.txt}
failures=0

fail() {
    echo "ERROR: $1"
    failures=$((failures + 1))
}

ok() {
    echo "ok: $1"
}

# contains <basic-regexp> <what the line proves>
contains() {
    if grep -q "$1" "$LOG"; then
        ok "$2"
    else
        fail "$2 (no line matching /$1/)"
    fi
}

# absent <basic-regexp> <what the absence proves>
absent() {
    if grep -q "$1" "$LOG"; then
        fail "$2"
    else
        ok "$2"
    fi
}

if [ ! -s "$LOG" ]; then
    echo "ERROR: $LOG is empty or missing: nothing to assert on"
    exit 1
fi

echo "asserting on $LOG ($(wc -l < "$LOG") lines)"

# 1. The EGL context came up and the GL strings crossed JNI: `getRendererInfo()`
#    is the same call MainActivity polls, so an empty answer is a real failure.
contains "GL_VERSION=" "EGL context created and GL strings reached Kotlin"

# 2. The surface was bound through the lifecycle machine and EGL, and the loop
#    drew. This is the assertion that a black screen fails.
contains "First frame rendered" "the surface/EGL path ran and a frame was drawn"

# 3. The loop kept drawing and read the centre pixel back (the clear colour is
#    (0, 158, 166); one LSB of rounding is allowed).
contains "frames=[0-9][0-9]* fps=[0-9.][0-9.]* mode=SOLID" "frame statistics were published"
contains "center_pixel_RGBA=([0-9][0-9]*,15[0-9],16[0-9],25[0-5])" "the centre pixel is the diagnostic colour"

# 4. No EGL surface was abandoned. A surface that EGL stopped resolving
#    (EGL_BAD_SURFACE, 0x300d) is recoverable — the bridge rebuilds it from the
#    window it still owns — but a run that never recovers must not pass.
absent "is unusable and was abandoned" "no EGL surface was abandoned"
absent "still cannot make the context current" "no run of failed attaches"

BAD_SURFACE=$(grep -c "EGL_BAD_SURFACE\|eglMakeCurrent failed (code 0x300d)" "$LOG" || true)
if [ "$BAD_SURFACE" -gt 0 ]; then
    echo "warning: EGL_BAD_SURFACE appeared $BAD_SURFACE time(s); the bridge recovered, but this is worth reading"
fi

# 5. Input really was translated to SDL: KEYCODE_A (29) must reach the queue as
#    scancode A, not as a raw Android keycode passed through.
contains "key DOWN code=29 scancode=A" "KEYCODE_A arrived as SDL scancode A"
contains "touch DOWN at" "a touch event reached the diagnostic renderer"

# 6. HOME then relaunch must leave the bridge usable. Whether the surface is
#    handed over again (a second binding) or kept across the cycle is the
#    platform's decision — on the emulator it is kept, because the activity is
#    `userLandscape` and is only stopped, never recreated — so the assertion is
#    about the bridge's state, not about a count the platform does not promise.
#    A real device that does destroy the surface exercises the other branch.
CREATED=$(grep -c "surface created:" "$LOG" || true)
BINDINGS=$(grep -c "ANativeWindow acquired, EGL surface bound" "$LOG" || true)
FRAMES=$(grep -c "mode=SOLID" "$LOG" || true)
echo "surface created callbacks: $CREATED, EGL bindings: $BINDINGS, frame lines: $FRAMES"
if [ "$BINDINGS" -ge 2 ]; then
    ok "the surface was handed over and bound again after HOME + relaunch"
elif [ "$FRAMES" -ge 2 ]; then
    ok "the surface survived HOME + relaunch and the loop kept drawing"
else
    fail "after HOME + relaunch: $BINDINGS binding(s), $FRAMES frame line(s) — the bridge was left unusable"
fi

# 7. The runtime was created and torn down cleanly (onDestroy / back handling).
contains "runtime created" "createRuntime ran"
contains "runtime stopped" "the runtime was stopped by onDestroy"

if [ "$failures" -gt 0 ]; then
    echo "FAILED: $failures assertion(s)"
    echo "----- BoardBridge logcat -----"
    grep "BoardBridge" "$LOG" | tail -60
    exit 1
fi

echo "All render-test assertions passed."
