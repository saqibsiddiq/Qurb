//! Phase 0 networking spike.
//!
//!   quictest stun              -- discover public endpoint + classify NAT mapping
//!   quictest serve [bind]      -- accept a QUIC stream, measure receive throughput
//!   quictest send <addr> [MiB] -- connect and push data
//!
//! `stun` is the one that matters. It answers whether UDP hole punching can
//! work from this network at all, which decides how much traffic ends up on
//! paid relay infrastructure forever.

use anyhow::{bail, Context, Result};
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Public STUN servers, deliberately on two different operators. Comparing the
/// mapped port each returns is what distinguishes an endpoint-independent NAT
/// (hole punching works) from a symmetric one (it does not).
const STUN_SERVERS: [&str; 2] = ["stun.l.google.com:19302", "stun.cloudflare.com:3478"];

fn main() -> Result<()> {
    rustls::crypto::ring::default_provider().install_default().ok();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("stun") => stun_probe(),
        Some("serve") => {
            let bind = args.get(2).map_or("0.0.0.0:5000".to_string(), |s| s.clone());
            rt()?.block_on(serve(bind.parse()?))
        }
        Some("send") => {
            let addr: SocketAddr = args.get(2).context("usage: quictest send <addr> [MiB]")?.parse()?;
            let mib: u64 = args.get(3).map_or(Ok(256), |s| s.parse())?;
            rt()?.block_on(send(addr, mib))
        }
        _ => {
            eprintln!("usage: quictest stun | serve [bind] | send <addr> [MiB]");
            Ok(())
        }
    }
}

fn rt() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Runtime::new()?)
}

// ---------------------------------------------------------------------------
// STUN: minimal RFC 5389 binding request.
// ---------------------------------------------------------------------------

fn stun_probe() -> Result<()> {
    println!("=== NAT probe ===");
    println!("Sending STUN binding requests. This reveals your public IP to the");
    println!("servers below, exactly as any WebRTC or VPN client does.\n");

    // Reuse ONE local socket across both servers. That is the entire trick:
    // same local port, two destinations. If the NAT reports the same public
    // port both times, its mapping is endpoint-independent.
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_secs(3)))?;
    println!("local  {}\n", sock.local_addr()?);

    let mut mapped = Vec::new();
    for server in STUN_SERVERS {
        match stun_binding(&sock, server) {
            Ok(addr) => {
                println!("  {server:<28} -> {addr}");
                mapped.push(addr);
            }
            Err(e) => println!("  {server:<28} -> failed: {e}"),
        }
    }

    println!();
    match mapped.len() {
        0 => {
            println!("verdict  NO STUN RESPONSE");
            println!("         UDP may be blocked outbound. Every connection would");
            println!("         fall back to TCP/443 relay. Retest on another network.");
        }
        1 => println!("verdict  INCONCLUSIVE -- only one server answered, rerun"),
        _ => {
            let same_port = mapped.windows(2).all(|w| w[0].port() == w[1].port());
            let same_ip = mapped.windows(2).all(|w| w[0].ip() == w[1].ip());
            if same_port && same_ip {
                println!("verdict  ENDPOINT-INDEPENDENT MAPPING (cone NAT)");
                println!("         Hole punching should succeed. This is the good case;");
                println!("         expect a high direct-connection rate from this network.");
            } else {
                println!("verdict  SYMMETRIC NAT");
                println!("         Mapped port changes per destination, so hole punching");
                println!("         will usually fail here and traffic will need a relay.");
                println!("         One symmetric endpoint is survivable; two is not.");
            }
        }
    }
    println!("\nRun this on every network you care about: home, phone hotspot,");
    println!("office, cafe wifi. The worst result is the one that sets your relay bill.");
    Ok(())
}

