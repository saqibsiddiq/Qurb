plugins {
    id("com.android.application") version "8.11.0" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    // Applied by app/build.gradle.kts only when a google-services.json exists,
    // so a checkout with no Firebase project still builds.
    id("com.google.gms.google-services") version "4.4.2" apply false
}
