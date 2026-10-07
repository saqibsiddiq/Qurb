import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Push is optional, and the build has to work without it.
//
// Waking a sleeping phone needs Firebase, which needs a `google-services.json`
// from a project somebody owns. Without that file the Google plugin fails the
// build outright — so the whole thing is switched on the file's presence, and
// a checkout with no Firebase project builds and runs exactly as before. The
// phone then learns about changes at its next scheduled look rather than the
// moment they happen.
//
// Two source sets rather than a runtime check, because the alternative is
// compiling against an SDK that is not there.
//
// `-Pqurb.idSuffix=.x` installs a second copy beside the real one, under its
// own package -- its own data, its own Block Store -- so that what clearing an
// app's data does can be tried on a phone without clearing the person's
// (2026-10-08). Firebase knows only `com.qurb`, so that copy builds without
// push.
val idSuffix = (findProperty("qurb.idSuffix") as String?).orEmpty()
val firebaseConfigured = file("google-services.json").exists() && idSuffix.isEmpty()
if (firebaseConfigured) {
    apply(plugin = "com.google.gms.google-services")
}

// Release signing, from a properties file kept outside the repository: the
// key that says an update comes from the same author as the installed app,
// which must never be committed and never be lost. Where it is:
// $QURB_SIGNING, or ~/.config/qurb/signing.properties, holding storeFile,
// storePassword, keyAlias and keyPassword. Without it a release builds
// unsigned, exactly as before -- see android/README.md, "Signing".
val signingFile = file(
    System.getenv("QURB_SIGNING") ?: "${System.getProperty("user.home")}/.config/qurb/signing.properties"
)
val signing = Properties().apply {
    if (signingFile.exists()) signingFile.inputStream().use { load(it) }
}

android {
    namespace = "com.qurb"
    compileSdk = 36

    signingConfigs {
        if (signingFile.exists()) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = signing.getProperty("keyPassword")
            }
        }
    }

    defaultConfig {
        applicationId = "com.qurb"
        applicationIdSuffix = idSuffix.ifEmpty { null }
        // 26 matches the API level the native library is built against; see
        // scripts/android-build.sh. It is also where the NDK's 64-bit file APIs
        // are complete, which SQLite needs for files over 2 GB.
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    // Which architectures go in the APK is decided per build type. Only what is
    // actually run: the other two ABIs build, and adding them without ever
    // running them would be a claim this project has not earned.
    buildTypes {
        release {
            // What a phone installs: arm64 only, the code shrunk and optimised
            // by R8, and unused resources dropped. Together, most of the
            // difference between a heavy app and a light one -- see decision
            // 0039 for the measurements.
            ndk {
                abiFilters += listOf("arm64-v8a")
            }
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
        debug {
            // x86_64 as well, for the emulator.
            ndk {
                abiFilters += listOf("arm64-v8a", "x86_64")
            }
            isMinifyEnabled = false
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
        viewBinding = true
    }

    sourceSets["main"].java.srcDir(
        if (firebaseConfigured) "src/push/java" else "src/nopush/java"
    )

    // Nothing here about stripping: scripts/android-app.sh strips the .so with
    // the NDK it already located before copying it into jniLibs, so Gradle
    // packages something that is 3.7 MB rather than 61 MB and does not need an
    // NDK of its own to do it.
}

dependencies {
    if (firebaseConfigured) {
        // Being woken when another device has something. Only the messaging
        // library: qurb uses no other part of Firebase, and does not want to.
        implementation(platform("com.google.firebase:firebase-bom:33.7.0"))
        implementation("com.google.firebase:firebase-messaging")
    }

    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.constraintlayout:constraintlayout:2.1.4")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
    implementation("androidx.swiperefreshlayout:swiperefreshlayout:1.1.0")
    implementation("androidx.coordinatorlayout:coordinatorlayout:1.2.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.activity:activity-ktx:1.9.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // Scanning a pairing code with the camera. CameraX for the preview and
    // frame delivery; the Play Services build of ML Kit for the decoding,
    // because it is a few hundred KB against several MB for the bundled model
    // and this phone has Play Services anyway.
    implementation("androidx.camera:camera-camera2:1.5.3")
    implementation("androidx.camera:camera-lifecycle:1.5.3")
    implementation("androidx.camera:camera-view:1.5.3")
    implementation("com.google.android.gms:play-services-mlkit-barcode-scanning:18.3.1")
    // Keeping the key without anyone writing 24 words down: a few bytes kept
    // by Play services, carried across a reinstall and backed up end to end
    // encrypted with the screen lock (decision 0052).
    implementation("com.google.android.gms:play-services-auth-blockstore:16.4.0")

    // Background sync. WorkManager rather than a bare AlarmManager or a
    // foreground service: it is the only scheduler that survives reboots,
    // respects Doze, and backs off on its own when the system is busy — which
    // is exactly the negotiation a sync app has to win to keep running at all.
    implementation("androidx.work:work-runtime-ktx:2.9.1")

    // UniFFI's Kotlin bindings call the native library through JNA. The `@aar`
    // classifier matters: the plain jar has no Android native components and
    // fails at runtime rather than at build time.
    implementation("net.java.dev.jna:jna:5.15.0@aar")
}
