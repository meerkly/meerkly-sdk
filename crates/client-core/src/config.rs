//! Client configuration, validated once at construction.

use anyhow::{Context, Result};
use std::time::Duration;

/// Where the gateway CA comes from.
///
/// Desktop and server hosts point at a PEM file; mobile hosts (Android/iOS) have
/// no stable filesystem path to hand in, so they pass the bytes directly.
#[derive(Debug, Clone)]
pub enum CaCert {
    /// PEM file path — pins this specific CA (dev / self-signed gateways).
    Path(String),
    /// PEM bytes — pins this specific CA.
    Pem(Vec<u8>),
    /// Verify against the bundled public (Mozilla/webpki) root CAs. The
    /// production default: the gateway serves a publicly-trusted (Let's Encrypt)
    /// certificate, so no CA has to ship inside the app, and the dialled hostname
    /// is verified as its own SNI rather than the fixed dev server name.
    PublicRoots,
}

impl CaCert {
    /// The SDK's trust decision from the options a host passes. Explicit PEM bytes
    /// or a path pin that CA (dev, self-signed gateway); passing neither trusts
    /// the public root CAs — what a production gateway with a Let's Encrypt cert
    /// on a real hostname needs.
    pub fn from_opts(pem: Option<Vec<u8>>, path: Option<String>) -> Self {
        match (pem, path) {
            (Some(pem), _) => CaCert::Pem(pem),
            (None, Some(path)) => CaCert::Path(path),
            (None, None) => CaCert::PublicRoots,
        }
    }

    /// Whether verification uses the public roots. When it does, the client must
    /// present the real dialled hostname as SNI (the cert is issued for that
    /// name), not the fixed dev server name.
    pub fn is_public_roots(&self) -> bool {
        matches!(self, CaCert::PublicRoots)
    }

    /// The CA material as PEM bytes, reading the file if this is a `Path`. Only
    /// valid for the pinned variants; [`CaCert::PublicRoots`] has no PEM.
    pub fn pem_bytes(&self) -> Result<Vec<u8>> {
        match self {
            CaCert::Path(path) => std::fs::read(path)
                .with_context(|| format!("cannot read the gateway CA certificate at {path}")),
            CaCert::Pem(bytes) => Ok(bytes.clone()),
            CaCert::PublicRoots => {
                anyhow::bail!("PublicRoots trusts the system root CAs and has no PEM material")
            }
        }
    }
}

/// The production gateway, used when a host does not name one itself.
///
/// QUIC over UDP/4443. The gateway serves a publicly-trusted (Let's Encrypt)
/// certificate for this exact hostname, so the matching trust decision is
/// [`CaCert::PublicRoots`] — nothing has to ship with the host application.
pub const DEFAULT_GATEWAY_ADDRESSES: &[&str] = &["gw.meerkly.com:4443"];

