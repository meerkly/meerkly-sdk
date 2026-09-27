plugins {
    // AGP 9 has built-in Kotlin support; a separate Kotlin plugin is no longer
    // applied (and is rejected if it is).
    id("com.android.library") version "9.4.0" apply false
    id("com.vanniktech.maven.publish") version "0.37.0" apply false
}
