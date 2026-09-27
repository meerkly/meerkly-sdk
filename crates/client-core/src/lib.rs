//! Runtime-agnostic meerkly exit-node core.
//!
//! This crate is the single implementation of the exit node, wrapped by every
//! platform binding (`client-napi` for Node, `client-ffi` for
//! Kotlin/Go/Swift/Python). It holds exactly one QUIC connection to one gateway,
//! opens outbound TCP sockets on demand, and reconnects automatically.
//!
//! **Runtime ownership.** Unlike a Node addon, the FFI bindings have no ambient
//! async runtime, so [`ProxyClient`] takes an explicit [`tokio::runtime::Handle`]
//! and runs the supervisor on it. The endpoint driver and every connection task
//! therefore share one runtime regardless of which runtime drives `start()`.
//!
//! **Ephemeral-session lifecycle.** Built for host apps that cannot run in the
//! background: fast `start()`, clean fast `stop()`, and no assumption that a
//! session lasts more than seconds.

mod config;
mod connection;
mod guard;
mod tls;

pub use config::{
    default_ca_cert_path, CaCert, ClientConfig, DEFAULT_CONNECT_TIMEOUT, DEFAULT_GATEWAY_ADDRESSES,
    DEFAULT_KEEP_ALIVE, DEFAULT_MAX_BACKOFF, DEFAULT_MAX_IDLE, DEFAULT_MIN_BACKOFF,
    DEFAULT_START_TIMEOUT,
};
pub use connection::{RejectionSlot, State, Supervisor};

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Semantic version of the client core, surfaced to bindings for the register
/// handshake's `client_version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Inner {
    config: ClientConfig,
    handle: tokio::runtime::Handle,
    state_tx: watch::Sender<State>,
    state_rx: watch::Receiver<State>,
    shutdown_tx: watch::Sender<bool>,
    started: AtomicBool,
    rejection: RejectionSlot,
}

/// A handle to a single exit-node client.
///
/// Construction validates the config but does no I/O; [`start`](Self::start)
/// spawns the connection supervisor on the runtime handle and resolves once the
/// client has registered with a gateway.
///
/// Cheaply cloneable (it is an `Arc` inside); clones share one supervisor, so a
/// binding can move a clone into an async task without borrowing the handle.
#[derive(Clone)]
pub struct ProxyClient {
    inner: Arc<Inner>,
}

