//! A QUIC session carried by the relay.
//!
//! The claim being tested is that the relay is safe to hand traffic to: it
//! forwards datagrams, an ordinary QUIC session runs inside, and the relay has
//! no key for any of it. If that works, everything built on top of QUIC —
//! pinned identity, the wire protocol, chunk transfer — works over the relay
//! without knowing it is there.

use qurb_relay::{endpoint_over, Frame, RelayServer, RelaySocket};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const ALICE: [u8; 32] = [0xA1; 32];
const BOB: [u8; 32] = [0xB2; 32];

async fn relay() -> (Arc<RelayServer>, std::net::SocketAddr) {
    let server = Arc::new(RelayServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let addr = server.local_addr().unwrap();
    let running = Arc::clone(&server);
    tokio::spawn(async move { running.serve().await });
    (server, addr)
}

/// A self-signed certificate, standing in for a real device identity.
fn credentials() -> (
    Vec<rustls::pki_types::CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
) {
    let generated = rcgen::generate_simple_self_signed(vec!["qurb-device".into()]).unwrap();
    let cert = rustls::pki_types::CertificateDer::from(generated.cert.der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::try_from(generated.key_pair.serialize_der())
        .unwrap();
    (vec![cert], key)
}

#[derive(Debug)]
struct AcceptAny(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer,
        _: &[rustls::pki_types::CertificateDer],
        _: &rustls::pki_types::ServerName,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("tls 1.2".into()))
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn server_config() -> quinn::ServerConfig {
    let (chain, key) = credentials();
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .unwrap();
    tls.alpn_protocols = vec![b"qurb/0".to_vec()];
    quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap(),
    ))
}

