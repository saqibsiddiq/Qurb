//! What is said over the signalling channel.
//!
//! JSON, because this is a low-volume control channel where being able to read
//! a capture matters more than bytes on the wire. The data plane is a compact
//! binary format precisely because the trade runs the other way there.

use crate::rendezvous::{GroupId, MemberId};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Where a device thinks it can be reached.
///
/// Both kinds are sent. Two devices on one network reach each other directly and
/// should not take a detour through the internet to do it, and the local address
/// is the only way they can discover that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoints {
    /// Seen by a STUN server: what the outside world can reach.
    pub public: Option<SocketAddr>,
    /// Bound locally: what the same network can reach.
    pub local: Vec<SocketAddr>,
}

impl Endpoints {
    /// Every address worth trying, local first.
    ///
    /// Order matters: a local address that works is faster, cheaper and does not
    /// involve anyone else's network.
    pub fn candidates(&self) -> Vec<SocketAddr> {
        let mut out = self.local.clone();
        out.extend(self.public);
        out
    }

    pub fn is_empty(&self) -> bool {
        self.public.is_none() && self.local.is_empty()
    }
}

/// Client to server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromClient {
    /// I am here, at these addresses.
    ///
    /// Sent on connecting and again whenever the addresses change — a laptop
    /// moving between networks does this several times a day.
    Announce { group: GroupId, member: MemberId, endpoints: Endpoints },

    /// I want to reach this member of my group.
    Connect { to: MemberId },

    /// Yes, and here is where I am.
    ///
    /// The reply that lets the server tell both sides to punch at once.
    Accept { to: MemberId, endpoints: Endpoints },
}

/// Server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromServer {
    /// Who else from your group is currently connected.
    Peers { members: Vec<Presence> },

    /// Someone in your group wants to reach you.
    ConnectRequest { from: MemberId, endpoints: Endpoints },

    /// Both sides are ready. Punch now.
    ///
    /// Sent to both at the same moment, which is the entire reason this is a
    /// held-open channel rather than a request and a response. Hole punching
    /// needs both routers to see an outbound packet at roughly the same time;
    /// a device that has to poll to find out will always be late.
    Punch { peer: MemberId, endpoints: Endpoints },

    /// Something was wrong with what you sent.
    Error { detail: String },
}

/// A member currently connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub member: MemberId,
    pub endpoints: Endpoints,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> Endpoints {
        Endpoints {
            public: Some("203.0.113.5:4500".parse().unwrap()),
            local: vec!["192.168.1.40:4500".parse().unwrap()],
        }
    }

    #[test]
    fn local_addresses_are_tried_first() {
        // Two devices on one network should not route through the internet to
        // reach each other.
        let candidates = endpoints().candidates();
        assert_eq!(candidates[0], "192.168.1.40:4500".parse().unwrap());
        assert_eq!(candidates[1], "203.0.113.5:4500".parse().unwrap());
    }

    #[test]
    fn a_device_with_no_public_address_still_offers_its_local_one() {
        let only_local =
            Endpoints { public: None, local: vec!["10.0.0.2:4500".parse().unwrap()] };
        assert!(!only_local.is_empty());
        assert_eq!(only_local.candidates().len(), 1);
    }

    #[test]
    fn messages_round_trip() {
        let group = GroupId::from_bytes([1; 32]);
        let member = MemberId::from_bytes([2; 32]);

        let sent = [
            FromClient::Announce { group, member, endpoints: endpoints() },
            FromClient::Connect { to: member },
            FromClient::Accept { to: member, endpoints: endpoints() },
        ];
        for message in sent {
            let json = serde_json::to_string(&message).unwrap();
            assert_eq!(serde_json::from_str::<FromClient>(&json).unwrap(), message);
        }

        let received = [
            FromServer::Peers { members: vec![Presence { member, endpoints: endpoints() }] },
            FromServer::ConnectRequest { from: member, endpoints: endpoints() },
            FromServer::Punch { peer: member, endpoints: endpoints() },
            FromServer::Error { detail: "no".into() },
        ];
        for message in received {
            let json = serde_json::to_string(&message).unwrap();
            assert_eq!(serde_json::from_str::<FromServer>(&json).unwrap(), message);
        }
    }

    #[test]
    fn rubbish_is_refused_rather_than_guessed_at() {
        for bad in ["", "{}", "null", "[]", r#"{"type":"nonsense"}"#, r#"{"type":"connect"}"#] {
            assert!(serde_json::from_str::<FromClient>(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn a_message_carries_no_file_information() {
        // Stated as a test because it is the promise the design makes: the
        // server is a phone book, and nothing in the vocabulary lets it become
        // more than one.
        let json = serde_json::to_string(&FromClient::Announce {
            group: GroupId::from_bytes([1; 32]),
            member: MemberId::from_bytes([2; 32]),
            endpoints: endpoints(),
        })
        .unwrap();

        for forbidden in ["path", "file", "name", "chunk", "hash", "content"] {
            assert!(!json.contains(forbidden), "{forbidden:?} appeared in a signalling message");
        }
    }
}
