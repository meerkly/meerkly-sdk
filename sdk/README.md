# SDKs

One Rust exit-node core ([`crates/client-core`](../crates/client-core)) exposed to every platform.
Two binding mechanisms:

- **Node** via **napi-rs** ([`crates/client-napi`](../crates/client-napi))
- **Go / Kotlin / Swift / Python / Ruby** via **uniffi** ([`crates/client-ffi`](../crates/client-ffi))
  — one interface generated to each language.

Every binding presents the same shape: construct a client with `{ publisherId, gatewayAddresses?, … }`,
`start()` (connect + register), `stop()`, and read `connected` / `clientKey` / `state`. The SDK
stores nothing; the gateway assigns an ephemeral per-connection id.

The end-to-end smokes need the gateway, which is closed source, so they run in
the private repo's CI against the revision of this repo it pins.

| Package | Lang | Mechanism | Status |
|---|---|---|---|
| `node/` (`@meerkly/sdk`) | Node/TS | napi | **built + smoke-tested** (end-to-end against a gateway) |
| `python/` | Python | uniffi | **built + smoke-tested** (end-to-end against a gateway) |
| `go/` | Go | uniffi (`uniffi-bindgen-go`) | **built + smoke-tested** (end-to-end against a gateway) |
| `ruby/` | Ruby | uniffi | **built + smoke-tested** (end-to-end against a gateway) — uses blocking `start`/`stop` (uniffi Ruby has no async) |
| `swift/` | Swift | uniffi | generated + type-checked (`swiftc -typecheck`); runtime shares the uniffi cdylib proven by Python/Go — on-device smoke in CI |
| `android/` (`com.meerkly:sdk`) | Kotlin | uniffi (JNA) | **AAR published to Maven Central** (`scripts/build-android-sdk.sh`); loaded on an emulator in CI |

## Building

```bash
# Node addon → sdk/node/index.node
./scripts/build-node-addon.sh

# uniffi cdylib + generate Python/Swift/Kotlin/Go (needs uniffi-bindgen-go, see below)
./scripts/build-ffi-bindings.sh

# Android AAR → sdk/android/lib/build/outputs/aar/ (needs the NDK + cargo-ndk)
./scripts/build-android-sdk.sh
```

`uniffi-bindgen-go` (version-matched to uniffi 0.28.3):

```bash
cargo install uniffi-bindgen-go \
  --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.4.0+v0.28.3
```

Generated binding *sources* are committed; the built native libs (`.node`, `.dylib`/`.so`) are not —
they're produced per target by the build scripts and CI.

## Notes

- **Runtime:** the FFI bindings have no ambient async runtime, so the core takes an explicit tokio
  runtime; each binding owns a dedicated one.
- **Go typed-nil:** `uniffi-bindgen-go` 0.4.0 returns a typed-nil `*ProxyError` as `error` from async
  methods; `build-ffi-bindings.sh` post-patches the generated Go to nil-check it.
- **Android:** an AAR bundling the per-ABI `.so` (built with `cargo-ndk`) + the generated Kotlin,
  loaded via JNA at runtime. `./scripts/build-android-sdk.sh` builds and verifies it; see
  [`sdk/android/README.md`](android/README.md) for the Maven Central setup.
- **Error field naming:** the FFI error carries `reason`, not `message` — uniffi generates the
  Kotlin error as a `kotlin.Exception` subclass with its own `message`, and a Rust field of that
  name yields Kotlin that will not compile.
