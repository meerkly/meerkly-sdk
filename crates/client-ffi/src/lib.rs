//! uniffi binding over [`meerkly_client_core`] → Kotlin / Swift / Python / Go.
//!
//! One interface, generated to every uniffi-supported language (Kotlin, Swift,
//! Python) plus Go (via `uniffi-bindgen-go`). A thin shell that owns a dedicated
//! tokio runtime and delegates to the core `ProxyClient` — the FFI counterpart of
//! the napi binding.

use meerkly_client_core::{
    CaCert, ClientConfig as CoreConfig, ProxyClient as CoreClient, State as CoreState,
    DEFAULT_KEEP_ALIVE,
};
use std::sync::Arc;
use std::time::Duration;

uniffi::setup_scaffolding!("meerkly");

/// Options for [`ProxyClient::new`]. In production a host passes only
/// `publisherId`; the gateway address and CA are development overrides.
#[derive(uniffi::Record)]
pub struct ProxyConfig {
    pub publisher_id: String,
    pub gateway_addresses: Vec<String>,
    #[uniffi(default = None)]
    pub ca_cert_path: Option<String>,
    #[uniffi(default = None)]
    pub ca_cert_pem: Option<Vec<u8>>,
    #[uniffi(default = None)]
    pub start_timeout_ms: Option<u32>,
    #[uniffi(default = None)]
    pub connect_timeout_ms: Option<u32>,
    /// Stable id for this machine, if the host keeps one. The SDK never mints or
    /// stores it — only the host knows where an id belongs on its platform.
    #[uniffi(default = None)]
    pub device_id: Option<String>,
    /// A human name for this machine, conventionally its hostname.
    #[uniffi(default = None)]
    pub device_name: Option<String>,
    /// Which binding is calling: "python", "ruby", "go", "swift", "kotlin".
    /// Unlike the node binding, this crate cannot detect it — uniffi generates
    /// all five from this one crate — so a host that wants its language
    /// attributed correctly sets it here. Defaults to the SDK's own default.
    #[uniffi(default = None)]
    pub sdk: Option<String>,
    /// The host application, e.g. "my-app/2.1.0".
    #[uniffi(default = None)]
    pub app: Option<String>,
    /// The transport the host is on at start: "cellular", "wifi", "ethernet"
    /// or "other". Report later changes with `set_network`.
    #[uniffi(default = None)]
    pub network: Option<String>,
}

/// What the client is currently doing.
#[derive(uniffi::Enum)]
pub enum ClientState {
    Idle,
    Connecting,
    Connected,
    Stopped,
}

/// The single error every fallible SDK call returns.
///
/// The field is `reason`, not `message`, and that is not a style choice. uniffi
/// generates the Kotlin variant as a subclass of `kotlin.Exception` with an
/// `override val message`, so a field literally named `message` collides with
/// `Throwable.message` and the generated Kotlin does not compile. Upstream's
/// answer is to not name an error field `message`. Nothing else reads the name,
/// and Kotlin callers still get the text through `e.message` as usual.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ProxyError {
    #[error("{reason}")]
    Failed { reason: String },
}

impl From<anyhow::Error> for ProxyError {
    fn from(e: anyhow::Error) -> Self {
        ProxyError::Failed {
            reason: format!("{e:#}"),
        }
    }
}

/// A single exit-node client. `start()` connects and registers; a background
/// supervisor keeps the connection up until `stop()`. Stores nothing itself —
/// a device id, if the host keeps one, is passed in and only carried.
#[derive(uniffi::Object)]
pub struct ProxyClient {
    core: CoreClient,
    // Owned so the async work has a runtime, and so the blocking `*_blocking`
    // variants (for bindings without async support, e.g. Ruby) can drive it.
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export(async_runtime = "tokio")]
impl ProxyClient {
    #[uniffi::constructor]
    pub fn new(config: ProxyConfig) -> Result<Arc<Self>, ProxyError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| ProxyError::Failed {
                reason: format!("cannot start the tokio runtime: {e}"),
            })?;

        // No CA supplied → trust the public roots (production gateway with a
        // Let's Encrypt cert); an explicit path/PEM pins a self-signed dev CA.
        let ca_cert = CaCert::from_opts(config.ca_cert_pem, config.ca_cert_path);

        let core_config = CoreConfig {
            publisher_id: config.publisher_id,
            gateway_addresses: config.gateway_addresses,
            // Not exposed by the SDK: the gateway derives geo from the observed IP.
            country: None,
            ca_cert,
            start_timeout: Duration::from_millis(config.start_timeout_ms.unwrap_or(30_000) as u64),
            connect_timeout: Duration::from_millis(config.connect_timeout_ms.unwrap_or(15_000) as u64),
            // Not exposed as an option, so it is the core default rather than a
            // copy of it: a hardcoded 10s here silently outlived the change that
            // moved the shared constant off the gateway's heartbeat interval.
            keep_alive: DEFAULT_KEEP_ALIVE,
            min_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            device_id: config.device_id,
            device_name: config.device_name,
            sdk: config.sdk,
            app: config.app,
        };

        let core = CoreClient::new(core_config, runtime.handle().clone())?;
        core.set_network(config.network);
        Ok(Arc::new(Self { core, runtime }))
    }

    /// Connect and register. Resolves once online; errors on timeout.
    pub async fn start(&self) -> Result<(), ProxyError> {
        let core = self.core.clone();
        core.start().await.map_err(Into::into)
    }

    /// Close the connection and stop reconnecting.
    pub async fn stop(&self) -> Result<(), ProxyError> {
        let core = self.core.clone();
        core.stop().await.map_err(Into::into)
    }

    /// Blocking `start`, for bindings without async support (e.g. Ruby). Blocks
    /// the caller until connected (or the start timeout).
    pub fn start_blocking(&self) -> Result<(), ProxyError> {
        self.runtime.block_on(self.core.start()).map_err(Into::into)
    }

    /// Blocking `stop`.
    pub fn stop_blocking(&self) -> Result<(), ProxyError> {
        self.runtime.block_on(self.core.stop()).map_err(Into::into)
    }

    pub fn connected(&self) -> bool {
        self.core.connected()
    }

    pub fn gateway_id(&self) -> Option<String> {
        self.core.gateway_id()
    }

    /// The gateway-assigned `account:instance-id`, or null until connected.
    pub fn client_key(&self) -> Option<String> {
        self.core.client_key()
    }

    /// Why the gateway last refused this client (e.g. "another device is
    /// already connected from this IP address"), or null if it has not since
    /// the last successful connection. Meant to be shown to the user while the
    /// client keeps retrying.
    pub fn last_rejection(&self) -> Option<String> {
        self.core.last_rejection()
    }

    /// Tell the gateway which transport the host is on now: "cellular",
    /// "wifi", "ethernet" or "other"; null when unknown. Call it whenever the
    /// platform reports a change (on Android, a `ConnectivityManager`
    /// callback). The gateway uses it, with the network it measures, to
    /// classify the exit as mobile, residential or datacenter.
    pub fn set_network(&self, network: Option<String>) {
        self.core.set_network(network);
    }

    /// The transport last reported, or null.
    pub fn network(&self) -> Option<String> {
        self.core.network()
    }

    pub fn state(&self) -> ClientState {
        match self.core.state() {
            CoreState::Idle => ClientState::Idle,
            CoreState::Connecting => ClientState::Connecting,
            CoreState::Connected { .. } => ClientState::Connected,
            CoreState::Stopped => ClientState::Stopped,
        }
    }
}