/// How long [`start`](crate::ProxyClient::start) waits for the first connection.
pub const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to wait when dialling a target on the gateway's behalf.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// QUIC keepalive interval — a fallback, not a second heartbeat.
///
/// What actually keeps an idle exit's NAT mapping warm is the gateway's
/// application heartbeat: it pings every `heartbeat_secs` (10s by default,
/// announced in `ServerHello`) and the client pongs, so packets cross in both
/// directions all the time. quinn's keepalive timer is reset by every packet
/// *received*, so that exchange already suppresses this one — but only while
/// the heartbeat is strictly faster than it.
///
/// This was 10s, exactly the gateway's heartbeat, which made the suppression a
/// dead heat: the timer expiry and the next ping landed together, and which one
/// won was decided by the ACK that happened to follow each pong. Losing that
/// race costs a PING and its ACK on every idle cycle, roughly 35% on top of an
/// idle connection's floor, and it would have started happening silently the
/// first time anyone retuned `HEARTBEAT_INTERVAL_SECS`. At twice the heartbeat
/// the PING can only fire once the heartbeat has genuinely stopped.
///
/// Measured 2026-09-12 on an idle fleet: the floor is ~117 KB/hour per
/// connection, ~325 bytes per 10s cycle, which is the heartbeat exchange alone.
pub const DEFAULT_KEEP_ALIVE: Duration = Duration::from_secs(20);
/// How long silence is tolerated before the connection is considered dead.
///
/// Set here rather than derived from [`DEFAULT_KEEP_ALIVE`], which it used to be
/// (`keep_alive * 3`). That coupling meant the keepalive could not be retuned
/// without also moving failure detection, so the two are now independent
/// numbers that mean independent things. 30s matches what the gateway does from
/// its side: close after three unanswered heartbeats, and let the lease lapse.
pub const DEFAULT_MAX_IDLE: Duration = Duration::from_secs(30);
/// The binding reported when a host does not name one. A direct Rust consumer
/// is exactly that; every other binding overrides it.
pub const DEFAULT_SDK: &str = "rust";
/// Reconnect backoff bounds.
pub const DEFAULT_MIN_BACKOFF: Duration = Duration::from_millis(500);
pub const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The public publisher id the host provides — an identifier (not a secret)
    /// that the gateway resolves to the earning account. The SDK persists nothing,
    /// and there is no client-side device/machine id — the gateway assigns an
    /// ephemeral instance id per connection.
    pub publisher_id: String,
    /// Gateway `host:port` addresses, tried in rotation. Defaults to
    /// [`DEFAULT_GATEWAY_ADDRESSES`] via [`ClientConfig::new`]; set it explicitly
    /// to point at a development gateway. More than one is what makes a gateway
    /// failure survivable.
    pub gateway_addresses: Vec<String>,
    /// Self-reported country — a **dev/test affordance only**, deliberately not
    /// exposed by the SDK bindings. In production the gateway derives an exit's
    /// country authoritatively from its observed IP (a self-report is spoofable),
    /// so this is used only as a fallback when the gateway has no GeoIP database —
    /// e.g. the local smoke tests, whose exits sit on loopback IPs that have no
    /// resolvable geo. The Rust `meerkly-server-node` harness sets it from
    /// `MEERKLY_COUNTRY`; leave it `None` everywhere else.
    pub country: Option<String>,
    /// The certificate authority that signs gateway certificates.
    pub ca_cert: CaCert,
    /// How long `start()` waits for the first successful connection.
    pub start_timeout: Duration,
    /// How long to wait when dialling a target on the gateway's behalf.
    pub connect_timeout: Duration,
    /// QUIC keepalive interval. A fallback for a gateway heartbeat that has
    /// stopped, so it must stay slower than one — see [`DEFAULT_KEEP_ALIVE`].
    pub keep_alive: Duration,
    pub min_backoff: Duration,
    pub max_backoff: Duration,

    /// Stable identifier for this machine, if the host keeps one. The SDK never
    /// mints or stores it: only the host knows where an id belongs on its
    /// platform, and a library that invented one would hand every process a
    /// different "device". `None` leaves the exit anonymous in the dashboard.
    pub device_id: Option<String>,
    /// A human name for this machine, conventionally its hostname. Seeds the
    /// dashboard's label so a device is recognisable before anyone renames it.
    pub device_name: Option<String>,
    /// Which binding is speaking — "rust", "node", "python", … Defaults to
    /// [`DEFAULT_SDK`]. `client-napi` sets "node"; the uniffi languages set
    /// their own, because one crate serves all five and cannot tell them apart.
    pub sdk: Option<String>,
    /// The host application, e.g. "meerkly-agent/1.0.0" — distinct from the
    /// SDK's own version, which the SDK reports itself.
    pub app: Option<String>,
}

