//! Wire messages exchanged between gateways and exit-node clients, and between
//! gateways.
//!
//! Three conversations use these types:
//!
//! * **Control stream** — one per QUIC connection, opened by the client. Carries
//!   the handshake and then heartbeats for the connection's lifetime.
//! * **Data stream** — one per proxied connection, opened by the gateway. Carries
//!   a single [`OpenTarget`]/[`TargetOpened`] exchange and then raw bytes.
//! * **Relay connection** — plain TCP between two gateways. Carries a single
//!   [`RelayOpen`]/[`RelayOpened`] exchange and then raw bytes.

use serde::{Deserialize, Serialize};

/// ALPN protocol identifier for the client-to-gateway QUIC connection.
pub const ALPN: &[u8] = b"meerkly/1";

/// Server name clients use when verifying a gateway's certificate. Gateways are
/// dialled by many different hostnames (a per-replica DNS name, a raw IP), so the
/// certificate identity is fixed rather than derived from the dial address.
pub const SERVER_NAME: &str = "meerkly-gateway";

/// First frame on the control stream: the client presenting its publisher id
/// and describing itself.
///
/// Every `device_*`/`sdk`/`app` field is **optional and self-reported**, and is
/// used only to label and group exits in the dashboard. They are as spoofable as
/// [`ClientHello::country`], so nothing that decides money may key off them:
/// earnings accrue to the account behind [`ClientHello::publisher_id`], from the
/// usage stream, never to a device.
///
/// A device id does **not** replace the gateway's per-connection instance id.
/// The gateway still mints a fresh one for every connection and derives
/// [`client_key`] from it, so two concurrent connections from one machine stay
/// distinguishable instead of displacing each other in the registry.
///
/// Derives `Default` so a caller sets only the fields it has and spreads the
/// rest (`..Default::default()`). Adding a field here should never be a
/// breaking change for the callers that do not populate it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientHello {
    /// The public publisher id the exit node presents — an identifier, not a
    /// secret (it ships inside apps and is shared as a QR code). The gateway
    /// resolves it to the owning account; an unresolvable id is rejected.
    pub publisher_id: String,
    pub client_version: String,
    /// Self-reported ISO 3166-1 alpha-2 country — a dev/test fallback only, not
    /// exposed by the SDK bindings. The gateway derives an exit's country from its
    /// observed IP (a self-report is spoofable) and uses this only when it has no
    /// GeoIP database, e.g. local smoke tests on loopback IPs. `None` in production.
    #[serde(default)]
    pub country: Option<String>,

    /// Stable identifier for the machine, minted and stored by the *host*, not
    /// by the SDK — a CLI and a phone persist it in different places, and only
    /// the host knows which. `None` from any client that keeps no state.
    #[serde(default)]
    pub device_id: Option<String>,
    /// OS family, from `std::env::consts::OS` — "linux", "windows", "macos",
    /// later "android"/"ios". A free string, not an enum, so a new platform
    /// needs no protocol change.
    #[serde(default)]
    pub device_os: Option<String>,
    /// CPU architecture, from `std::env::consts::ARCH`.
    #[serde(default)]
    pub device_arch: Option<String>,
    /// The machine's hostname. Seeds the dashboard label, so a device is
    /// recognisable before anyone renames it.
    #[serde(default)]
    pub device_name: Option<String>,
    /// Which binding is speaking: "rust", "node", "python", … Set automatically
    /// where the binding knows (napi is always "node"), host-supplied otherwise:
    /// `client-ffi` is one crate serving Python, Ruby, Go, Swift and Kotlin
    /// through uniffi and cannot tell which language called it.
    #[serde(default)]
    pub sdk: Option<String>,
    /// The host application, e.g. "meerkly-agent/1.0.0". Distinct from
    /// [`ClientHello::client_version`], which is the SDK's own version.
    #[serde(default)]
    pub app: Option<String>,
    /// The transport the host is on right now: "cellular", "wifi", "ethernet"
    /// or "other". Host-supplied (only the host can ask its OS), `None` when it
    /// does not know. The gateway combines it with the measured egress network
    /// to classify the exit as mobile, residential or datacenter. Self-reported,
    /// so it can only ever refine a measurement, never override one: a hosting
    /// network is `datacenter` whatever this says.
    #[serde(default)]
    pub network: Option<String>,
}

/// The gateway's reply to [`ClientHello`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    pub accepted: bool,
    /// Populated when `accepted` is false.
    #[serde(default)]
    pub reason: Option<String>,
    pub gateway_id: String,
    pub client_key: String,
    /// How often the gateway will send [`ControlToClient::Ping`].
    pub heartbeat_secs: u64,
    /// Whether this gateway understands [`ControlToGateway::Network`]. A client
    /// must not send that message otherwise: a gateway older than it stops
    /// reading the control stream at the first frame it cannot parse, which
    /// starves its heartbeat accounting and drops the exit. Absent (false) from
    /// every gateway that predates it.
    #[serde(default)]
    pub network_updates: bool,
}

/// Gateway to client, on the control stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlToClient {
    Ping { nonce: u64 },
    /// The gateway is going away; the client should reconnect without waiting for
    /// a transport-level failure.
    Shutdown { reason: String },
    /// A message from a newer gateway. Ignored, so a gateway can add messages
    /// without dropping every client that predates them.
    #[serde(other)]
    Unknown,
}

/// Client to gateway, on the control stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlToGateway {
    Pong { nonce: u64 },
    /// The host's transport changed (see [`ClientHello::network`]). Only sent
    /// when [`ServerHello::network_updates`] was true.
    Network { network: String },
    /// A message from a newer client. Ignored, for the same reason as
    /// [`ControlToClient::Unknown`].
    #[serde(other)]
    Unknown,
}

