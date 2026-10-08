# Goy Input Bridge

Android Shizuku-powered relative mouse/keyboard bridge and custom control editor.

The buildable source snapshot is stored in \`source.tar.gz\` so it can be pulled into a Vercel Sandbox or CI runner without requiring a local checkout.

Build from the archive root:

    tar -xzf source.tar.gz
    cd inputbridge
    ./gradlew assembleDebug

CI uses the runner's Android SDK command-line tools directly.

CI SDK check updated.
