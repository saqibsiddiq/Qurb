//! Talking to the rendezvous service.

use crate::error::{Error, Result};
use crate::message::{Endpoints, FromClient, FromServer, Presence};
use crate::rendezvous::{GroupId, MemberId};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// A held-open connection to the rendezvous service.
pub struct SignalClient {
    outgoing: mpsc::UnboundedSender<FromClient>,
    incoming: mpsc::UnboundedReceiver<FromServer>,
    group: GroupId,
    member: MemberId,
    /// Aborted on drop.
    ///
    /// The reader task parks on the socket waiting for a message that may never
    /// come, so it cannot notice on its own that nobody is listening any more.
    /// Without this a dropped client leaves its connection open until something
    /// else happens to it — one leaked connection per abandoned client, and the
    /// server goes on offering its stale address to peers.
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for SignalClient {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl SignalClient {
    /// Connect and announce.
    ///
    /// Refuses a plain `ws://` URL to anywhere but this machine. Rendezvous
    /// identifiers are bearer secrets — anyone who sees one can enumerate that
    /// group's addresses — so sending them unencrypted across a network would
    /// hand them to everyone on the path. Tests and local development use
    /// [`connect_insecure`](Self::connect_insecure), which says what it is.
    pub async fn connect(
        url: &str,
        group: GroupId,
        member: MemberId,
        endpoints: Endpoints,
    ) -> Result<Self> {
        if url.starts_with("ws://") && !is_local(url) {
            return Err(Error::InsecureUrl { url: url.to_string() });
        }
        Self::open(url, group, member, endpoints).await
    }

    /// Connect without requiring encryption. For tests and local development.
    pub async fn connect_insecure(
        url: &str,
        group: GroupId,
        member: MemberId,
        endpoints: Endpoints,
    ) -> Result<Self> {
        Self::open(url, group, member, endpoints).await
    }

    async fn open(
        url: &str,
        group: GroupId,
        member: MemberId,
        endpoints: Endpoints,
    ) -> Result<Self> {
        let (websocket, _) = tokio_tungstenite::connect_async(url).await?;
        let (mut sink, mut source) = websocket.split();

        let (outgoing, mut to_send) = mpsc::unbounded_channel::<FromClient>();
        let (received, incoming) = mpsc::unbounded_channel::<FromServer>();

        let writer = tokio::spawn(async move {
            while let Some(message) = to_send.recv().await {
                let Ok(text) = serde_json::to_string(&message) else { continue };
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            // Say goodbye rather than vanishing, so the server can drop this
            // device from its directory immediately instead of waiting for a
            // timeout it would otherwise have to implement.
            let _ = sink.close().await;
        });

        let reader = tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                if let Message::Text(text) = message {
                    match serde_json::from_str::<FromServer>(&text) {
                        Ok(parsed) => {
                            if received.send(parsed).is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::debug!(error = %e, "unparseable signalling message"),
                    }
                }
            }
        });

        let client =
            Self { outgoing, incoming, group, member, tasks: vec![writer, reader] };
        client.announce(endpoints)?;
        Ok(client)
    }

    /// Say where we are, replacing anything said before.
    ///
    /// Called again whenever the addresses change, which a laptop moving between
    /// networks does several times a day.
    pub fn announce(&self, endpoints: Endpoints) -> Result<()> {
        self.send(FromClient::Announce { group: self.group, member: self.member, endpoints })
    }

    /// Ask to reach a peer. The reply arrives as [`FromServer::Punch`].
    pub fn connect_to(&self, peer: MemberId) -> Result<()> {
        self.send(FromClient::Connect { to: peer })
    }

    /// Agree to be reached, which is what releases the simultaneous punch.
    pub fn accept(&self, peer: MemberId, endpoints: Endpoints) -> Result<()> {
        self.send(FromClient::Accept { to: peer, endpoints })
    }

    pub async fn next(&mut self) -> Option<FromServer> {
        self.incoming.recv().await
    }

    /// Wait for the peers already connected, which the server sends on announce.
    pub async fn peers(&mut self) -> Result<Vec<Presence>> {
        loop {
            match self.next().await {
                Some(FromServer::Peers { members }) => return Ok(members),
                Some(FromServer::Error { detail }) => return Err(Error::Server { detail }),
                Some(_) => continue,
                None => return Err(Error::Closed),
            }
        }
    }

    pub fn member(&self) -> MemberId {
        self.member
    }

    fn send(&self, message: FromClient) -> Result<()> {
        self.outgoing.send(message).map_err(|_| Error::Closed)
    }
}

/// Whether a URL points at this machine, where plaintext is not a network risk.
///
/// Conservative by construction: anything it cannot confidently identify as
/// local is treated as remote, because the cost of being wrong in that
/// direction is a refused connection and in the other is a leaked secret.
fn is_local(url: &str) -> bool {
    let rest = url.strip_prefix("ws://").unwrap_or(url);
    let rest = rest.strip_prefix("wss://").unwrap_or(rest);

    // An IPv6 host is bracketed, so the colons inside it are not separators.
    let host = if let Some(bracketed) = rest.strip_prefix('[') {
        match bracketed.split_once(']') {
            Some((inner, _)) => inner,
            None => return false,
        }
    } else {
        rest.split(['/', ':']).next().unwrap_or("")
    };

    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_to_a_remote_host_is_refused() {
        // The rendezvous identifier is a bearer secret. Sending it in the clear
        // across a network gives it to everyone on the path.
        for url in ["ws://example.com:8080", "ws://203.0.113.5:9000", "ws://signal.qurb.dev/ws"] {
            assert!(!is_local(url), "{url} was treated as local");
        }
    }

    #[test]
    fn plaintext_to_this_machine_is_allowed() {
        for url in ["ws://localhost:9000", "ws://127.0.0.1:9000/ws", "ws://[::1]:9000"] {
            assert!(is_local(url), "{url} was treated as remote");
        }
    }
}
