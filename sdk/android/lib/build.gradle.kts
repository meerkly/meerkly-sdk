import com.vanniktech.maven.publish.AndroidSingleVariantLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SourcesJar

plugins {
    id("com.android.library")
    id("com.vanniktech.maven.publish")
}

/**
 * The published version is the Cargo workspace version — the same number every
 * other SDK is released at (see .github/workflows/release-sdks.yml). Reading it
 * here rather than stamping it in from CI means a local `publishToMavenLocal`
 * produces the same coordinates a release would, and there is no second place
 * to forget to bump.
 *
 * `-PmeerklyVersion=…` overrides it, which is only for trying a build without
 * touching Cargo.toml.
 */
fun workspaceVersion(): String {
    providers.gradleProperty("meerklyVersion").orNull
        ?.takeIf { it.isNotBlank() }
        ?.let { return it }

    val cargoToml = rootProject.file("../../Cargo.toml")
    require(cargoToml.isFile) {
        "cannot find ${cargoToml.path} — this Gradle build must stay at sdk/android " +
            "inside the Rust workspace, since it reads the release version from it"
    }
    var inWorkspacePackage = false
    for (raw in cargoToml.readLines()) {
        val line = raw.trim()
        if (line.startsWith("[")) {
            inWorkspacePackage = line == "[workspace.package]"
            continue
        }
        if (inWorkspacePackage) {
            Regex("""^version\s*=\s*"([^"]+)"""").find(line)?.let { return it.groupValues[1] }
        }
    }
    error("no [workspace.package] version found in ${cargoToml.path}")
}

group = "com.meerkly"
version = workspaceVersion()

// So the local build output reads meerkly-sdk-release.aar rather than lib-release.aar.
base {
    archivesName.set("meerkly-sdk")
}

android {
    namespace = "com.meerkly.sdk"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        minSdk = 24
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // Package exactly the three ABIs scripts/build-android-sdk.sh cross-compiles.
        // Without this a stale .so left in jniLibs for some other ABI would be
        // packaged and shipped, and the AAR content check would still pass.
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
    }

    // src/main/kotlin, src/main/jniLibs and src/androidTest/kotlin are AGP
    // defaults, so nothing needs declaring here. The native libraries are
    // cross-compiled into src/main/jniLibs by scripts/build-android-sdk.sh
    // before Gradle runs.

    // The .so files are already stripped and optimised by cargo's release
    // profile; letting Gradle recompress them only slows loading.
    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }

    // Instrumented tests run against the release variant — the one that becomes
    // the published AAR. Against the default (debug) they would prove that
    // *a* build of the native library loads, not that the shipped one does.
    testBuildType = "release"

    buildTypes {
        named("release") {
            // The generated uniffi bindings are the whole public API. Shrinking
            // a library here would only fight the consumer's own R8 run.
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    // Both are `api`, not `implementation`: the generated bindings expose JNA
    // types across the FFI boundary and suspend functions to the caller, so a
    // consumer resolving them transitively is correct rather than accidental.
    //
    // JNA must be the @aar artifact — the plain jar carries no Android native
    // dispatch libraries, and the SDK would fail to load at runtime.
    api("net.java.dev.jna:jna:5.19.1@aar")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")

    // The generated bindings annotate their API-34 cleaner path with
    // @RequiresApi. It is a compile-time annotation on internal classes, so it
    // stays `implementation` and out of the published POM's compile scope.
    implementation("androidx.annotation:annotation:1.10.0")

    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.11.0")
}

mavenPublishing {
    // automaticRelease: the deployment is released as soon as the Central Portal
    // finishes validating it, rather than waiting in the portal for someone to
    // press Publish. That matches how npm and crates.io are released from this
    // same workflow — a tag is the decision to release, and a half-released set
    // of SDKs is the thing worth avoiding.
    publishToMavenCentral(automaticRelease = true)

    // Sign only when a key is actually available. CI supplies one as
    // ORG_GRADLE_PROJECT_signingInMemoryKey; a developer machine has none, and
    // signAllPublications() fails the build outright rather than skipping — which
    // would make `publishToMavenLocal` impossible without a GPG key just to look
    // at the POM.
    //
    // This cannot let an unsigned release slip out. The release workflow's
    // preflight refuses to start without the signing secrets, and the Central
    // Portal rejects an unsigned deployment regardless, so the failure mode is a
    // loud rejection rather than a bad artifact.
    if (providers.gradleProperty("signingInMemoryKey").isPresent) {
        signAllPublications()
    } else {
        logger.lifecycle("no signing key configured — publishing unsigned (fine for mavenLocal, rejected by Maven Central)")
    }

    configure(
        AndroidSingleVariantLibrary(
            javadocJar = JavadocJar.Empty(),
            sourcesJar = SourcesJar.Sources(),
            variant = "release",
        ),
    )

    coordinates("com.meerkly", "sdk", version.toString())

    pom {
        name.set("Meerkly SDK")
        description.set(
            "Meerkly proxy network SDK — turn an Android app into a bandwidth-sharing exit node.",
        )
        url.set("https://github.com/meerkly/meerkly-sdk/tree/main/sdk/android")
        inceptionYear.set("2026")

        licenses {
            license {
                name.set("MIT License")
                url.set("https://opensource.org/licenses/MIT")
                distribution.set("repo")
            }
            license {
                name.set("The Apache License, Version 2.0")
                url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("meerkly")
                name.set("Meerkly")
                url.set("https://meerkly.com")
            }
        }
        scm {
            url.set("https://github.com/meerkly/meerkly-sdk")
            connection.set("scm:git:git://github.com/meerkly/meerkly-sdk.git")
            developerConnection.set("scm:git:ssh://git@github.com/meerkly/meerkly-sdk.git")
        }
    }
}
