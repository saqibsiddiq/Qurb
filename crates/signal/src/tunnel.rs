//! The relay, carried by the rendezvous service, at `/relay`.
//!
//! The relay forwards datagrams over a byte stream (`qurb_relay`). On its own
//! port that stream is TCP; but a phone off its home network reaches the
//! rendezvous service and very often nothing else -- a carrier, a hotspot, a
//! network that lets only web traffic out, or a service published the one
//! way a proxy will publish it, over TLS by name. On 2026-10-15 the S23 could
//! not sync on another phone's hotspot, and the relay meant for exactly that
//! ran nowhere, and could not have been published through the Tailscale
//! Funnel that publishes the rendezvous: the relay spoke plain TCP.
//!
//! So the rendezvous service relays as well: a WebSocket opened at `/relay`
//! on its own address is a byte stream to a relay it was given
//! ([`crate::SignalServer::relaying`]), and a device finds it with nothing to
//! configure, beside the rendezvous it already uses ([`beside`]). Each
//! WebSocket message carries whatever bytes were written; the relay's own
//! framing runs inside, unchanged, and inside that a QUIC session with pinned
//! certificates, so the service still sees nothing it can read.

use crate::error::{Error, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Where the relay is, on a rendezvous service's address.
pub const PATH: &str = "/relay";

/// The most one message may carry: well over what a writer hands over at a
/// time ([`CHUNK`]), and bounded, as everything a stranger can send is.
const MAX_MESSAGE: usize = 1 << 20;

/// How much is read from the stream into one message.
const CHUNK: usize = 64 * 1024;

/// What the stream between the WebSocket and its reader may hold.
const BUFFER: usize = 256 * 1024;

/// How often an idle tunnel is pinged, so that a proxy on the way -- a
/// Funnel, a load balancer -- does not close a connection that is only
/// quiet. A device's relay connection lasts as long as its connector, which
/// on a computer is hours.
const PING_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The relay beside a rendezvous service at `signal_url`: the same address
/// at [`PATH`], with the same pinned certificate, if any.
///
/// `wss://rendezvous.example.com` → `wss://rendezvous.example.com/relay`;
/// `wss://203.0.113.5:9000#40…41` → `wss://203.0.113.5:9000/relay#40…41`.
pub fn beside(signal_url: &str) -> String {
    let (address, pin) = crate::tls::split_pin(signal_url.trim());
    let address = address.trim_end_matches('/');
    match pin {
        Some(pin) => format!("{address}{PATH}#{pin}"),
        None => format!("{address}{PATH}"),
    }
}

fn config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE))
}

/// Open the relay at `url`, a `ws://` or `wss://` URL, as a byte stream.
///
/// The rendezvous connection's rules: `ws://` only to this machine or its
/// local network, a fingerprint after `#` pinning the certificate, and a
/// bounded handshake.
pub async fn open(url: &str) -> Result<DuplexStream> {
    if url.starts_with("ws://") && !crate::client::is_local(url) {
        return Err(Error::InsecureUrl { url: url.to_string() });
    }
    let (address, pin) = crate::tls::split_pin(url);
    let connector = match pin {
        None => None,
        Some(fingerprint) => {
            Some(tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(crate::tls::pinned_to(fingerprint)?)))
        }
    };
    let handshake = tokio_tungstenite::connect_async_tls_with_config(address, Some(config()), false, connector);
    let (websocket, _) = tokio::time::timeout(crate::client::HANDSHAKE_TIMEOUT, handshake)
        .await
        .map_err(|_| Error::NoAnswer(crate::client::HANDSHAKE_TIMEOUT))??;
    Ok(carry(websocket))
}

/// A WebSocket, as the byte stream its binary messages carry: the stream
/// returned is read and written like a socket, and the WebSocket is pumped
/// behind it until either end closes.
pub(crate) fn carry<S>(websocket: WebSocketStream<S>) -> DuplexStream
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (ours, theirs) = tokio::io::duplex(BUFFER);
    let (mut from_stream, mut to_stream) = tokio::io::split(theirs);
    let (mut sink, mut source) = websocket.split();

    tokio::spawn(async move {
        while let Some(Ok(message)) = source.next().await {
            match message {
                Message::Binary(bytes) => {
                    if to_stream.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                // Pings are answered by the library as it reads; text has no
                // place in a byte stream.
                _ => {}
            }
        }
        let _ = to_stream.shutdown().await;
    });

    tokio::spawn(async move {
        let mut buffer = vec![0u8; CHUNK];
        let mut ping = tokio::time::interval(PING_EVERY);
        ping.reset();
        loop {
            tokio::select! {
                read = from_stream.read(&mut buffer) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sink.send(Message::Binary(buffer[..n].to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = sink.close().await;
    });

    ours
}

/// Accept a WebSocket at [`PATH`] on a connection already known to be asking
/// for it, as a byte stream.
pub(crate) async fn accept<S>(stream: S) -> Result<DuplexStream>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let websocket = tokio_tungstenite::accept_async_with_config(stream, Some(config())).await?;
    Ok(carry(websocket))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_is_beside_the_rendezvous() {
        assert_eq!(beside("wss://rendezvous.example.com"), "wss://rendezvous.example.com/relay");
        assert_eq!(beside("wss://saqib.tail86283c.ts.net/"), "wss://saqib.tail86283c.ts.net/relay");
        assert_eq!(beside("ws://localhost:9000"), "ws://localhost:9000/relay");
        assert_eq!(beside("wss://203.0.113.5:9000#4047ab"), "wss://203.0.113.5:9000/relay#4047ab");
    }
}
