// The same Android plugin as android/, so the build needs nothing new. Java
// and no libraries: the spike is one screen calling the platform directly.
plugins {
    id("com.android.application") version "8.11.0"
}

android {
    namespace = "app.qurb.accessspeed"
    compileSdk = 36
    defaultConfig {
        applicationId = "app.qurb.accessspeed"
        minSdk = 30
        targetSdk = 36
        versionCode = 1
        versionName = "spike"
    }
}
