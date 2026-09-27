// Standalone Gradle build for the Android SDK. It is deliberately NOT part of a
// larger Gradle tree: the Rust workspace above it is the real project, and this
// build's only job is to package the cross-compiled cdylib plus the generated
// Kotlin into an AAR and publish it.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "meerkly-android-sdk"
include(":lib")
