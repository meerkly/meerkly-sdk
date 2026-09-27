//! Wire protocol shared by the meerkly gateway and exit-node client.
//!
//! Keeping the message types and framing in one crate means the two sides
//! cannot drift: a change to a message shape breaks compilation of both.
//!
//! Two framings live here:
//!
//! * **Control framing** ([`read_frame`]/[`write_frame`]) — a `u32` big-endian
//!   length prefix followed by a JSON body, used only at the head of a stream
//!   (the handshake and the per-connection open exchange).
//! * **Raw bytes** — once a stream finishes its control exchange it carries raw
//!   bytes in both directions with no further framing, spliced by [`splice`].

mod frame;
mod messages;

pub use frame::{read_frame, write_frame, FrameError, MAX_FRAME_LEN};
pub use messages::{
    client_key, ClientHello, ControlToClient, ControlToGateway, OpenTarget, RelayOpen, RelayOpened,
    ServerHello, TargetOpened, ALPN, SERVER_NAME,
};

/// Copy bytes in both directions between two byte pipes until both are done.
///
/// This is the splice at the heart of every proxied connection: proxy user ↔
/// QUIC stream on the gateway, QUIC stream ↔ target socket on the exit node. It
/// returns `(bytes_a_to_b, bytes_b_to_a)`.
///
/// Each direction shuts down the far side's write half when its source hits EOF,
/// so a target closing its end propagates as a normal FIN rather than hanging
/// until a timeout.
///
/// Metering note: the caller must not rely on the return value alone for
/// byte accounting — `copy_bidirectional` yields the totals only once *both*
/// directions finish. Long-lived tunnels that end abruptly (the common case for
/// foreground-only exit nodes) need an incremental counter wrapped around the
/// pipes instead; see the gateway's metering layer.
pub async fn splice<A, B>(a: &mut A, b: &mut B) -> std::io::Result<(u64, u64)>
where
    A: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    B: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tokio::io::copy_bidirectional(a, b).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_key_joins_account_and_installation() {
        assert_eq!(client_key("acct-123", "install-9"), "acct-123:install-9");
    }
}
