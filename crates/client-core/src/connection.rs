//! The connection supervisor.
//!
//! Maintains exactly one QUIC connection to one gateway, no matter how many
//! proxied connections flow over it. Every proxy request is a QUIC stream on that
//! single connection; the supervisor's only job is to keep the connection there
//! and to rebuild it when it dies.

use crate::config::ClientConfig;
use anyhow::{Context, Result};
use meerkly_protocol::{
    read_frame, splice, write_frame, ClientHello, ControlToClient, ControlToGateway, OpenTarget,
    ServerHello, TargetOpened, SERVER_NAME,
};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::{debug, info, warn};

/// The reason the gateway gave the last time it refused this client, shared
/// between the supervisor that hears it and the [`crate::ProxyClient`] that
/// reports it. Cleared once a gateway accepts the client again.
pub type RejectionSlot = Arc<Mutex<Option<String>>>;

/// What the client is currently doing. Surfaced to host apps so they can show
/// connection state without polling the gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Idle,
    Connecting,
    /// Registered with a gateway. `client_key` is the gateway-assigned
    /// cluster-wide handle (`account:ephemeral-instance-id`).
    Connected {
        gateway_id: String,
        client_key: String,
    },
    Stopped,
}

pub struct Supervisor {
    config: ClientConfig,
    endpoint: quinn::Endpoint,
    state_tx: watch::Sender<State>,
    shutdown_tx: watch::Sender<bool>,
    rejection: RejectionSlot,
}

impl Supervisor {
    /// Must be called from within the tokio runtime the supervisor will run on:
    /// `quinn::Endpoint::client` spawns the endpoint driver onto the current
    /// runtime.
    pub fn new(
        config: ClientConfig,
        state_tx: watch::Sender<State>,
        shutdown_tx: watch::Sender<bool>,
    ) -> Result<Self> {
        crate::tls::install_crypto_provider();

        // One endpoint for the process lifetime: reconnects reuse the same UDP
        // socket rather than churning ports, which keeps NAT bindings stable.
        let mut endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse().expect("valid bind address"))
                .context("cannot create the QUIC endpoint")?;
        endpoint.set_default_client_config(crate::tls::client_config(
            &config.ca_cert,
            config.keep_alive,
        )?);