fn stun_binding(sock: &UdpSocket, server: &str) -> Result<SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

    const MAGIC: u32 = 0x2112A442;
    let dest = server.to_socket_addrs()?.find(|a| a.is_ipv4()).context("no A record")?;

    let mut req = Vec::with_capacity(20);
    req.extend_from_slice(&0x0001u16.to_be_bytes()); // binding request
    req.extend_from_slice(&0u16.to_be_bytes()); // length
    req.extend_from_slice(&MAGIC.to_be_bytes());
    let txn: [u8; 12] = rand_txn();
    req.extend_from_slice(&txn);
    sock.send_to(&req, dest)?;

    let mut buf = [0u8; 512];
    let (n, _) = sock.recv_from(&mut buf)?;
    if n < 20 || u16::from_be_bytes([buf[0], buf[1]]) != 0x0101 {
        bail!("not a binding success response");
    }

    // Walk the TLV attributes looking for XOR-MAPPED-ADDRESS (0x0020).
    let mut i = 20;
    while i + 4 <= n {
        let atype = u16::from_be_bytes([buf[i], buf[i + 1]]);
        let alen = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let body = i + 4;
        if atype == 0x0020 && body + 8 <= n && buf[body + 1] == 0x01 {
            let port = u16::from_be_bytes([buf[body + 2], buf[body + 3]]) ^ (MAGIC >> 16) as u16;
            let raw = u32::from_be_bytes([buf[body + 4], buf[body + 5], buf[body + 6], buf[body + 7]]);
            let ip = Ipv4Addr::from(raw ^ MAGIC);
            return Ok(SocketAddr::new(IpAddr::V4(ip), port));
        }
        i = body + alen + ((4 - alen % 4) % 4); // attributes are 4-byte aligned
    }
    bail!("no XOR-MAPPED-ADDRESS in response")
}

fn rand_txn() -> [u8; 12] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut out = [0u8; 12];
    out.copy_from_slice(&n.to_le_bytes()[..12]);
    out
}

// ---------------------------------------------------------------------------
// QUIC throughput. Self-signed certs: the real engine authenticates peers with
// Noise IK over static Curve25519 keys, not the web PKI, so TLS here is just
// the transport's own requirement.
// ---------------------------------------------------------------------------


async fn serve(bind: SocketAddr) -> Result<()> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut sc = ServerConfig::with_single_cert(vec![cert_der], key_der)?;
    let mut tc = TransportConfig::default();
    tc.max_concurrent_uni_streams(64u32.into());
    sc.transport_config(Arc::new(tc));

    let endpoint = Endpoint::server(sc, bind)?;
    println!("listening on {}", endpoint.local_addr()?);

    while let Some(incoming) = endpoint.accept().await {
        let conn = incoming.await?;
        println!("\nconnection from {}", conn.remote_address());
        let mut recv = conn.accept_uni().await?;

        let t0 = Instant::now();
        let mut total = 0u64;
        let mut buf = vec![0u8; 256 * 1024];
        while let Some(n) = recv.read(&mut buf).await? {
            total += n as u64;
        }
        let secs = t0.elapsed().as_secs_f64();
        println!("received {} in {secs:.2}s -> {:.0} MiB/s",
            spike::human(total), (total as f64 / (1024.0 * 1024.0)) / secs);
        println!("rtt {:?}, lost {}", conn.rtt(), conn.stats().path.lost_packets);
    }
    Ok(())
}

async fn send(addr: SocketAddr, mib: u64) -> Result<()> {
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;

    let cc = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(cc)?,
    )));

    let t_conn = Instant::now();
    let conn = endpoint.connect(addr, "localhost")?.await?;
    println!("connected to {addr} in {:?}", t_conn.elapsed());

    let mut send = conn.open_uni().await?;
    let block = vec![0xABu8; 1 << 20];
    let t0 = Instant::now();
    for _ in 0..mib {
        send.write_all(&block).await?;
    }
    send.finish()?;
    conn.closed().await;

    let secs = t0.elapsed().as_secs_f64();
    println!("sent {} in {secs:.2}s -> {:.0} MiB/s",
        spike::human(mib << 20), mib as f64 / secs);
    Ok(())
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self, _: &rustls::pki_types::CertificateDer, _: &[rustls::pki_types::CertificateDer],
        _: &rustls::pki_types::ServerName, _: &[u8], _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self, _: &[u8], _: &rustls::pki_types::CertificateDer, _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self, _: &[u8], _: &rustls::pki_types::CertificateDer, _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}
