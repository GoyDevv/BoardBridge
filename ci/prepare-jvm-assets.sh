#!/usr/bin/env bash
#
# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
#
# Prepares JVM assets for the debug build (run in CI, NOT committed to git):
#   - downloads the Android OpenJDK 21 runtime (OpenJDK, GPLv2+Classpath-Exception)
#     from the FoldCraftLauncher asset mirror,
#   - repackages universal + per-ABI into a single symlink-free zip per ABI
#     (java.util.zip friendly) under app/src/main/assets/jre21/,
#   - compiles jvm-test/HelloJvm.java into app/src/main/assets/jvmtest/hello.jar.
#
# The zips/jar are placed in assets (gitignored) so the APK bundles them.
set -euo pipefail

# zip/unzip are used to repackage the runtime; install if missing (CI safety).
command -v zip >/dev/null 2>&1 || { sudo apt-get update -qq && sudo apt-get install -y -qq zip unzip; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/app/src/main/assets"
BASE="https://raw.githubusercontent.com/ShirosakiMio/FoldCraftLauncher/main/FCL/src/main/assets/app_runtime/java/jre21"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$ASSETS/jre21" "$ASSETS/jvmtest"

echo "==> Downloading JRE 21 universal image"
curl -fsSL "$BASE/universal.tar.xz" -o "$WORK/universal.tar.xz"

# x86_64 for the CI emulator; arm64 for physical devices (Mali-G52).
for arch in x86_64 arm64; do
  echo "==> Preparing JRE for $arch"
  curl -fsSL "$BASE/bin-$arch.tar.xz" -o "$WORK/bin-$arch.tar.xz"
  rm -rf "$WORK/jre-$arch"
  mkdir -p "$WORK/jre-$arch"
  tar -xJf "$WORK/universal.tar.xz" -C "$WORK/jre-$arch"
  tar -xJf "$WORK/bin-$arch.tar.xz" -C "$WORK/jre-$arch"
  # -r recursive, no -y so symlinks are dereferenced (java.util.zip friendly)
  ( cd "$WORK/jre-$arch" && zip -qr "$ASSETS/jre21/jre-$arch.zip" . )
  echo "    $ASSETS/jre21/jre-$arch.zip ($(du -h "$ASSETS/jre21/jre-$arch.zip" | cut -f1))"
  # sanity: libjvm.so must be present
  unzip -l "$ASSETS/jre21/jre-$arch.zip" | grep -q "lib/server/libjvm.so" \
    || { echo "ERROR: libjvm.so missing for $arch"; exit 1; }
done

echo "==> Compiling HelloJvm.java (release 11)"
rm -rf "$WORK/cls"; mkdir -p "$WORK/cls"
javac --release 11 -d "$WORK/cls" "$ROOT/jvm-test/HelloJvm.java"
( cd "$WORK/cls" && jar cf "$ASSETS/jvmtest/hello.jar" . )

echo "==> Done."
ls -la "$ASSETS/jre21" "$ASSETS/jvmtest"
