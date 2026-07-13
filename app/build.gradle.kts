plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.boardbridge.egl"
    compileSdk = 35

    // NDK r27c: defaults to 16 KB page-aligned shared libraries, required for
    // Android 15 (API 35) on 64-bit devices such as Mali-G52 / Helio G85.
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "com.boardbridge.egl"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"

        ndk {
            // arm64-v8a is the primary target (Mali-G52 / Helio G85).
            // armeabi-v7a for older 32-bit devices; x86_64 for emulators.
            abiFilters.addAll(listOf("arm64-v8a", "armeabi-v7a", "x86_64"))
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        buildConfig = false
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.ktx)
}
