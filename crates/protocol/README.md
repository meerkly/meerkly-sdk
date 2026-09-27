# meerkly-protocol

Wire types and framing for the [meerkly](https://meerkly.com) proxy network — the messages a
gateway and an exit node exchange, and the length-prefixed JSON framing that carries them.

This crate exists so the gateway and the client can share one definition of the protocol. If you
are embedding an exit node, you want **[`meerkly-sdk`](https://crates.io/crates/meerkly-sdk)**
instead; it depends on this and you will not need to name it yourself.

The protocol is versioned by its ALPN identifier, `meerkly/1`.

## Licence

MIT OR Apache-2.0
