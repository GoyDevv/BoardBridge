# Keep classes that declare native methods and the methods themselves.
-keepclasseswithmembernames class * {
    native <methods>;
}

# The JNI entry-point class is referenced by name from C++.
-keep class com.boardbridge.egl.NativeBridge { *; }