        Ok(Self {
            config,
            endpoint,
            state_tx,
            shutdown_tx,
            rejection: RejectionSlot::default(),
        })
    }

    /// Record the gateway's refusal reasons in `slot`, so the host can show
    /// them. A builder rather than a `new` parameter, so `new` keeps its
    /// signature for anyone constructing a supervisor directly.
    pub fn with_rejection_slot(mut self, slot: RejectionSlot) -> Self {
        self.rejection = slot;
        self
    }

    fn set_rejection(&self, reason: Option<String>) {
        if let Ok(mut slot) = self.rejection.lock() {
            *slot = reason;
        }
    }

    /// Connect, serve, reconnect — until shutdown is requested.
    pub async fn run(self) {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut backoff = self.config.min_backoff;
        let mut next_address = 0usize;

        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            let address = &self.config.gateway_addresses[next_address];
            next_address = (next_address + 1) % self.config.gateway_addresses.len();

            let _ = self.state_tx.send(State::Connecting);
            match self.connect_and_serve(address, &mut shutdown_rx).await {
                Ok(()) => {
                    // A clean end means the gateway closed the connection or we
                    // are shutting down. Either way the next attempt should be
                    // immediate, not backed off.
                    backoff = self.config.min_backoff;
                }
                Err(e) => {
                    warn!(gateway = %address, error = %format!("{e:#}"), "gateway connection failed");
                }
            }

            if *shutdown_rx.borrow() {
                break;
            }
            let _ = self.state_tx.send(State::Idle);

            debug!(?backoff, "waiting before the next connection attempt");
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = shutdown_rx.changed() => {}
            }
            backoff = (backoff * 2).min(self.config.max_backoff);
        }

        self.endpoint.close(0u32.into(), b"client stopping");
        let _ = self.state_tx.send(State::Stopped);
        info!("client stopped");
    }

    async fn connect_and_serve(
        &self,
        address: &str,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let socket_addr = resolve(address).await?;
        debug!(gateway = %address, resolved = %socket_addr, "connecting");

        // Pinned trust uses the fixed dev server name; public-root trust must
        // present the real hostname, since the gateway's cert is issued for it.
        let server_name = if self.config.ca_cert.is_public_roots() {
            host_of(address)
        } else {
            SERVER_NAME.to_owned()
        };

        let connection = self
            .endpoint
            .connect(socket_addr, &server_name)
            .context("cannot start the QUIC handshake")?
            .await
            .context("QUIC handshake failed")?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .context("cannot open the control stream")?;

        write_frame(&mut send, &client_hello(&self.config))
            .await
            .context("sending client_hello")?;

        let hello: ServerHello = read_frame(&mut recv).await.context("reading server_hello")?;
        if !hello.accepted {
            // Kept apart from transport errors: this one is written for people
            // (e.g. "another device is already connected from this IP
            // address"), and it is the one a host app can usefully show.
            let reason = hello
                .reason
                .unwrap_or_else(|| "no reason given".to_owned());
            self.set_rejection(Some(reason.clone()));
            anyhow::bail!("gateway rejected this client: {reason}");
        }
        self.set_rejection(None);

        info!(
            gateway_id = %hello.gateway_id,
            gateway = %address,
            client_key = %hello.client_key,
            "connected"
        );

        // The gateway has just told us how often it will ping. Our transport
        // keepalive is only a fallback for that ping stopping (see
        // DEFAULT_KEEP_ALIVE), and it is suppressed only while the ping is
        // faster than it. quinn cannot retune a live connection's transport
        // config, so a mismatch cannot be fixed from here — but it costs every
        // idle connection a PING and an ACK per cycle, so it is said out loud
        // rather than left to show up as an unexplained rise in metered bytes.
        if hello.heartbeat_secs >= self.config.keep_alive.as_secs() {
            warn!(
                gateway_heartbeat_secs = hello.heartbeat_secs,
                keep_alive_secs = self.config.keep_alive.as_secs(),
                "the gateway's heartbeat is not faster than our transport keepalive; \
                 idle connections will send redundant PINGs"
            );
        }
        let _ = self.state_tx.send(State::Connected {
            gateway_id: hello.gateway_id.clone(),
            client_key: hello.client_key.clone(),
        });

        // Streams opened by the gateway are proxy requests. Accepting them in a
        // separate task keeps the control stream responsive under load.
        let accept_task = tokio::spawn(accept_streams(connection.clone(), self.config.connect_timeout));

        let result = self
            .serve_control(&connection, &mut send, recv, shutdown_rx)
            .await;
        accept_task.abort();
        result
    }

    /// Answer heartbeats until the connection ends or shutdown is requested.
    async fn serve_control(
        &self,
        connection: &quinn::Connection,
        send: &mut quinn::SendStream,
        mut recv: quinn::RecvStream,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        // Frames are read in a dedicated task and handed over a channel:
        // `read_frame` is not cancellation-safe, so it must never be a `select!`
        // branch where a competing arm could drop it mid-frame.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ControlToClient>(8);
        let reader = tokio::spawn(async move {
            while let Ok(frame) = read_frame::<_, ControlToClient>(&mut recv).await {
                if tx.send(frame).await.is_err() {
                    break;
                }
            }
        });

        let outcome = loop {
            tokio::select! {
                frame = rx.recv() => match frame {
                    Some(ControlToClient::Ping { nonce }) => {
                        if let Err(e) = write_frame(send, &ControlToGateway::Pong { nonce }).await {
                            break Err(anyhow::Error::new(e).context("answering a heartbeat"));
                        }
                    }
                    Some(ControlToClient::Shutdown { reason }) => {
                        info!(%reason, "gateway asked us to reconnect");
                        break Ok(());
                    }
                    // The reader task ended, which means the control stream is
                    // gone even if the connection has not noticed yet.
                    None => break Ok(()),
                },

                _ = connection.closed() => break Ok(()),

                _ = shutdown_rx.changed() => {
                    connection.close(0u32.into(), b"client stopping");
                    break Ok(());
                }
            }
        };

        reader.abort();
        outcome
    }
}

