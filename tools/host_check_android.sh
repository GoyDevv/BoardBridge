#!/bin/sh
# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
#
# Typechecks the Android-only code paths (`graphics/gles.rs`, `egl/{display,
# context,surface}.rs`, `android/*`, `platform/*`, `render/diagnostics.rs`) on a
# host that has no NDK and no C toolchain — a plain `cargo check` on the host
# skips all of them, which is how a wrong method placement in
# `impl GraphicsBackend for GlesBackend` reached CI once already.
#
# How it works: copy the crate to a scratch directory, strip every
# `cfg(target_os = "android")` gate so the Android code is compiled by the host
# toolchain, and drop the `jni` module (its crate needs `cc` for its build
# script). This is a *type/lint* check only — nothing is linked, no NDK symbol
# is resolved, and the Android build is still verified by CI.
#
# Usage: tools/host_check_android.sh [path-to-crate]   (default: boardbridge)
set -eu

SRC=${1:-boardbridge}
OUT=${HOST_CHECK_DIR:-/tmp/boardbridge-host-check}

rm -rf "$OUT"
mkdir -p "$OUT"
cp -a "$SRC/Cargo.toml" "$OUT/"
[ -f "$SRC/Cargo.lock" ] && cp -a "$SRC/Cargo.lock" "$OUT/"
cp -a "$SRC/src" "$OUT/src"

cd "$OUT"
rm -rf src/jni

python3 - <<'PY'
import pathlib
import re

for path in pathlib.Path("src").rglob("*.rs"):
    text = path.read_text()
    # Compile the Android half ...
    patched = text.replace('#[cfg(target_os = "android")]\n', "")
    # ... and make sure the host half cannot collide with it.
    patched = patched.replace(
        '#[cfg(not(target_os = "android"))]', "#[cfg(any())]"
    )
    patched = re.sub(r"^pub mod jni;$", "", patched, flags=re.M)
    if patched != text:
        path.write_text(patched)
PY

echo "== cargo check (Android modules, host target) =="
cargo check --lib 2>&1 | tail -60
echo "== cargo clippy (Android modules, host target) =="
cargo clippy --lib -- -D warnings 2>&1 | tail -60