impl ClientConfig {
    /// A ready-to-run configuration for the production network: the public
    /// gateway, public-root TLS, and the timeouts every binding already uses.
    ///
    /// This is the whole setup a host needs — a publisher id and nothing else:
    ///
    /// ```no_run
    /// # use meerkly_sdk::{ClientConfig, ProxyClient};
    /// let client = ProxyClient::new(
    ///     ClientConfig::new("pub_..."),
    ///     tokio::runtime::Handle::current(),
    /// )?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// Every field stays public, so overriding one after construction — a dev
    /// gateway, a pinned CA — is just an assignment.
    pub fn new(publisher_id: impl Into<String>) -> Self {
        Self {
            publisher_id: publisher_id.into(),
            gateway_addresses: DEFAULT_GATEWAY_ADDRESSES
                .iter()
                .map(|a| (*a).to_owned())
                .collect(),
            country: None,
            ca_cert: CaCert::PublicRoots,
            start_timeout: DEFAULT_START_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            keep_alive: DEFAULT_KEEP_ALIVE,
            min_backoff: DEFAULT_MIN_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            device_id: None,
            device_name: None,
            sdk: None,
            app: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.publisher_id.trim().is_empty(),
            "publisherId must not be empty"
        );
        anyhow::ensure!(
            !self.gateway_addresses.is_empty(),
            "gatewayAddresses must contain at least one address"
        );
        for addr in &self.gateway_addresses {
            anyhow::ensure!(
                addr.contains(':'),
                "gateway address {addr:?} must be in host:port form"
            );
        }
        match &self.ca_cert {
            CaCert::Path(path) => {
                std::fs::metadata(path).with_context(|| {
                    format!(
                        "cannot read the gateway CA certificate at {path}; set caCertPath or \
                         MEERKLY_CA_CERT_PATH"
                    )
                })?;
            }
            CaCert::Pem(bytes) => {
                anyhow::ensure!(!bytes.is_empty(), "caCertPem must not be empty");
            }
            // Nothing to validate: the public roots ship with the binary.
            CaCert::PublicRoots => {}
        }
        Ok(())
    }
}

/// Default CA path, overridable by environment so a deployment does not have to
/// thread the path through application code.
pub fn default_ca_cert_path() -> String {
    std::env::var("MEERKLY_CA_CERT_PATH").unwrap_or_else(|_| "certs/dev/ca.crt".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of `new`: a publisher id alone is a valid, production-ready
    /// config. Until this existed, the documented one-argument quickstart failed
    /// validation because `gateway_addresses` was empty.
    #[test]
    fn new_is_valid_with_only_a_publisher_id() {
        let config = ClientConfig::new("pub_abc123");
        config.validate().expect("the default config must validate");
        assert_eq!(config.gateway_addresses, vec!["gw.meerkly.com:4443"]);
        assert!(config.ca_cert.is_public_roots());
        assert!(config.country.is_none());
    }

    /// The transport keepalive is a fallback for a heartbeat that has stopped,
    /// not a second heartbeat. It has to stay slower than the gateway's, or
    /// quinn's PING races the ping/pong exchange and every idle exit pays for a
    /// PING and an ACK per cycle — about 35% on top of a measured floor of
    /// ~117 KB/hour — while buying no liveness the heartbeat did not already
    /// provide.
    #[test]
    fn the_transport_keepalive_is_slower_than_the_gateway_heartbeat() {
        // The gateway's own HEARTBEAT_INTERVAL_SECS default, restated because the
        // gateway is deliberately not a dependency of the SDK. That is also why
        // the handshake warns on the value the gateway actually announces: this
        // constant can only encode the default, not the deployment.
        const GATEWAY_HEARTBEAT: Duration = Duration::from_secs(10);

        assert!(
            DEFAULT_KEEP_ALIVE > GATEWAY_HEARTBEAT,
            "a keepalive at the heartbeat's own interval is a dead heat, not suppression"
        );
        assert!(
            DEFAULT_MAX_IDLE > DEFAULT_KEEP_ALIVE,
            "silence must outlast one keepalive, or the PING never gets an answer"
        );
    }

    #[test]
    fn an_empty_publisher_id_is_rejected() {
        assert!(ClientConfig::new("   ").validate().is_err());
    }

    #[test]
    fn a_gateway_address_without_a_port_is_rejected() {
        let mut config = ClientConfig::new("pub_abc123");
        config.gateway_addresses = vec!["gw.meerkly.com".to_owned()];
        assert!(config.validate().is_err());
    }
}