/// Accept proxy-request streams for the life of the connection.
async fn accept_streams(connection: quinn::Connection, connect_timeout: std::time::Duration) {
    loop {
        match connection.accept_bi().await {
            Ok((send, recv)) => {
                tokio::spawn(async move {
                    if let Err(e) = serve_stream(send, recv, connect_timeout).await {
                        debug!(error = %format!("{e:#}"), "proxied connection ended");
                    }
                });
            }
            // The connection is gone; the supervisor will notice and reconnect.
            Err(_) => return,
        }
    }
}

/// Handle one proxy request: dial the target and splice.
///
/// This is where the network property that defines the product lives — the
/// outbound TCP connection originates here, on the exit node's machine, so the
/// target sees the exit node's IP.
async fn serve_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    connect_timeout: std::time::Duration,
) -> Result<()> {
    let request: OpenTarget = read_frame(&mut recv).await.context("reading open_target")?;

    // SSRF guard: resolve + validate the target and connect to a public address
    // only (see `guard`). Refusals surface as an in-band TargetOpened error below.
    let dial = tokio::time::timeout(
        connect_timeout,
        crate::guard::connect(&request.host, request.port),
    )
    .await;

    let target = match dial {
        Ok(Ok(stream)) => stream,
        // Both a refused connection and a timeout are reported in-band rather
        // than by resetting the stream, so the gateway can pass a useful reason
        // back to the proxy user.
        Ok(Err(e)) => {
            write_frame(
                &mut send,
                &TargetOpened {
                    ok: false,
                    error: Some(e.to_string()),
                    local_addr: None,
                },
            )
            .await?;
            return Ok(());
        }
        Err(_) => {
            write_frame(
                &mut send,
                &TargetOpened {
                    ok: false,
                    error: Some(format!("timed out after {connect_timeout:?}")),
                    local_addr: None,
                },
            )
            .await?;
            return Ok(());
        }
    };

    target.set_nodelay(true).ok();
    let local_addr = target.local_addr().ok().map(|a| a.to_string());
    debug!(
        target = %format_args!("{}:{}", request.host, request.port),
        ?local_addr,
        "target connected"
    );

    write_frame(
        &mut send,
        &TargetOpened {
            ok: true,
            error: None,
            local_addr,
        },
    )
    .await?;

    let mut target = target;
    let mut tunnel = tokio::io::join(recv, send);
    splice(&mut tunnel, &mut target)
        .await
        .context("proxying data")?;
    Ok(())
}

/// Resolve `host:port` to a socket address.
///
/// Prefers IPv4: the QUIC endpoint is bound to `0.0.0.0`, so an IPv6 result
/// would be unroutable from it.
async fn resolve(address: &str) -> Result<std::net::SocketAddr> {
    let resolved = tokio::net::lookup_host(address)
        .await
        .with_context(|| format!("cannot resolve gateway address {address}"))?;

    let mut first = None;
    for addr in resolved {
        if addr.is_ipv4() {
            return Ok(addr);
        }
        first.get_or_insert(addr);
    }
    first.with_context(|| format!("gateway address {address} resolved to nothing"))
}

/// The host portion of a `host:port` address, for use as the TLS SNI when
/// trusting public roots. Strips a trailing `:port` and IPv6 brackets; leaves a
/// bare host unchanged.
fn host_of(address: &str) -> String {
    let host = match address.rsplit_once(':') {
        // Only treat the tail as a port when it is numeric, so a bare IPv6
        // literal (`::1`) is not mistaken for `host:port`.
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => address,
    };
    host.trim_start_matches('[').trim_end_matches(']').to_owned()
}

