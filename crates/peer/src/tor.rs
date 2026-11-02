//! Tor as the slow last resort (decision 0082): a phone that cannot reach its
//! computer directly -- a home connection behind a shared address that lets
//! nothing in, and no IPv6 -- reaches it through the computer's onion service.
//!
//! Tor carries streams and qurb speaks QUIC, which is datagrams; so each end
//! runs a bridge that packs datagrams into the stream, each behind its length.
//! The computer's bridge hands them to its own QUIC endpoint from a local
//! socket, the phone's takes them from its QUIC endpoint at a local address,
//! and nothing above this -- the handshake, the identities, the protocol --
//! knows Tor was there. Slow: measured on the owner's laptop on 2026-11-02 at
//! 0.15–0.31 MB/s each way, 7–18 s to reach the service; for opening files
//! away from home, never for backups.
//!
//! Tor's own state is kept under the directory given to [`set_dir`], which
//! each program names once at start: the computer's store, the phone's app
//! folder.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where Tor keeps its state and its copy of the network's directory. The
/// first one given is kept.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The port a computer's onion service is reached on.
const PORT: u16 = 1;

/// The largest datagram carried: QUIC's never exceed this.
const MOST: usize = 65_535;

fn config() -> crate::Result<arti_client::TorClientConfig> {
    let dir = DIR.get().cloned().unwrap_or_else(|| std::env::temp_dir().join("qurb-tor"));
    let mut builder = arti_client::config::TorClientConfigBuilder::from_directories(dir.join("state"), dir.join("cache"));
    builder.address_filter().allow_onion_addrs(true);
    // Android keeps an app's files private by its own means, and Arti's
    // ownership checks do not know them.
    #[cfg(target_os = "android")]
    builder.storage().permissions().dangerously_trust_everyone();
    builder.build().map_err(|e| crate::Error::Tor { detail: e.to_string() })
}

/// One Tor client for the process, started when first wanted: a few seconds
/// the first time, a fraction of one after (measured 8.3 s, then 0.17 s).
async fn client() -> crate::Result<std::sync::Arc<arti_client::TorClient<tor_rtcompat::PreferredRuntime>>> {
    static CLIENT: tokio::sync::OnceCell<std::sync::Arc<arti_client::TorClient<tor_rtcompat::PreferredRuntime>>> =
        tokio::sync::OnceCell::const_new();
    CLIENT
        .get_or_try_init(|| async {
            // Arti's TLS asks for the process's default provider and panics
            // if none was named; qurb's own TLS names its provider per use.
            let _ = rustls::crypto::ring::default_provider().install_default();
            arti_client::TorClient::create_bootstrapped(config()?)
                .await
                .map_err(|e| crate::Error::Tor { detail: e.to_string() })
        })
        .await
        .cloned()
}

/// Reach `onion` through Tor, and return a local address that is that
/// computer: what QUIC is pointed at, the bridge carrying its packets.
pub async fn dial(onion: &str) -> crate::Result<SocketAddr> {
    let tor = client().await?;
    let stream = tor
        .connect((onion, PORT))
        .await
        .map_err(|e| crate::Error::Tor { detail: e.to_string() })?;
    let bridge_error = |e: std::io::Error| crate::Error::Tor { detail: format!("the local bridge: {e}") };
    let local = UdpSocket::bind("127.0.0.1:0").await.map_err(bridge_error)?;
    let address = local.local_addr().map_err(bridge_error)?;
    tokio::spawn(bridge(stream, local, None));
    Ok(address)
}

/// Publish this computer's onion service, its packets handed to the QUIC
/// endpoint listening on `quic_port`. Returns the onion address to announce.
#[cfg(feature = "tor-host")]
pub async fn host(quic_port: u16) -> crate::Result<String> {
    use futures_util::StreamExt;
    let tor = client().await?;
    let config = tor_hsservice::config::OnionServiceConfigBuilder::default()
        .nickname("qurb".parse().map_err(|e: tor_hsservice::InvalidNickname| crate::Error::Tor { detail: e.to_string() })?)
        .build()
        .map_err(|e| crate::Error::Tor { detail: e.to_string() })?;
    let (service, requests) = tor
        .launch_onion_service(config)
        .map_err(|e| crate::Error::Tor { detail: e.to_string() })?
        .ok_or_else(|| crate::Error::Tor { detail: "onion services are switched off".into() })?;
    let onion = {
        use safelog::DisplayRedacted;
        service
            .onion_address()
            .map(|a| a.display_unredacted().to_string())
            .ok_or_else(|| crate::Error::Tor { detail: "no onion address yet".into() })?
    };
    tokio::spawn(async move {
        // Kept alive with the task: dropped, the service stops.
        let _service = service;
        let mut streams = tor_hsservice::handle_rend_requests(requests);
        while let Some(request) = streams.next().await {
            tokio::spawn(async move {
                let Ok(stream) = request.accept(tor_cell::relaycell::msg::Connected::new_empty()).await else { return };
                let Ok(local) = UdpSocket::bind("127.0.0.1:0").await else { return };
                let to: SocketAddr = ([127, 0, 0, 1], quic_port).into();
                bridge(stream, local, Some(to)).await;
            });
        }
    });
    Ok(onion)
}

/// Datagrams one way, length-prefixed frames the other, until either ends.
/// `to` is where the local side's datagrams go: the computer's QUIC port, or,
/// on the phone, whoever last sent one (its own QUIC endpoint).
async fn bridge(stream: arti_client::DataStream, local: UdpSocket, mut to: Option<SocketAddr>) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let local = std::sync::Arc::new(local);
    let (seen_tx, mut seen_rx) = tokio::sync::watch::channel(to);
    let inbound = {
        let local = std::sync::Arc::clone(&local);
        async move {
            let mut length = [0u8; 2];
            let mut buf = vec![0u8; MOST];
            loop {
                if reader.read_exact(&mut length).await.is_err() {
                    return;
                }
                let n = u16::from_be_bytes(length) as usize;
                if reader.read_exact(&mut buf[..n]).await.is_err() {
                    return;
                }
                let target = *seen_rx.borrow_and_update();
                if let Some(target) = target {
                    let _ = local.send_to(&buf[..n], target).await;
                }
            }
        }
    };
    let outbound = async move {
        let mut buf = vec![0u8; MOST];
        loop {
            let Ok((n, from)) = local.recv_from(&mut buf).await else { return };
            if to.is_none() {
                // The phone's side: its QUIC endpoint is whoever sends here.
                to = Some(from);
                let _ = seen_tx.send(to);
            }
            let mut frame = Vec::with_capacity(n + 2);
            frame.extend_from_slice(&(n as u16).to_be_bytes());
            frame.extend_from_slice(&buf[..n]);
            if writer.write_all(&frame).await.is_err() || writer.flush().await.is_err() {
                return;
            }
        }
    };
    tokio::select! {
        _ = inbound => {}
        _ = outbound => {}
    }
}
