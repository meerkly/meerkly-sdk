//! QUIC/TLS setup for the client side.
//!
//! Two trust modes:
//!
//! * **Pinned** ([`CaCert::Path`]/[`CaCert::Pem`]) — verify against an explicit
//!   CA and use the fixed server name `meerkly-gateway`, never the dialled
//!   address. Keeps verification meaningful while letting a client reach a
//!   self-signed dev gateway through any DNS name, VIP, or raw IP.
//! * **Public roots** ([`CaCert::PublicRoots`]) — the production default: verify
//!   against the bundled Mozilla root CAs (the gateway serves a Let's Encrypt
//!   cert) and, at dial time, use the real hostname as SNI (see `connection.rs`).

use crate::config::CaCert;
use anyhow::{Context, Result};
use quinn::crypto::rustls::QuicClientConfig;
use quinn::ClientConfig;
use std::sync::Arc;
use std::time::Duration;

pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn client_config(ca_cert: &CaCert, keep_alive: Duration) -> Result<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    match ca_cert {
        CaCert::PublicRoots => {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        pinned => {
            let pem = pinned.pem_bytes()?;
            let mut added = 0usize;
            for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
                let cert = cert.context("the gateway CA is not a valid PEM chain")?;
                roots
                    .add(cert)
                    .context("the gateway CA contains an unusable certificate")?;
                added += 1;
            }
            anyhow::ensure!(added > 0, "the gateway CA contains no certificates");
        }
    }

    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![meerkly_protocol::ALPN.to_vec()];

    let mut config = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(crypto).context("TLS config is not usable for QUIC")?,
    ));

    let mut transport = quinn::TransportConfig::default();
    // Two independent numbers: how often to poke a path nothing else is using,
    // and how long silence is tolerated. The idle timeout used to be
    // `keep_alive * 3`, which meant one could not be changed without the other.
    transport.keep_alive_interval(Some(keep_alive));
    transport.max_idle_timeout(Some(
        crate::config::DEFAULT_MAX_IDLE
            .try_into()
            .context("idle timeout out of range")?,
    ));
    config.transport_config(Arc::new(transport));

    Ok(config)
}
