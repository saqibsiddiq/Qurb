//! Load for the rendezvous service and the relay. Throwaway: see README.md.
//!
//!   service-capacity signal <ws-url> <devices> [samples]
//!   service-capacity relay <host:port> <pairs> <mib-per-pair>
//!
//! Prints `KEY value` lines for the driver script to read, and waits on stdin
//! at the points where the driver measures the service process.

use anyhow::{bail, Context, Result};
use qurb_keys::MasterKey;
use qurb_signal::{Endpoints, FromServer, GroupId, MemberId, SignalClient};
use std::io::BufRead;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("signal") => {
            let url = args.get(2).context("url")?;
            let devices: usize = args.get(3).context("devices")?.parse()?;
            let samples: usize = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(200);
            signal(url, devices, samples).await
        }
        Some("relay-idle") => {
            let addr = args.get(2).context("relay address")?;
            let devices: usize = args.get(3).context("devices")?.parse()?;
            relay_idle(addr, devices).await
        }
        Some("relay") => {
            let addr = args.get(2).context("relay address")?;
            let pairs: usize = args.get(3).context("pairs")?.parse()?;
            let mib: usize = args.get(4).context("MiB per pair")?.parse()?;
            relay(addr, pairs, mib).await
        }
        _ => bail!("usage: service-capacity signal <url> <devices> [samples] | relay <addr> <pairs> <mib>"),
    }
}

/// Wait for the driver to say it has measured.
fn pause(label: &str) {
    println!("{label}");
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
}

fn somewhere(n: usize) -> Endpoints {
    // Plausible and unreachable: a service never dials what it is told.
    Endpoints {
        public: Some(format!("198.51.100.{}:{}", n % 250 + 1, 40_000 + n % 20_000).parse().unwrap()),
        local: vec![format!("192.168.1.{}:{}", n % 250 + 1, 40_000 + n % 20_000).parse().unwrap()],
    }
}

// -- the rendezvous service ---------------------------------------------------

/// `devices` connected and announced, in groups of two -- one person's phone
/// and laptop -- then a sample of introductions timed while all of them are
/// held open.
async fn signal(url: &str, devices: usize, samples: usize) -> Result<()> {
    let pairs = devices / 2;
    let started = Instant::now();
    let mut askers = Vec::with_capacity(pairs);

    // A few hundred at a time: a thundering herd of handshakes measures the
    // listen backlog rather than the service.
    let mut launched = 0;
    while launched < pairs {
        let batch: Vec<_> = (launched..(launched + 200).min(pairs))
            .map(|pair| {
                let url = url.to_string();
                async move {
                    // Fresh for every run, as real devices are: the same seed
                    // each run met the last run's connections still
                    // registered under the same identity, and half the
                    // introductions went to them (2026-11-01).
                    let mut seed = [0u8; 32];
                    seed[..8].copy_from_slice(&(pair as u64).to_le_bytes());
                    seed[8..24].copy_from_slice(&run_nonce());
                    let master = MasterKey::from_bytes(seed);
                    let group = GroupId::derive(&master);
                    let a = MemberId::derive(&master, &[1; 32]);
                    let b = MemberId::derive(&master, &[2; 32]);
                    // `wss://…#fingerprint` goes through the pinned TLS path a
                    // device uses for a server with no domain name.
                    let connect = |member, n| {
                        let url = url.clone();
                        async move {
                            if url.starts_with("wss://") {
                                SignalClient::connect(&url, group, member, somewhere(n)).await
                            } else {
                                SignalClient::connect_insecure(&url, group, member, somewhere(n)).await
                            }
                        }
                    };
                    let asker = connect(a, pair * 2).await?;
                    let answerer = connect(b, pair * 2 + 1).await?;
                    Ok::<_, anyhow::Error>((asker, answerer, b))
                }
            })
            .collect();
        for joined in futures_join(batch).await {
            let (asker, answerer, b) = joined?;
            tokio::spawn(answer(answerer));
            askers.push((asker, b));
        }
        launched = (launched + 200).min(pairs);
    }
    println!("CONNECTED {} {:.2}", pairs * 2, started.elapsed().as_secs_f64());
    if let Some(ms) = std::env::var("SETTLE_MS").ok().and_then(|v| v.parse().ok()) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    pause("MEASURE_HELD");

    // Introductions, one at a time, while everyone else stays connected.
    let step = (pairs / samples.max(1)).max(1);
    let mut timings = Vec::new();
    for (asker, peer) in askers.iter_mut().step_by(step).take(samples) {
        let t = Instant::now();
        asker.connect_to(*peer)?;
        loop {
            match tokio::time::timeout(Duration::from_secs(10), asker.next()).await {
                Ok(Some(FromServer::Punch { .. })) => break,
                Ok(Some(_)) => continue,
                _ => bail!("an introduction did not complete"),
            }
        }
        timings.push(t.elapsed());
    }
    timings.sort();
    let at = |q: f64| timings[((timings.len() as f64 - 1.0) * q) as usize].as_secs_f64() * 1000.0;
    println!("INTRODUCTIONS {} p50_ms {:.2} p95_ms {:.2} max_ms {:.2}", timings.len(), at(0.5), at(0.95), at(1.0));
    pause("MEASURE_AFTER");
    Ok(())
}

