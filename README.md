# meerkly-sdk

The Meerkly exit-node SDK. Embed it in an app or service and that host shares
its connection with the [Meerkly](https://meerkly.com) network: it holds one
QUIC connection to a gateway, and proxy traffic leaves through its IP. Earners
are paid per GB, metered on the gateway.

An exit node identifies itself only by a **publisher id** (`pub_…`) from
[dashboard.meerkly.com](https://dashboard.meerkly.com). It is a public
identifier, not a secret.

## Packages

| Package | Registry | Source |
|---|---|---|
| `meerkly-sdk` (Rust) | crates.io | [`crates/client-core`](crates/client-core) |
| `meerkly-protocol` (wire types) | crates.io | [`crates/protocol`](crates/protocol) |
| `@meerkly/sdk` (Node) | npm | [`sdk/node`](sdk/node), napi over [`crates/client-napi`](crates/client-napi) |
| `com.meerkly:sdk` (Android) | Maven Central | [`sdk/android`](sdk/android), uniffi over [`crates/client-ffi`](crates/client-ffi) |
| `@meerkly/expo` (Expo / React Native) | npm | [`sdk/expo`](sdk/expo) |
| Go, Python, Ruby, Swift | not published yet | [`sdk/`](sdk), uniffi over [`crates/client-ffi`](crates/client-ffi) |

Minimal per-language snippets are in [`examples/`](examples). Build
instructions and per-binding notes are in [`sdk/README.md`](sdk/README.md).

## Releasing

Every SDK releases at one synced version: `[workspace.package] version` in
`Cargo.toml`. `sdk/expo/package.json` must match it.

1. Bump both, commit, and push to `main`.
2. Push the tag `v<X.Y.Z>`.
3. Approve the `release` environment when `release-sdks` asks, once before the
   builds and once before publishing.

See the comments at the top of
[`.github/workflows/release-sdks.yml`](.github/workflows/release-sdks.yml) for
why every build must pass before anything is uploaded.

## License

MIT OR Apache-2.0, at your option.