impl ProxyClient {
    /// Validate the config and prepare a client bound to `handle`.
    ///
    /// The supervisor — and thus the QUIC endpoint and all connection tasks —
    /// runs on `handle`. A Node binding passes its ambient runtime handle; a JNI
    /// or PyO3 binding creates a multi-threaded, timer-enabled runtime and passes
    /// its handle.
    pub fn new(mut config: ClientConfig, handle: tokio::runtime::Handle) -> Result<Self> {
        // An empty address list means "wherever Meerkly is", not "nowhere".
        //
        // ClientConfig::new already fills this, but every binding builds the
        // struct literally instead — client-ffi and client-napi both do — so the
        // default never reached them and an omitted list was rejected as invalid.
        // Filling it here rather than in each binding is what makes the SDK
        // documentation true for all of them at once, and means the next binding
        // cannot forget.
        //
        // A host that names its own gateways is untouched; only the empty case
        // is filled.
        if config.gateway_addresses.is_empty() {
            config.gateway_addresses = DEFAULT_GATEWAY_ADDRESSES
                .iter()
                .map(|a| (*a).to_owned())
                .collect();
        }
        config.validate()?;
        let (state_tx, state_rx) = watch::channel(State::Idle);
        let (shutdown_tx, _) = watch::channel(false);
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                handle,
                state_tx,
                state_rx,
                shutdown_tx,
                started: AtomicBool::new(false),
                rejection: RejectionSlot::default(),
            }),
        })
    }

    /// Connect to a gateway and start serving proxy requests.
    ///
    /// Resolves once the client is registered with a gateway. Rejects if no
    /// gateway accepts the client within `start_timeout`, leaving the client
    /// stopped so the caller can decide whether to retry.
    pub async fn start(&self) -> Result<()> {
        let inner = self.inner.clone();
        if inner.started.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("this client has already been started"));
        }

        // Build and run the supervisor on the client's own runtime, so the QUIC
        // endpoint driver and all connection tasks share it regardless of which
        // runtime is driving `start()`. A construction failure (bad CA, endpoint
        // bind) is surfaced here rather than swallowed by the spawned task.
        let (created_tx, created_rx) = tokio::sync::oneshot::channel::<Result<()>>();
        {
            let config = inner.config.clone();
            let state_tx = inner.state_tx.clone();
            let shutdown_tx = inner.shutdown_tx.clone();
            let rejection = inner.rejection.clone();
            inner.handle.spawn(async move {
                match Supervisor::new(config, state_tx, shutdown_tx) {
                    Ok(supervisor) => {
                        let supervisor = supervisor.with_rejection_slot(rejection);
                        let _ = created_tx.send(Ok(()));
                        supervisor.run().await;
                    }
                    Err(e) => {
                        let _ = created_tx.send(Err(e));
                    }
                }
            });
        }
        match created_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                inner.started.store(false, Ordering::SeqCst);
                return Err(e);
            }
            Err(_) => {
                inner.started.store(false, Ordering::SeqCst);
                return Err(anyhow!("supervisor task dropped before starting"));
            }
        }

        let mut state_rx = inner.state_rx.clone();
        let wait = async {
            loop {
                if matches!(*state_rx.borrow_and_update(), State::Connected { .. }) {
                    return;
                }
                if state_rx.changed().await.is_err() {
                    return;
                }
            }
        };

        match tokio::time::timeout(inner.config.start_timeout, wait).await {
            Ok(()) if self.connected() => Ok(()),
            _ => {
                let _ = inner.shutdown_tx.send(true);
                inner.started.store(false, Ordering::SeqCst);
                let base = format!(
                    "no gateway accepted this client within {:?} (tried {})",
                    inner.config.start_timeout,
                    inner.config.gateway_addresses.join(", ")
                );
                Err(match self.last_rejection() {
                    Some(reason) => anyhow!("{base}: {reason}"),
                    None => anyhow!(base),
                })
            }
        }
    }

    /// Close the QUIC connection and stop reconnecting.
    pub async fn stop(&self) -> Result<()> {
        let inner = self.inner.clone();
        let _ = inner.shutdown_tx.send(true);

        let mut state_rx = inner.state_rx.clone();
        // Bounded so `stop()` cannot hang a shutdown path if the supervisor is
        // wedged mid-connection.
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if matches!(*state_rx.borrow_and_update(), State::Stopped) {
                    return;
                }
                if state_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .await;

        inner.started.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Whether the client currently holds a connection to a gateway.
    pub fn connected(&self) -> bool {
        matches!(*self.inner.state_rx.borrow(), State::Connected { .. })
    }

    /// The gateway currently serving this client, or `None` when disconnected.
    pub fn gateway_id(&self) -> Option<String> {
        match &*self.inner.state_rx.borrow() {
            State::Connected { gateway_id, .. } => Some(gateway_id.clone()),
            _ => None,
        }
    }

    /// One of `idle`, `connecting`, `connected`, `stopped`.
    pub fn state(&self) -> State {
        self.inner.state_rx.borrow().clone()
    }

    /// The reason the gateway gave the last time it refused this client — for
    /// example "another device is already connected from this IP address" — or
    /// `None` if it has not refused it since the last successful connection.
    ///
    /// Only the gateway's own refusals, which are written to be shown to a
    /// person; network failures are not reported here. The client keeps
    /// retrying either way, so this can be set while [`state`](Self::state) is
    /// `Connecting` or `Idle`.
    pub fn last_rejection(&self) -> Option<String> {
        self.inner.rejection.lock().ok().and_then(|r| r.clone())
    }

    /// The cluster-wide identity the gateway assigned to *this connection*
    /// (`account:instance-id`), or `None` until connected. Ephemeral: a
    /// reconnect yields a new one. It is what keeps two concurrent connections
    /// from one machine distinguishable; the persistent identity, if the host
    /// keeps one, is [`ClientConfig::device_id`], and is reported separately.
    pub fn client_key(&self) -> Option<String> {
        match &*self.inner.state_rx.borrow() {
            State::Connected { client_key, .. } => Some(client_key.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod client_defaults_tests {
    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    /// The bug this guards: every binding builds ClientConfig as a struct
    /// literal rather than through ClientConfig::new, so the default address
    /// never reached them and an omitted list was rejected as invalid — which
    /// is exactly what the SDK documentation told hosts to do.
    #[test]
    fn an_empty_address_list_falls_back_to_the_production_gateway() {
        let rt = runtime();
        let config = ClientConfig {
            gateway_addresses: Vec::new(),
            ..ClientConfig::new("pub_test")
        };

        let client = ProxyClient::new(config, rt.handle().clone())
            .expect("an empty list must mean the production gateway, not an error");

        assert_eq!(
            client.inner.config.gateway_addresses,
            DEFAULT_GATEWAY_ADDRESSES.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn addresses_a_host_names_itself_are_left_alone() {
        let rt = runtime();
        let config = ClientConfig {
            gateway_addresses: vec!["gw.example.test:4443".to_owned()],
            ..ClientConfig::new("pub_test")
        };

        let client = ProxyClient::new(config, rt.handle().clone()).expect("valid config");

        assert_eq!(
            client.inner.config.gateway_addresses,
            vec!["gw.example.test:4443".to_owned()],
        );
    }

    /// Filling the default must not turn other invalid configs into valid ones.
    #[test]
    fn an_empty_publisher_id_is_still_rejected() {
        let rt = runtime();
        let config = ClientConfig {
            gateway_addresses: Vec::new(),
            ..ClientConfig::new("")
        };

        assert!(ProxyClient::new(config, rt.handle().clone()).is_err());
    }
}
