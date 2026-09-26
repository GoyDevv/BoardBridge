import org.gradle.api.tasks.PathSensitivity
// Imported explicitly: inside a Gradle script `java` resolves to the Java
// plugin's extension, so a fully qualified `java.util.Properties()` does not
// compile ("Unresolved reference: util").
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

// ---------------------------------------------------------------------------
// Rust core: boardbridge/ -> app/build/rustJniLibs/<abi>/libboardbridge.so
// ---------------------------------------------------------------------------
//
// Usage:
//   ./gradlew :app:assembleDebug                      # cargo ndk, release profile
//   ./gradlew :app:assembleDebug -PcargoProfile=debug  # faster, unoptimized
//   ./gradlew :app:assembleDebug -PskipRust            # Kotlin-only iteration
//
// Requirements (both documented in docs/ANDROID.md):
//   * cargo + cargo-ndk in PATH          (`cargo install cargo-ndk --locked`)
//   * an NDK at $ANDROID_NDK_HOME, or the one AGP resolved for android.ndkVersion

/** Profile passed to `cargo build`; `release` matches CI and the docs. */
val cargoProfile: String = (project.findProperty("cargoProfile") as String?) ?: "release"

/** Set `-PskipRust` to build the Kotlin shell against an existing .so. */
val skipRust: Boolean = (project.findProperty("skipRust") as String?)?.toBoolean() ?: false

/** ABIs packaged by the APK; must stay in sync with `defaultConfig.ndk.abiFilters`. */
val cargoAbis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")

/** Where cargo-ndk drops its output; consumed by `jniLibs.srcDir` below. */
val cargoRustJniLibsDir: File = layout.buildDirectory.dir("rustJniLibs").get().asFile

android {
    // The Kotlin shell lives in com.boardbridge.bridge, the same package the
    // exported Rust symbols are named after (Java_com_boardbridge_bridge_*).
    namespace = "com.boardbridge.bridge"
    compileSdk = 35

    // NDK r27c: defaults to 16 KB page-aligned shared libraries, required for
    // Android 15 (API 35) on 64-bit devices. cargo-ndk is pointed at the same
    // NDK; see the `cargoBuildBoardBridge` task below.
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "com.boardbridge.bridge"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.2.0"

        ndk {
            // arm64-v8a is the primary target (Mali-G52 / Helio G85 class).
            // armeabi-v7a for older 32-bit devices; x86_64 for emulators.
            // Every ABI listed here is built by `cargo ndk` below.
            abiFilters.addAll(cargoAbis)
        }
    }

    // There is no C/C++ in the app: libboardbridge.so is a Rust cdylib built by
    // the `cargoBuildBoardBridge` task and picked up from build/rustJniLibs.
    // No externalNativeBuild/CMake section belongs here any more.
    sourceSets.getByName("main") {
        jniLibs.srcDir(cargoRustJniLibsDir)
    }

    packaging {
        jniLibs {
            // Page-aligned, uncompressed .so files: required for Android 15's
            // 16 KB pages (the Rust link line sets max-page-size=16384 as well).
            useLegacyPackaging = false
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

/**
 * Finds the NDK for cargo-ndk.
 *
 * `ANDROID_NDK_HOME`/`ANDROID_NDK_ROOT` win, because that is what a developer
 * who installed an NDK by hand expects; otherwise the NDK that AGP resolved for
 * `android.ndkVersion` under the SDK in `local.properties` is used, so the
 * Gradle build works with no extra environment at all.
 */
fun resolveNdkHome(): File? {
    for (variable in listOf("ANDROID_NDK_HOME", "ANDROID_NDK_ROOT")) {
        val value = System.getenv(variable)
        if (!value.isNullOrBlank()) return file(value)
    }
    val sdkRoot = System.getenv("ANDROID_SDK_ROOT")?.takeIf { it.isNotBlank() }
        ?: System.getenv("ANDROID_HOME")?.takeIf { it.isNotBlank() }
        ?: rootProject.file("local.properties").takeIf { it.isFile }?.let { properties ->
            Properties()
                .apply { properties.inputStream().use { load(it) } }
                .getProperty("sdk.dir")
        }
    val ndkVersion = android.ndkVersion
    if (sdkRoot.isNullOrBlank() || ndkVersion.isNullOrBlank()) return null
    return file("$sdkRoot/ndk/$ndkVersion")
}

val ndkHome: File? = resolveNdkHome()

val cargoBuildBoardBridge = tasks.register<Exec>("cargoBuildBoardBridge") {
    group = "boardbridge"
    description = "Cross-compiles the Rust bridge (boardbridge/) into build/rustJniLibs with cargo-ndk."

    workingDir = rootProject.file("boardbridge")
    inputs.dir(rootProject.file("boardbridge/src"))
        .withPathSensitivity(PathSensitivity.RELATIVE)
    inputs.file(rootProject.file("boardbridge/Cargo.toml"))
    inputs.property("profile", cargoProfile)
    outputs.dir(cargoRustJniLibsDir)

    val arguments = mutableListOf(
        "cargo", "ndk",
        "--platform", "26", // minSdk; selects the NDK's clang wrapper
    )
    cargoAbis.forEach { abi ->
        arguments += listOf("-t", abi)
    }
    arguments += listOf("-o", cargoRustJniLibsDir.absolutePath, "build")
    if (cargoProfile == "release") {
        arguments += "--release"
    }
    commandLine(arguments)

    // 16 KB page alignment (Android 15): explicit so it does not depend on the
    // NDK's default. `cargo ndk` supplies the linker through CARGO_TARGET_* env
    // vars, so RUSTFLAGS is free for this.
    environment("RUSTFLAGS", "-C link-arg=-Wl,-z,max-page-size=16384")
    ndkHome?.let { environment("ANDROID_NDK_HOME", it.absolutePath) }

    doFirst {
        val ndk = ndkHome
        if (ndk == null || !ndk.isDirectory) {
            throw GradleException(
                "No Android NDK found for the Rust build. Set ANDROID_NDK_HOME (or " +
                    "sdk.dir in local.properties) and run `cargo install cargo-ndk --locked`; " +
                    "see docs/ANDROID.md. To build the Kotlin shell only, pass -PskipRust.",
            )
        }
    }
}

if (skipRust) {
    logger.lifecycle("skipRust is set: libboardbridge.so is not rebuilt from boardbridge/")
} else {
    // The .so must exist before AGP merges native libraries into the APK.
    // `configure {}` (rather than `tasks.named("preBuild") {}`) keeps this
    // branch `Unit`-typed, which is what the Kotlin DSL expects from an
    // if/else statement and what silences the "implicitly cast to Any" warning.
    tasks.named("preBuild").configure {
        dependsOn(cargoBuildBoardBridge)
    }
}
