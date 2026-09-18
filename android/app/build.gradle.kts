plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.qurb"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.qurb"
        // 26 matches the API level the native library is built against; see
        // scripts/android-build.sh. It is also where the NDK's 64-bit file APIs
        // are complete, which SQLite needs for files over 2 GB.
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            // Only what is actually shipped here. The other two ABIs build, and
            // adding them to the APK without ever running them would be a claim
            // this project has not earned.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
        debug {
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

    // Nothing here about stripping: scripts/android-app.sh strips the .so with
    // the NDK it already located before copying it into jniLibs, so Gradle
    // packages something that is 3.7 MB rather than 61 MB and does not need an
    // NDK of its own to do it.
}

dependencies {
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
