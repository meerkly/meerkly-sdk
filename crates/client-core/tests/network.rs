//! The host's transport reaches the gateway, and never a gateway that cannot
//! take it.
//!
//! The gateway classifies an exit as mobile, residential or datacenter partly
//! from the transport the host reports. It rides in the hello, and a change
//! under a live connection is sent as a control message, but only to a gateway
//! that said it understands one: an older gateway stops reading the control
//! stream at a frame it cannot parse, and the exit would be dropped.

use std::sync::Arc;
use std::time::Duration;

use meerkly_protocol::{
    read_frame, write_frame, ClientHello, ControlToGateway, ServerHello, ALPN, SERVER_NAME,
};
use meerkly_sdk::{CaCert, ClientConfig, ProxyClient};
use tokio::sync::mpsc;

/// What the mock gateway saw: the hello's `network`, then every control frame.
#[derive(Debug)]
enum Seen {
    Hello(Option<String>),
    Control(ControlToGateway),
}

/// A gateway on loopback that accepts every client, advertising
/// `network_updates` as told, and reports what it reads.
fn gateway(network_updates: bool) -> (String, Vec<u8>, mpsc::UnboundedReceiver<Seen>) {
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
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                let Ok((mut send, mut recv)) = connection.accept_bi().await else {
                    return;
                };
                let hello: ClientHello = read_frame(&mut recv).await.unwrap();
                let _ = tx.send(Seen::Hello(hello.network));
                write_frame(
                    &mut send,
                    &ServerHello {
                        accepted: true,
                        reason: None,
                        gateway_id: "test-gateway".to_owned(),
                        client_key: "acct:test".to_owned(),
                        heartbeat_secs: 15,
                        network_updates,
                    },
                )
                .await
                .unwrap();
                while let Ok(frame) = read_frame::<_, ControlToGateway>(&mut recv).await {
                    let _ = tx.send(Seen::Control(frame));
                }
                drop(connection);
            });
        }
    });

    (address, ca.pem().into_bytes(), rx)
}

async fn started_client(address: String, ca_pem: Vec<u8>, network: Option<&str>) -> ProxyClient {
    let mut config = ClientConfig::new("pub_test");
    config.gateway_addresses = vec![address];
    config.ca_cert = CaCert::Pem(ca_pem);
    config.start_timeout = Duration::from_secs(3);
    let client = ProxyClient::new(config, tokio::runtime::Handle::current()).unwrap();
    client.set_network(network.map(str::to_owned));
    client.start().await.unwrap();
    client
}

async fn next(rx: &mut mpsc::UnboundedReceiver<Seen>) -> Option<Seen> {
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hello_carries_the_network_and_changes_follow() {
    let (address, ca, mut seen) = gateway(true);
    let client = started_client(address, ca, Some(" WiFi ")).await;

    assert!(matches!(next(&mut seen).await, Some(Seen::Hello(Some(n))) if n == "wifi"));

    client.set_network(Some("cellular".to_owned()));
    match next(&mut seen).await {
        Some(Seen::Control(ControlToGateway::Network { network })) => {
            assert_eq!(network, "cellular")
        }
        other => panic!("expected a network update, got {other:?}"),
    }

    // Setting the same value again is not news.
    client.set_network(Some("cellular".to_owned()));
    // Losing it is, and is said as an empty string.
    client.set_network(None);
    match next(&mut seen).await {
        Some(Seen::Control(ControlToGateway::Network { network })) => assert_eq!(network, ""),
        other => panic!("expected a network update, got {other:?}"),
    }
    client.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gateway_without_network_updates_is_never_sent_one() {
    let (address, ca, mut seen) = gateway(false);
    let client = started_client(address, ca, Some("ethernet")).await;

    assert!(matches!(next(&mut seen).await, Some(Seen::Hello(Some(n))) if n == "ethernet"));
    client.set_network(Some("cellular".to_owned()));
    assert!(
        next(&mut seen).await.is_none(),
        "an update reached a gateway that did not ask for them"
    );
    assert_eq!(client.network().as_deref(), Some("cellular"));
    client.stop().await.unwrap();
}