/// First frame on a data stream: the gateway asking the client to dial a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenTarget {
    pub host: String,
    pub port: u16,
}

/// The client's reply to [`OpenTarget`]. On `ok`, raw bytes follow immediately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetOpened {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// The client's local socket address for the outbound connection. Useful for
    /// debugging which exit the traffic actually took.
    #[serde(default)]
    pub local_addr: Option<String>,
}

/// First frame on a gateway-to-gateway relay connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayOpen {
    pub client_key: String,
    pub host: String,
    pub port: u16,
    /// The cluster's shared internal secret, proving this frame came from a peer
    /// gateway rather than from whoever reached the port.
    ///
    /// Optional on the wire so a peer running an older build still parses, and
    /// so the dev data plane (which configures no secret) needs no special case.
    /// The receiving gateway decides whether a missing one is acceptable — when
    /// it has a secret configured, it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// The owning gateway's reply to [`RelayOpen`]. On `ok`, raw bytes follow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayOpened {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// Build the Redis-facing identity of a connected exit node: the routing handle
/// passed between gateways. Keyed on the resolved account, not the presented
/// publisher id, so all of an account's exits share one namespace.
pub fn client_key(account: &str, installation_id: &str) -> String {
    format!("{account}:{installation_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_messages_tag_on_type() {
        let json = serde_json::to_string(&ControlToClient::Ping { nonce: 7 }).unwrap();
        assert_eq!(json, r#"{"type":"ping","nonce":7}"#);

        let back: ControlToGateway =
            serde_json::from_str(r#"{"type":"pong","nonce":7}"#).unwrap();
        assert!(matches!(back, ControlToGateway::Pong { nonce: 7 }));
    }

    /// The compatibility contract that lets a fleet upgrade in any order: an
    /// old client's hello, which carries none of the device fields, must still
    /// deserialise — the exit simply has no identity.
    #[test]
    fn a_hello_without_device_fields_still_parses() {
        let hello: ClientHello = serde_json::from_str(
            r#"{"publisher_id":"pub_abc","client_version":"0.4.0"}"#,
        )
        .unwrap();
        assert_eq!(hello.publisher_id, "pub_abc");
        assert!(hello.device_id.is_none());
        assert!(hello.device_os.is_none());
        assert!(hello.device_arch.is_none());
        assert!(hello.device_name.is_none());
        assert!(hello.sdk.is_none());
        assert!(hello.app.is_none());
    }

    /// The other direction: a new client's hello reaching an older gateway.
    /// Nothing in this crate sets `deny_unknown_fields`, so unknown fields are
    /// ignored rather than fatal — asserted here because losing that property
    /// would force every gateway and client to deploy in lockstep.
    #[test]
    fn a_hello_with_unknown_fields_is_not_rejected() {
        let hello: ClientHello = serde_json::from_str(
            r#"{"publisher_id":"pub_abc","client_version":"0.5.0","invented_later":"x"}"#,
        )
        .unwrap();
        assert_eq!(hello.publisher_id, "pub_abc");
    }

    #[test]
    fn device_fields_round_trip() {
        let hello = ClientHello {
            publisher_id: "pub_abc".to_owned(),
            client_version: "0.5.0".to_owned(),
            country: None,
            device_id: Some("dev_1234".to_owned()),
            device_os: Some("linux".to_owned()),
            device_arch: Some("aarch64".to_owned()),
            device_name: Some("prod-fra-01".to_owned()),
            sdk: Some("rust".to_owned()),
            app: Some("meerkly-agent/1.0.0".to_owned()),
            network: Some("cellular".to_owned()),
        };
        let back: ClientHello = serde_json::from_str(&serde_json::to_string(&hello).unwrap()).unwrap();
        assert_eq!(back.device_id.as_deref(), Some("dev_1234"));
        assert_eq!(back.device_os.as_deref(), Some("linux"));
        assert_eq!(back.device_arch.as_deref(), Some("aarch64"));
        assert_eq!(back.device_name.as_deref(), Some("prod-fra-01"));
        assert_eq!(back.sdk.as_deref(), Some("rust"));
        assert_eq!(back.app.as_deref(), Some("meerkly-agent/1.0.0"));
        assert_eq!(back.network.as_deref(), Some("cellular"));
    }

    /// An unknown control message must parse to `Unknown` rather than fail:
    /// either reader stops at the first frame it cannot parse.
    #[test]
    fn unknown_control_messages_are_tolerated() {
        let g: ControlToGateway =
            serde_json::from_str(r#"{"type":"invented_later","x":1}"#).unwrap();
        assert!(matches!(g, ControlToGateway::Unknown));
        let c: ControlToClient = serde_json::from_str(r#"{"type":"invented_later"}"#).unwrap();
        assert!(matches!(c, ControlToClient::Unknown));

        let n = serde_json::to_string(&ControlToGateway::Network {
            network: "wifi".to_owned(),
        })
        .unwrap();
        assert_eq!(n, r#"{"type":"network","network":"wifi"}"#);
    }

    /// A gateway that predates `network_updates` must read as not accepting them.
    #[test]
    fn server_hello_without_network_updates_means_false() {
        let h: ServerHello = serde_json::from_str(
            r#"{"accepted":true,"gateway_id":"g","client_key":"k","heartbeat_secs":10}"#,
        )
        .unwrap();
        assert!(!h.network_updates);
    }

    #[test]
    fn optional_fields_default_when_absent() {
        let opened: TargetOpened = serde_json::from_str(r#"{"ok":true}"#).unwrap();
        assert!(opened.ok);
        assert!(opened.error.is_none());
        assert!(opened.local_addr.is_none());
    }
}
