//! napi-rs binding over [`meerkly_client_core`] → the `@meerkly/sdk`
//! Node addon (desktop and Node server hosts).
//!
//! A thin shell: it maps JS options to a [`ClientConfig`], owns a dedicated tokio
//! runtime (the FFI side has no ambient one), and delegates to the core
//! `ProxyClient`. The core does the real work.

use meerkly_client_core::{
    CaCert, ClientConfig, ProxyClient as CoreClient, State, DEFAULT_KEEP_ALIVE,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;
use std::time::Duration;

/// Options for [`ProxyClient`]. In production a host passes only `publisherId`;
/// the gateway address and CA are development overrides.
#[napi(object)]
pub struct ProxyClientOptions {
    /// The public publisher id (an identifier, not a secret) the gateway resolves
    /// to the earning account.
    pub publisher_id: String,
    /// Gateway `host:port` addresses, tried in rotation.
    pub gateway_addresses: Vec<String>,
    /// PEM file pinning the gateway CA (for a self-signed dev gateway). Omit in
    /// production to verify the gateway's public cert against the system roots.
    /// Ignored when `caCertPem` is set.
    pub ca_cert_path: Option<String>,
    /// PEM bytes of the gateway CA (an alternative to a file path).
    pub ca_cert_pem: Option<Uint8Array>,
    /// How long `start()` waits for the first connection. Default 30000.
    pub start_timeout_ms: Option<u32>,
    /// How long the exit waits when dialling a target. Default 15000.
    pub connect_timeout_ms: Option<u32>,
    /// Stable id for this machine, if the host keeps one. The SDK never mints or
    /// stores it — only the host knows where an id belongs on its platform.
    /// Omit to stay anonymous in the dashboard.
    pub device_id: Option<String>,
    /// A human name for this machine, conventionally its hostname. Seeds the
    /// dashboard label.
    pub device_name: Option<String>,
    /// The host application, e.g. "my-app/2.1.0". Distinct from the SDK version,
    /// which the SDK reports itself.
    pub app: Option<String>,
}

struct Inner {
    core: CoreClient,
    // Kept alive for the client's lifetime; dropping it shuts the runtime down.
    _runtime: tokio::runtime::Runtime,
}

/// A single exit-node client. `start()` connects and registers; a background
/// supervisor then keeps the connection up until `stop()`.
#[napi]
pub struct ProxyClient {
    inner: Arc<Inner>,
}

#[napi]
impl ProxyClient {
    #[napi(constructor)]
    pub fn new(options: ProxyClientOptions) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::from_reason(format!("cannot start the tokio runtime: {e}")))?;

        // No CA supplied → trust the public roots (production gateway with a
        // Let's Encrypt cert); an explicit path/PEM pins a self-signed dev CA.
        let ca_cert = CaCert::from_opts(options.ca_cert_pem.map(|p| p.to_vec()), options.ca_cert_path);

        let config = ClientConfig {
            publisher_id: options.publisher_id,
            gateway_addresses: options.gateway_addresses,
            // Not exposed by the SDK: the gateway derives geo from the observed IP.
            country: None,
            ca_cert,
            start_timeout: Duration::from_millis(options.start_timeout_ms.unwrap_or(30_000) as u64),
            connect_timeout: Duration::from_millis(options.connect_timeout_ms.unwrap_or(15_000) as u64),
            // Not exposed as an option, so it is the core default rather than a
            // copy of it: a hardcoded 10s here silently outlived the change that
            // moved the shared constant off the gateway's heartbeat interval.
            keep_alive: DEFAULT_KEEP_ALIVE,
            min_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            device_id: options.device_id,
            device_name: options.device_name,
            // This crate *is* the node binding, so it can name itself. The
            // uniffi bindings cannot: one crate serves five languages there.
            sdk: Some("node".to_owned()),
            app: options.app,
        };

        let core = CoreClient::new(config, runtime.handle().clone()).map_err(to_napi_error)?;
        Ok(Self {
            inner: Arc::new(Inner {
                core,
                _runtime: runtime,
            }),
        })
    }

    /// Connect to a gateway and start serving proxy requests. Resolves once
    /// registered; rejects if no gateway accepts within `startTimeoutMs`.
    #[napi]
    pub async fn start(&self) -> Result<()> {
        let core = self.inner.core.clone();
        core.start().await.map_err(to_napi_error)
    }

    /// Close the connection and stop reconnecting.
    #[napi]
    pub async fn stop(&self) -> Result<()> {
        let core = self.inner.core.clone();
        core.stop().await.map_err(to_napi_error)
    }

    #[napi(getter)]
    pub fn connected(&self) -> bool {
        self.inner.core.connected()
    }

    #[napi(getter)]
    pub fn gateway_id(&self) -> Option<String> {
        self.inner.core.gateway_id()
    }

    /// The gateway-assigned `account:instance-id`, or `null` until connected.
    #[napi(getter)]
    pub fn client_key(&self) -> Option<String> {
        self.inner.core.client_key()
    }

    /// Why the gateway last refused this client, or `null` if it has not since
    /// the last successful connection.
    #[napi(getter)]
    pub fn last_rejection(&self) -> Option<String> {
        self.inner.core.last_rejection()
    }

    /// One of `idle`, `connecting`, `connected`, `stopped`.
    #[napi(getter)]
    pub fn state(&self) -> String {
        match self.inner.core.state() {
            State::Idle => "idle",
            State::Connecting => "connecting",
            State::Connected { .. } => "connected",
            State::Stopped => "stopped",
        }
        .to_owned()
    }
}

fn to_napi_error(e: anyhow::Error) -> Error {
    Error::from_reason(format!("{e:#}"))
}