/// Build the handshake frame this client presents.
///
/// Split out of `connect` so it can be asserted directly: the hello is the whole
/// of what an exit tells the network about itself, and it is otherwise only
/// observable by standing up a gateway.
///
/// The SDK fills in what it knows for certain — its own version, the OS and the
/// architecture it was compiled for — and passes through what only the host can
/// know. It deliberately does not invent a `device_id`: a library that minted
/// one would give every process on a machine a different identity, and would
/// have nowhere durable to keep it.
fn client_hello(config: &ClientConfig) -> ClientHello {
    ClientHello {
        publisher_id: config.publisher_id.clone(),
        client_version: env!("CARGO_PKG_VERSION").to_owned(),
        country: config.country.clone(),
        device_id: config.device_id.clone(),
        device_os: Some(std::env::consts::OS.to_owned()),
        device_arch: Some(std::env::consts::ARCH.to_owned()),
        device_name: config.device_name.clone(),
        sdk: Some(
            config
                .sdk
                .clone()
                .unwrap_or_else(|| crate::config::DEFAULT_SDK.to_owned()),
        ),
        app: config.app.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{client_hello, host_of};
    use crate::ClientConfig;

    /// The SDK fills OS and architecture itself rather than trusting the host to
    /// pass them, so no binding can report them wrongly or forget them.
    #[test]
    fn the_sdk_reports_its_own_os_and_arch() {
        let hello = client_hello(&ClientConfig::new("pub_abc"));
        assert_eq!(hello.device_os.as_deref(), Some(std::env::consts::OS));
        assert_eq!(hello.device_arch.as_deref(), Some(std::env::consts::ARCH));
    }

    /// A host that stores no device identity stays anonymous — the fields are
    /// absent rather than empty strings, which is what the gateway treats as
    /// "this client has no identity".
    #[test]
    fn a_host_that_sets_nothing_reports_no_identity() {
        let hello = client_hello(&ClientConfig::new("pub_abc"));
        assert!(hello.device_id.is_none());
        assert!(hello.device_name.is_none());
        assert!(hello.app.is_none());
    }

    /// `sdk` defaults to the binding actually compiled in. A direct Rust
    /// consumer is "rust"; napi overrides it with "node", and the uniffi
    /// languages set it themselves because one crate serves all five.
    #[test]
    fn sdk_defaults_to_rust_and_is_overridable() {
        let hello = client_hello(&ClientConfig::new("pub_abc"));
        assert_eq!(hello.sdk.as_deref(), Some("rust"));

        let mut config = ClientConfig::new("pub_abc");
        config.sdk = Some("python".to_owned());
        assert_eq!(client_hello(&config).sdk.as_deref(), Some("python"));
    }

    #[test]
    fn host_supplied_identity_reaches_the_wire() {
        let mut config = ClientConfig::new("pub_abc");
        config.device_id = Some("dev_1234".to_owned());
        config.device_name = Some("prod-fra-01".to_owned());
        config.app = Some("meerkly-agent/1.0.0".to_owned());

        let hello = client_hello(&config);
        assert_eq!(hello.device_id.as_deref(), Some("dev_1234"));
        assert_eq!(hello.device_name.as_deref(), Some("prod-fra-01"));
        assert_eq!(hello.app.as_deref(), Some("meerkly-agent/1.0.0"));
        assert_eq!(hello.publisher_id, "pub_abc");
        assert_eq!(hello.client_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn host_of_strips_port_and_brackets() {
        assert_eq!(host_of("gw.meerkly.com:4443"), "gw.meerkly.com");
        assert_eq!(host_of("gw.meerkly.com"), "gw.meerkly.com");
        assert_eq!(host_of("[2001:db8::1]:4443"), "2001:db8::1");
        assert_eq!(host_of("192.0.2.10:4443"), "192.0.2.10");
    }
}
