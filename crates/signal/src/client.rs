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

/// Whether plaintext to this URL stays off the open internet.
///
/// Two places it does. This machine, obviously. And the local network — a
/// laptop and a phone on the same Wi-Fi, which is how qurb is set up before
/// anyone has hosted anything, and where requiring a certificate would mean
/// requiring a certificate for `192.168.1.4`, which nobody can get.
///
/// Anywhere else is the open internet, where the rendezvous identifiers are
/// bearer secrets crossing networks belonging to strangers: a café, a mobile
/// carrier, whatever is between. That needs `wss://`.
///
/// Conservative by construction: anything it cannot confidently place on a
/// local network is treated as remote, because being wrong that way costs a
/// refused connection and being wrong the other way leaks a secret.
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

    if matches!(host, "localhost" | "::1") {
        return true;
    }

    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            // Loopback, RFC 1918 private ranges, and link-local. Explicitly
            // *not* carrier-grade NAT (100.64/10): a phone on mobile data sits
            // inside one of those with the whole carrier, which is not a local
            // network in any sense that makes plaintext acceptable.
            ip.is_loopback() || ip.is_private() || ip.is_link_local()
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            // Unique-local (fc00::/7) and link-local (fe80::/10).
            ip.is_loopback()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
        // A hostname. It could resolve anywhere, so it is treated as remote —
        // which for a name means `wss://`, and a name is exactly the case
        // where getting a certificate is possible.
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundary this draws is "does it leave the local network", and it
    /// is the whole reason the check exists.
    #[test]
    fn plaintext_stays_on_the_local_network() {
        // Here, and on the same Wi-Fi: no certificate is obtainable for
        // `192.168.1.4`, and nothing leaves the building.
        assert!(is_local("ws://localhost:9000"));
        assert!(is_local("ws://127.0.0.1:9000"));
        assert!(is_local("ws://192.168.1.4:9000"));
        assert!(is_local("ws://10.0.2.2:9000"));
        assert!(is_local("ws://172.16.5.9:9000"));
        assert!(is_local("ws://[::1]:9000"));
        assert!(is_local("ws://[fd00::1]:9000"));

        // The open internet, where the identifiers cross networks belonging to
        // strangers.
        assert!(!is_local("ws://192.140.152.39:9000"));
        assert!(!is_local("ws://rendezvous.example.com:9000"));
        assert!(!is_local("ws://8.8.8.8:9000"));

        // Carrier-grade NAT is not a local network: a phone on mobile data
        // shares one with the rest of the carrier.
        assert!(!is_local("ws://100.64.0.1:9000"));
    }

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
