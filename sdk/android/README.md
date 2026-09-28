# Meerkly Android SDK

`com.meerkly:sdk` — the Rust exit-node core ([`crates/client-core`](../../crates/client-core))
wrapped in Kotlin through [uniffi](../../crates/client-ffi), packaged as an AAR with a
native library for every shipped ABI.

## Using it

```kotlin
dependencies {
    implementation("com.meerkly:sdk:0.6.0")
}
```

```kotlin
import com.meerkly.sdk.ProxyClient
import com.meerkly.sdk.ProxyConfig

val client = ProxyClient(
    ProxyConfig(
        publisherId = "pub_…",          // from the Meerkly dashboard
        gatewayAddresses = emptyList(),  // empty → gw.meerkly.com:4443
        deviceId = yourStableDeviceId,   // optional; the SDK never mints or stores one
        deviceName = Build.MODEL,
        app = "my-app/2.1.0",
    ),
)

client.start()   // suspend: connects and registers, throws ProxyException on failure
// … client.connected(), client.state(), client.clientKey() …
// client.lastRejection(): why the gateway last refused this device, or null —
// e.g. "another device is already connected from this IP address". Show it.
client.stop()
client.destroy() // releases the native handle
```

The host app needs `android.permission.INTERNET`. The AAR declares no permissions
of its own, so nothing is added to your merged manifest.

Exit-node work is continuous and outlives an Activity, so run the client from a
foreground service rather than a screen.

| | |
|---|---|
| Coordinates | `com.meerkly:sdk` |
| Version | the Cargo workspace version — one number for every Meerkly SDK |
| minSdk | 24 |
| ABIs | `arm64-v8a`, `armeabi-v7a`, `x86_64` |
| Transitive deps | JNA (`@aar`), kotlinx-coroutines-core |
| License | MIT |

## Building it locally

One command does everything — cross-compile, regenerate the Kotlin, assemble,
and verify the AAR's contents:

```bash
./scripts/build-android-sdk.sh
```

It needs, once:

```bash
sdkmanager --install "ndk;28.2.13676358"
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
```

The result is `lib/build/outputs/aar/meerkly-sdk-release.aar`. To try it from
another project, install it into your local Maven repository — `ANDROID_HOME`
must be set, which the build script does for itself but a bare Gradle
invocation does not:

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
cd sdk/android && ./gradlew publishToMavenLocal
```

then add `mavenLocal()` to the consuming project's repositories.

Notes on the layout:

- The generated Kotlin (`lib/src/main/kotlin/com/meerkly/sdk/meerkly.kt`) is
  committed, like every other binding in this repo. The native libraries under
  `lib/src/main/jniLibs/` are not — they are built per target.
- The published version is read out of the workspace `Cargo.toml` at
  configuration time, so there is no second place to bump. `-PmeerklyVersion=…`
  overrides it for a throwaway build.
- The error field is `reason`, not `message`. uniffi generates the Kotlin error
  as a `kotlin.Exception` subclass with an `override val message`, so a Rust
  field named `message` produces Kotlin that does not compile. Callers still
  read the text through `e.message` as usual.

## Releasing

Android is part of the one synced release in
[`.github/workflows/release-sdks.yml`](../../.github/workflows/release-sdks.yml):
every SDK goes out at the workspace version, and nothing is uploaded until every
build has succeeded. `android-build` assembles and verifies the AAR;
`android-publish` signs it and uploads it to the Central Portal, skipping the
upload if that version is already published so a partially failed release can be
re-run.

### One-time setup

This has to be done once by whoever owns the Meerkly identity, and cannot be
automated — it involves an account, a DNS record for a domain, and a private key.

1. **Central Portal account and namespace.** Sign up at
   [central.sonatype.com](https://central.sonatype.com), then register the
   namespace `com.meerkly`. The portal issues a TXT record value; add it to the
   DNS for `meerkly.com` and verify. Namespace verification is what proves the
   group id is yours.
2. **Publishing token.** In the portal, generate a user token. It gives a
   username and a password.
3. **GPG signing key.** Maven Central requires every artifact to be signed.
   Give it a passphrase — the workflow expects one.

   ```bash
   gpg --full-generate-key                    # RSA 4096, no expiry, "Meerkly <you@meerkly.com>"
   gpg --list-secret-keys --keyid-format=long # note the long key id after "sec   rsa4096/"
   gpg --keyserver keys.openpgp.org --send-keys <KEY_ID>
   ```

   The public key must reach a keyserver or Central's validation fails.
4. **Repository secrets.** Run each on its own and paste the value at the
   prompt, so nothing lands in shell history.

   ```bash
   gh secret set MAVEN_CENTRAL_USERNAME      --repo meerkly/meerkly-sdk --env release
   gh secret set MAVEN_CENTRAL_PASSWORD      --repo meerkly/meerkly-sdk --env release
   gh secret set MAVEN_SIGNING_KEY_PASSWORD  --repo meerkly/meerkly-sdk --env release
   ```

   The private key is piped straight from gpg into the secret rather than
   printed, so the key text never appears on screen or in a scrollback buffer:

   ```bash
   gpg --armor --export-secret-keys <KEY_ID> \
     | gh secret set MAVEN_SIGNING_KEY --repo meerkly/meerkly-sdk --env release
   ```

Until all four exist, the release workflow's `preflight` job fails immediately
and names the missing ones, so nothing is ever published half-way.
