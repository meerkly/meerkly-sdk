# examples

Minimal "import the SDK and start sharing bandwidth in the background" snippets,
one per language.

The shape is the same everywhere: construct a client with your **API key**, call
`start()` (connect + register), and keep the client alive. After `start()` returns
the exit node already runs on the core's background threads — your app just carries
on with its own work.

### Config

In production a host provides only the API key (the gateway endpoint and CA are
baked in). Until that lands, the examples also pass a dev **gateway address** and
**CA path** — the two `// dev override` lines. Point them at a running gateway
and the CA that signed its certificate.

### Building the SDK first

- Node: `./scripts/build-node-addon.sh` (produces `sdk/node/index.node`)
- Go / Python / Swift / Kotlin / Ruby: `./scripts/build-ffi-bindings.sh`

Ruby needs the `ffi` gem and the native lib on its path, e.g.
`RUBYLIB=sdk/ruby DYLD_LIBRARY_PATH=sdk/ruby ruby examples/ruby/start.rb`.

Per-language packaging notes (module paths, cgo flags, SwiftPM/Gradle) live in
[`../sdk/README.md`](../sdk/README.md).
