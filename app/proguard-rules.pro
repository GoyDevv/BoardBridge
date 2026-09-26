# Keep classes that declare native methods, and the methods themselves.
-keepclasseswithmembernames class * {
    native <methods>;
}

# The JNI entry points are found by symbol name derived from this class's
# package and object name (Java_com_boardbridge_bridge_NativeBridge_*), so
# renaming or stripping it would break every call into libboardbridge.so.
-keep class com.boardbridge.bridge.NativeBridge { *; }

# Only meaningful if minification is ever enabled; the debug and release builds
# currently ship with it off (app/build.gradle.kts).
