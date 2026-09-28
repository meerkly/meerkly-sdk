# @meerkly/sdk

Turn a Node.js or Electron app into a **Meerkly proxy exit node**. Your app opens
one connection to the Meerkly gateway and shares this machine's internet
connection; traffic is routed out through it and the associated account earns per
GB. The SDK stores nothing on disk and runs entirely in the background.

> The `publisherId` is a **public** identifier (safe to ship in an app or show in
> a QR code) — it attributes shared bandwidth to an account. It is not a secret
> and grants no dashboard access.

## Install

```bash
npm install @meerkly/sdk
```

Prebuilt native binaries are published for macOS (x64/arm64), Linux (x64/arm64,
glibc), and Windows (x64) — npm installs the one matching the host automatically.
Node.js **≥ 20** required.

## Quick start

```js
import { ProxyClient } from "@meerkly/sdk";
// CommonJS: const { ProxyClient } = require("@meerkly/sdk");

const client = new ProxyClient({
  publisherId: "pub_xxxxxxxxxxxxxxxxxxxxxxxx", // from your Meerkly dashboard
});

await client.start();               // connects + registers; resolves once online
console.log("online:", client.clientKey, "via", client.gatewayId);

// ... share bandwidth for as long as the app runs ...

await client.stop();                // disconnect and stop reconnecting
```

That's the whole integration. A background supervisor keeps the connection up
(reconnecting on network changes) between `start()` and `stop()`.

### Electron

Create the client in the **main process** (not the renderer) and stop it on quit:

```js
const { app } = require("electron");
const { ProxyClient } = require("@meerkly/sdk");

const client = new ProxyClient({ publisherId: process.env.MEERKLY_PUBLISHER_ID });
app.whenReady().then(() => client.start().catch(console.error));
app.on("before-quit", (e) => { e.preventDefault(); client.stop().finally(() => app.exit()); });
```

## API

### `new ProxyClient(options)`

| option | type | default | notes |
|---|---|---|---|
| `publisherId` | `string` | — | **required.** Your public publisher id. |
| `gatewayAddresses` | `string[]` | production gateway | Override the `host:port` list (dev/testing). |
| `startTimeoutMs` | `number` | `30000` | How long `start()` waits for the first connection. |
| `connectTimeoutMs` | `number` | `15000` | Per-target dial timeout on the exit. |
| `caCertPath` / `caCertPem` | `string` / `Uint8Array` | — | Pin a self-signed gateway CA (dev only). Omit in production — the gateway's public certificate is verified against the system roots. |

### Methods & properties

- `start(): Promise<void>` — connect and register; resolves once online, rejects on timeout.
- `stop(): Promise<void>` — close the connection and stop reconnecting.
- `connected: boolean` — whether a gateway connection is currently held.
- `state: string` — one of `idle`, `connecting`, `connected`, `stopped`.
- `gatewayId: string | null` — the serving gateway, or `null` when disconnected.
- `clientKey: string | null` — the gateway-assigned `account:instance-id`.
- `lastRejection: string | null` — why the gateway last refused this client
  (e.g. `another device is already connected from this IP address`), or `null`
  if it has not since the last successful connection. Written to be shown to a
  person.

## How it works

The package is a native addon (Rust core via [napi-rs](https://napi.rs)). `start()`
opens a QUIC connection to the gateway, authenticates with the public
`publisherId`, and registers this process as an exit. The gateway then routes
proxy requests over that connection, and the exit makes the outbound connections
from **this machine's IP**. No inbound ports are opened, so it works behind NAT.

## License

MIT.