/// Sixteen bytes that differ between runs.
fn run_nonce() -> [u8; 16] {
    use std::sync::OnceLock;
    static NONCE: OnceLock<[u8; 16]> = OnceLock::new();
    *NONCE.get_or_init(|| {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let mut out = [0u8; 16];
        out.copy_from_slice(&(now ^ ((std::process::id() as u128) << 64)).to_le_bytes());
        out
    })
}

/// The other half of every introduction: agree to be reached.
async fn answer(mut client: SignalClient) {
    while let Some(message) = client.next().await {
        if let FromServer::ConnectRequest { from, .. } = message {
            let _ = client.accept(from, somewhere(7));
        }
    }
}

/// Every future in `batch`, concurrently, in order.
async fn futures_join<T: Send + 'static>(
    batch: Vec<impl std::future::Future<Output = T> + Send + 'static>,
) -> Vec<T> {
    let handles: Vec<_> = batch.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        out.push(handle.await.expect("task"));
    }
    out
}

// -- the relay ---------------------------------------------------------------

/// `devices` registered with the relay and sending nothing: what a relay holds
/// for every device that is on and configured to fall back to it.
async fn relay_idle(addr: &str, devices: usize) -> Result<()> {
    let relay: std::net::SocketAddr = addr.parse()?;
    let started = Instant::now();
    let mut held = Vec::with_capacity(devices);
    for n in 0..devices {
        let mut id = [0xC0u8; 32];
        id[..8].copy_from_slice(&(n as u64).to_le_bytes());
        held.push(qurb_relay::RelaySocket::connect(relay, id).await?);
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("REGISTERED {devices} {:.2}", started.elapsed().as_secs_f64());
    pause("MEASURE_HELD");
    drop(held);
    Ok(())
}

/// `pairs` QUIC sessions through the relay at once, each carrying `mib` MiB on
/// one stream: what a sync through the relay is, minus the sync.
async fn relay(addr: &str, pairs: usize, mib: usize) -> Result<()> {
    rustls::crypto::ring::default_provider().install_default().ok();
    let relay: std::net::SocketAddr = addr.parse()?;
    let payload = Arc::new(vec![0x5Au8; mib << 20]);

    let (done_tx, mut done_rx) = mpsc::unbounded_channel();
    let mut ready = Vec::new();
    for pair in 0..pairs {
        let alice = [0xA0, pair as u8, (pair >> 8) as u8].iter().cycle().take(32).copied().collect::<Vec<_>>();
        let bob = [0xB0, pair as u8, (pair >> 8) as u8].iter().cycle().take(32).copied().collect::<Vec<_>>();
        let (alice, bob): ([u8; 32], [u8; 32]) = (alice.try_into().unwrap(), bob.try_into().unwrap());

        let bob_socket = qurb_relay::RelaySocket::connect(relay, bob).await?;
        let listener = qurb_relay::endpoint_over(bob_socket, Some(server_config()))?;
        let done = done_tx.clone();
        let expected = payload.len();
        tokio::spawn(async move {
            let connection = listener.accept().await.unwrap().await.unwrap();
            let mut stream = connection.accept_uni().await.unwrap();
            let received = stream.read_to_end(expected + 1).await.unwrap();
            let _ = done.send(received.len());
        });

        let alice_socket = qurb_relay::RelaySocket::connect(relay, alice).await?;
        let to_bob = alice_socket.address_for(bob)?;
        let dialler = qurb_relay::endpoint_over(alice_socket, None)?;
        let (go_tx, go_rx) = oneshot::channel::<()>();
        let payload = Arc::clone(&payload);
        tokio::spawn(async move {
            let connection =
                dialler.connect_with(client_config(), to_bob, "qurb-device").unwrap().await.unwrap();
            let _ = go_rx.await;
            let mut stream = connection.open_uni().await.unwrap();
            stream.write_all(&payload).await.unwrap();
            stream.finish().unwrap();
            // Held until the reader has everything, or the connection closes
            // under the last packets.
            let _ = stream.stopped().await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            drop(connection);
        });
        ready.push(go_tx);
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    pause("MEASURE_IDLE");

    let started = Instant::now();
    for go in ready {
        let _ = go.send(());
    }
    let mut total = 0usize;
    for _ in 0..pairs {
        total += tokio::time::timeout(Duration::from_secs(600), done_rx.recv())
            .await
            .context("a transfer stalled")?
            .context("a reader went away")?;
    }
    let seconds = started.elapsed().as_secs_f64();
    if total != pairs * payload.len() {
        bail!("{total} bytes arrived of {}", pairs * payload.len());
    }
    println!(
        "RELAYED pairs {pairs} bytes {total} seconds {seconds:.2} mib_per_s {:.1}",
        total as f64 / (1 << 20) as f64 / seconds
    );
    pause("MEASURE_AFTER");
    Ok(())
}

fn credentials() -> (Vec<rustls::pki_types::CertificateDer<'static>>, rustls::pki_types::PrivateKeyDer<'static>) {
    let generated = rcgen::generate_simple_self_signed(vec!["qurb-device".into()]).unwrap();
    let cert = rustls::pki_types::CertificateDer::from(generated.cert.der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::try_from(generated.key_pair.serialize_der()).unwrap();
    (vec![cert], key)
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
    tls.alpn_protocols = vec![b"load".to_vec()];
    quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap(),
    ))
}

/// Accepts any certificate: this measures carrying bytes, not identity, which
/// the real code checks and the relay never sees either way.
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
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn client_config() -> quinn::ClientConfig {
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(provider())))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"load".to_vec()];
    quinn::ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap()))
}
