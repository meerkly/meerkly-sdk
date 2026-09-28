# meerkly-sdk

The Rust SDK for the [meerkly](https://meerkly.com) residential/mobile proxy network.

Embed it and this machine becomes an **exit node**: it holds one QUIC connection to a meerkly
gateway and dials outbound TCP on the gateway's behalf, so traffic the network routes through
you leaves via your own connection. Bandwidth you share is metered per GB and earns the account
behind your publisher id.

This crate is the same core every other meerkly SDK wraps — `@meerkly/sdk` on npm, and the
Kotlin, Swift, Python, Ruby and Go bindings are all generated from it.

## Usage

```toml
[dependencies]
meerkly-sdk = "0.3"
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use meerkly_sdk::{ClientConfig, ProxyClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ProxyClient::new(
        ClientConfig::new("pub_your_publisher_id"),
        tokio::runtime::Handle::current(),
    )?;

    client.start().await?;
    println!("online via {:?}", client.gateway_id());

    tokio::signal::ctrl_c().await?;
    client.stop().await?;
    Ok(())
}
```

`ClientConfig::new` targets the production gateway (`gw.meerkly.com:4443`) and verifies its
certificate against the public root CAs, so nothing has to ship alongside your binary. Every
field is public — point `gateway_addresses` somewhere else, or set `ca_cert` to pin a
self-signed development CA, by assigning to it.

## Getting a publisher id

Create one at [dashboard.meerkly.com](https://dashboard.meerkly.com). It looks like
`pub_` followed by 24 alphanumeric characters.

A publisher id is a **public identifier, not a secret**. It ships inside third-party apps and is
handed out as a QR code; it only says which account earns the bandwidth shared through it, and
grants no access to anything. Committing one to a repository is not a credential leak.

## What it does and does not do

- **Stores nothing.** No config file, no cache, no id of its own. A host that keeps a device id
  may pass one in (`ClientConfig::device_id`) so the dashboard can show a stable, nameable row per
  machine; the SDK carries it but never mints or persists it. The gateway still assigns a fresh
  ephemeral instance id on every connection, and earnings accrue to the account, not to a device.
- **Reconnects on its own.** One QUIC connection at a time, gateway addresses tried in rotation,
  exponential backoff. `start()` resolves once registered; `stop()` closes cleanly and is bounded
  so it can never hang a shutdown path.
- **Refuses to be an SSRF vector.** Every outbound dial resolves the host itself and rejects
  loopback, private, link-local, CGNAT and unique-local targets, then connects to the address it
  validated — so DNS rebinding cannot slip past the check.
- **Owns no async runtime.** You pass a `tokio::runtime::Handle`; the QUIC endpoint and every
  connection task run on it.

Built for hosts that cannot run in the background: `start()` and `stop()` are both fast, and no
part of the design assumes a session outlives a few seconds.

## Logging

Standard [`tracing`](https://docs.rs/tracing) — install any subscriber to see it. The meerkly
tools read the filter from `MEERKLY_LOG`:

```rust,no_run
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_env("MEERKLY_LOG")
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    )
    .init();
```

## Just want to run a node?

Install the agent instead — it is this crate packaged as a background service, with a config
file and `systemctl`/`brew services` integration:

```bash
cargo install meerkly    # or: brew install meerkly/tap/meerkly
```

## Licence

MIT
