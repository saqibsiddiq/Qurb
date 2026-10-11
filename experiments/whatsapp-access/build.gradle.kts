// The same Android plugin as android/, so the build needs nothing new. Java
// and no libraries: the spike is one screen calling the platform directly.
plugins {
    id("com.android.application") version "8.11.0"
}

android {
    namespace = "app.qurb.waspike"
    compileSdk = 36
    defaultConfig {
        applicationId = "app.qurb.waspike"
        minSdk = 30
        targetSdk = 36
        versionCode = 1
        versionName = "spike"
    }
}
