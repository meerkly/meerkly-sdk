//! A gateway's refusal reason reaches the host.
//!
//! The gateway explains a refused handshake in words a person can act on
//! ("another device is already connected from this IP address"). Before
//! `last_rejection` the client only logged it, and a host app could say no
//! more than "not connected".

use std::sync::Arc;
use std::time::Duration;

use meerkly_sdk::{CaCert, ClientConfig, ProxyClient};
use meerkly_protocol::{read_frame, write_frame, ClientHello, ServerHello, ALPN, SERVER_NAME};

const REASON: &str = "another device is already connected from this IP address";

/// A one-trick gateway on loopback: refuses every client with [`REASON`].
/// Returns its address and the CA a client must pin to reach it.
fn refusing_gateway() -> (String, Vec<u8>) {
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let mut ca_params = rcgen::CertificateParams::new(vec![]).unwrap();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca = ca_params.self_signed(&ca_key).unwrap();

    let leaf_key = rcgen::KeyPair::generate().unwrap();
    let leaf = rcgen::CertificateParams::new(vec![SERVER_NAME.to_owned()])
        .unwrap()
        .signed_by(&leaf_key, &ca, &ca_key)
        .unwrap();

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into()),
        )
        .unwrap();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap(),
    ));
    let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint.local_addr().unwrap().to_string();

    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                let Ok((mut send, mut recv)) = connection.accept_bi().await else { return };
                let _: ClientHello = read_frame(&mut recv).await.unwrap();
                write_frame(
                    &mut send,
                    &ServerHello {
                        accepted: false,
                        reason: Some(REASON.to_owned()),
                        gateway_id: "test-gateway".to_owned(),
                        client_key: String::new(),
                        heartbeat_secs: 15,
                        network_updates: false,
                    },
                )
                .await
                .unwrap();
                let _ = send.finish();
                // Let the frame land before the connection goes.
                tokio::time::sleep(Duration::from_millis(200)).await;
                connection.close(1u32.into(), b"address in use");
            });
        }
    });

    (address, ca.pem().into_bytes())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_client_reports_the_gateways_reason() {
    let (address, ca_pem) = refusing_gateway();

    let mut config = ClientConfig::new("pub_test");
    config.gateway_addresses = vec![address];
    config.ca_cert = CaCert::Pem(ca_pem);
    config.start_timeout = Duration::from_secs(2);
    config.min_backoff = Duration::from_millis(50);
    config.max_backoff = Duration::from_millis(200);

    let client = ProxyClient::new(config, tokio::runtime::Handle::current()).unwrap();
    assert_eq!(client.last_rejection(), None, "nothing refused yet");

    let err = client.start().await.expect_err("the gateway refuses every client");
    assert!(
        format!("{err:#}").contains(REASON),
        "start() should say why, got: {err:#}"
    );
    assert_eq!(client.last_rejection().as_deref(), Some(REASON));
}