fn client_config() -> quinn::ClientConfig {
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(provider())))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"qurb/0".to_vec()];
    quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_quic_session_runs_through_the_relay() {
    rustls::crypto::ring::default_provider().install_default().ok();
    let (server, addr) = relay().await;

    // Bob listens, through the relay.
    let bob_socket = RelaySocket::connect(addr, BOB).await.unwrap();
    let bob = endpoint_over(bob_socket, Some(server_config())).unwrap();

    let listening = tokio::spawn(async move {
        let incoming = bob.accept().await.expect("a connection");
        let connection = incoming.await.expect("handshake");
        let mut stream = connection.accept_uni().await.expect("stream");
        stream.read_to_end(64 * 1024).await.expect("read")
    });

    // Give Bob a moment to register before Alice starts sending.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice dials, through the relay.
    let alice_socket = RelaySocket::connect(addr, ALICE).await.unwrap();
    let bob_address = alice_socket.address_for(BOB).unwrap();
    let alice = endpoint_over(alice_socket, None).unwrap();

    let connection = alice
        .connect_with(client_config(), bob_address, "qurb-device")
        .unwrap()
        .await
        .expect("connect through the relay");

    let mut stream = connection.open_uni().await.unwrap();
    stream.write_all(b"carried by a relay that cannot read it").await.unwrap();
    stream.finish().unwrap();

    let received = tokio::time::timeout(Duration::from_secs(10), listening)
        .await
        .expect("the transfer stalled")
        .expect("listener panicked");

    assert_eq!(received, b"carried by a relay that cannot read it");
    assert!(server.stats().forwarded() > 0, "nothing went through the relay");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_transfer_survives_the_relay() {
    // A handshake crossing is one thing; sustained traffic with congestion
    // control, acknowledgements and retransmission is another.
    rustls::crypto::ring::default_provider().install_default().ok();
    let (server, addr) = relay().await;

    let bob_socket = RelaySocket::connect(addr, BOB).await.unwrap();
    let bob = endpoint_over(bob_socket, Some(server_config())).unwrap();

    let listening = tokio::spawn(async move {
        let connection = bob.accept().await.unwrap().await.unwrap();
        let mut stream = connection.accept_uni().await.unwrap();
        stream.read_to_end(8 << 20).await.unwrap()
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let alice_socket = RelaySocket::connect(addr, ALICE).await.unwrap();
    let bob_address = alice_socket.address_for(BOB).unwrap();
    let alice = endpoint_over(alice_socket, None).unwrap();
    let connection = alice
        .connect_with(client_config(), bob_address, "qurb-device")
        .unwrap()
        .await
        .unwrap();

    let payload: Vec<u8> = (0..(4usize << 20)).map(|i| (i * 31) as u8).collect();
    let mut stream = connection.open_uni().await.unwrap();
    stream.write_all(&payload).await.unwrap();
    stream.finish().unwrap();

    let received = tokio::time::timeout(Duration::from_secs(60), listening)
        .await
        .expect("the transfer stalled")
        .unwrap();

    assert_eq!(received.len(), payload.len());
    assert_eq!(received, payload, "the relay corrupted the stream");
    println!("relayed {} bytes in {} frames", server.stats().bytes(), server.stats().forwarded());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_relay_never_sees_the_content() {
    // The property the whole design rests on. Distinctive plaintext goes in;
    // nothing resembling it may appear in what the relay handles.
    rustls::crypto::ring::default_provider().install_default().ok();

    // A relay that keeps a copy of everything it forwards, so the test can look.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

    let watching = Arc::clone(&seen);
    tokio::spawn(async move {
        let server = RelayServerWithTap { listener, seen: watching };
        server.serve().await;
    });

    let bob_socket = RelaySocket::connect(addr, BOB).await.unwrap();
    let bob = endpoint_over(bob_socket, Some(server_config())).unwrap();
    let listening = tokio::spawn(async move {
        let connection = bob.accept().await.unwrap().await.unwrap();
        let mut stream = connection.accept_uni().await.unwrap();
        stream.read_to_end(64 * 1024).await.unwrap()
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let alice_socket = RelaySocket::connect(addr, ALICE).await.unwrap();
    let bob_address = alice_socket.address_for(BOB).unwrap();
    let alice = endpoint_over(alice_socket, None).unwrap();
    let connection = alice
        .connect_with(client_config(), bob_address, "qurb-device")
        .unwrap()
        .await
        .unwrap();

    const SECRET: &[u8] = b"CANARY-the-relay-must-never-see-this-plaintext";
    let mut stream = connection.open_uni().await.unwrap();
    stream.write_all(SECRET).await.unwrap();
    stream.finish().unwrap();

    let received = tokio::time::timeout(Duration::from_secs(10), listening).await.unwrap().unwrap();
    assert_eq!(received, SECRET, "setup: the message should have arrived");

    let observed = seen.lock().unwrap();
    assert!(!observed.is_empty(), "the tap saw nothing; the test proves nothing");
    assert!(
        observed.windows(SECRET.len()).all(|w| w != SECRET),
        "the plaintext passed through the relay in the clear"
    );
}

type Registry =
    Arc<std::sync::Mutex<std::collections::HashMap<[u8; 32], tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>;

/// A relay that records every byte it forwards, so a test can inspect it.
struct RelayServerWithTap {
    listener: tokio::net::TcpListener,
    seen: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl RelayServerWithTap {
    async fn serve(&self) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::sync::mpsc;

        let registry: Registry = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        while let Ok((stream, _)) = self.listener.accept().await {
            let registry = Arc::clone(&registry);
            let seen = Arc::clone(&self.seen);
            tokio::spawn(async move {
                let _ = stream.set_nodelay(true);
                let (mut reader, mut writer) = stream.into_split();
                let (outbox, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
                tokio::spawn(async move {
                    while let Some(bytes) = outgoing.recv().await {
                        if writer.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                });

                let mut identity: Option<[u8; 32]> = None;
                let mut length = [0u8; 4];
                let mut body = vec![0u8; 64 * 1024];
                loop {
                    if reader.read_exact(&mut length).await.is_err() {
                        break;
                    }
                    let size = u32::from_be_bytes(length) as usize;
                    if size == 0 || size > body.len() {
                        break;
                    }
                    if reader.read_exact(&mut body[..size]).await.is_err() {
                        break;
                    }
                    match Frame::decode(&body[..size]) {
                        Ok(Frame::Register { member }) => {
                            registry.lock().unwrap().insert(member, outbox.clone());
                            identity = Some(member);
                        }
                        Ok(Frame::Forward { to, payload }) => {
                            seen.lock().unwrap().extend_from_slice(&payload);
                            let from = identity.unwrap_or([0; 32]);
                            let target = registry.lock().unwrap().get(&to).cloned();
                            if let Some(target) = target {
                                let _ = target.send(Frame::Deliver { from, payload }.encode());
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_never_registers_cannot_forward() {
    // Otherwise the relay is an open proxy for anyone who finds the port.
    use tokio::io::AsyncWriteExt;
    let (server, addr) = relay().await;

    let victim = RelaySocket::connect(addr, BOB).await.unwrap();
    let _keep = Arc::clone(&victim);

    let mut anonymous = tokio::net::TcpStream::connect(addr).await.unwrap();
    let frame = Frame::Forward { to: BOB, payload: b"unsolicited".to_vec() }.encode();
    anonymous.write_all(&frame).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(server.stats().forwarded(), 0, "an unregistered client forwarded a frame");
    assert!(server.stats().dropped() > 0, "the frame was not counted as dropped");
}

#[tokio::test(flavor = "multi_thread")]
async fn rubbish_does_not_bring_the_relay_down() {
    use tokio::io::AsyncWriteExt;
    let (server, addr) = relay().await;

    let mut hostile = tokio::net::TcpStream::connect(addr).await.unwrap();
    // An absurd length, which without a cap is an out-of-memory attack.
    hostile.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    hostile.write_all(&[0xFF; 64]).await.unwrap();
    drop(hostile);

    let mut junk = tokio::net::TcpStream::connect(addr).await.unwrap();
    junk.write_all(&[0u8; 4]).await.unwrap();
    drop(junk);

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Still serving.
    let socket = RelaySocket::connect(addr, ALICE).await;
    assert!(socket.is_ok(), "the relay stopped working after junk input");
    assert!(server.stats().connections.load(Ordering::Relaxed) >= 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frame_for_a_device_that_is_not_there_is_dropped() {
    // As a router would. This carries datagrams, and QUIC already copes with
    // losing them.
    let (server, addr) = relay().await;
    let socket = RelaySocket::connect(addr, ALICE).await.unwrap();
    let nowhere = socket.address_for(BOB).unwrap();

    use quinn::AsyncUdpSocket;
    let _ = socket.try_send(&quinn::udp::Transmit {
        destination: nowhere,
        ecn: None,
        contents: b"into the void",
        segment_size: None,
        src_ip: None,
    });

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(server.stats().forwarded(), 0);
    assert!(server.stats().dropped() > 0);
}
