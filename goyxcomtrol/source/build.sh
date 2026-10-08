#!/bin/bash
set -euo pipefail
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
BT=$SDK/build-tools/34.0.0
PLAT=$SDK/platforms/android-34
APP=$(cd "$(dirname "$0")" && pwd)
LIBS=$APP/android/libs
OUT=$APP/build
rm -rf "$OUT"
mkdir -p "$OUT/gen" "$OUT/classes" "$OUT/flat" "$LIBS"

: "${JAVAC:=${JAVA_HOME:-}/bin/javac}"

"$BT/aapt2" compile --dir "$APP/res" -o "$OUT/flat/res.zip"
"$BT/aapt2" link -o "$OUT/base.apk" -I "$PLAT/android.jar" \
  --manifest "$APP/AndroidManifest.xml" -A "$APP/assets" \
  --java "$OUT/gen" --min-sdk-version 24 --target-sdk-version 34 \
  "$OUT/flat/res.zip"

"$BT/aidl" -p"$PLAT/framework.aidl" -I"$APP/src" -o"$OUT/gen" "$APP/src/com/xtg/pad/IVPad.aidl"

CP="$PLAT/android.jar:$LIBS/api.jar:$LIBS/provider.jar:$LIBS/aidl.jar"
find "$APP/src" -name '*.java' > "$OUT/srcs.txt"
find "$OUT/gen" -name '*.java' >> "$OUT/srcs.txt"
"$JAVAC" -nowarn -encoding UTF-8 -cp "$CP" -d "$OUT/classes" @"$OUT/srcs.txt"

"$BT/d8" --min-api 24 --lib "$PLAT/android.jar" --output "$OUT" \
  $(find "$OUT/classes" -name '*.class') "$LIBS/api.jar" "$LIBS/provider.jar" "$LIBS/aidl.jar"

cd "$OUT"
python3 - <<'PY'
import shutil, zipfile
shutil.copy('base.apk', 'unsigned.apk')
with zipfile.ZipFile('unsigned.apk', 'a', zipfile.ZIP_DEFLATED) as z:
    z.write('classes.dex', 'classes.dex')
PY

# Permanent SpaceCell signing identity is supplied to CI.
KS=$APP/signing/spacecell-release.jks
STOREPASS=${SC_SIGN_STORE:?SC_SIGN_STORE is required}
KEYALIAS=${SC_SIGN_ALIAS:-spacecell-release}
test -f "$KS" || { echo "missing SpaceCell release keystore: $KS" >&2; exit 1; }

"$BT/zipalign" -f 4 unsigned.apk aligned.apk
"$BT/apksigner" sign --ks "$KS" --ks-key-alias "$KEYALIAS" \
  --ks-pass pass:"$STOREPASS" --key-pass pass:"$STOREPASS" \
  --min-sdk-version 24 --out "$APP/xtg-cloud-pad-1.6.0.apk" aligned.apk
"$BT/apksigner" verify --verbose --print-certs "$APP/xtg-cloud-pad-1.6.0.apk"
printf 'APK: %s\n' "$APP/xtg-cloud-pad-1.6.0.apk"
