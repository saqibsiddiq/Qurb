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

    /// I have something for this member, whenever it is next able to hear it.
    ///
    /// Carries no content and no filenames — the fact that there is work, and
    /// who it is for. That is the whole point: the service arranges meetings
    /// and never learns what is said at them.
    ///
    /// Sent when a device's own state advances. It is a request to have the
    /// other device woken *if the server can*, not a request to transfer
    /// anything: the two devices do that directly once both are awake.
    ///
    /// Delivered immediately if the recipient is connected, and remembered if
    /// not, so it arrives the moment they appear. Remembering it is what makes
    /// a device that was asleep at the moment of the change learn about it
    /// without waiting for its own next poll — the difference between a photo
    /// arriving in seconds and arriving in a quarter of an hour.
    Waiting { to: MemberId },

    /// This is how to wake me when I am not connected.
    ///
    /// A phone cannot hold a socket open in the background, so the device most
    /// in need of being told something is the one that cannot be told. The
    /// token is whatever the platform's push service issued; qurb never looks
    /// inside it and does nothing with it but hand it back.
    ///
    /// `None` withdraws it — on signing out, or when the platform revokes one.
    /// A device that never sends this is simply never woken, which is how
    /// every desktop behaves: it is already connected.
    Reachable { via: Option<String> },
}

/// Server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromServer {
    /// Who else from your group is currently connected.
    Peers { members: Vec<Presence> },

    /// Someone in your group wants to reach you.
    ConnectRequest { from: MemberId, endpoints: Endpoints },

    /// Someone in your group has just announced themselves.
    ///
    /// Sent to the members who were already connected, which is the half that
    /// used to be missing: the server knew the moment a device appeared and
    /// told only the device itself. Everyone else had to find out by asking,
    /// and a peer that asks on a backoff will not be asking at the moment a
    /// phone is briefly awake.
    ///
    /// Carries the same `Presence` a `Peers` entry does, so a recipient can act
    /// on it without a second round trip.
    Appeared { peer: Presence },

    /// Both sides are ready. Punch now.
    ///
    /// Sent to both at the same moment, which is the entire reason this is a
    /// held-open channel rather than a request and a response. Hole punching
    /// needs both routers to see an outbound packet at roughly the same time;
    /// a device that has to poll to find out will always be late.
    Punch { peer: MemberId, endpoints: Endpoints },

    /// Somebody in your group has something for you. Sync with them.
    ///
    /// The counterpart of [`FromClient::Waiting`], and equally empty: it says
    /// who, never what. A device that receives one should sync with that peer
    /// now rather than at its next scheduled attempt.
    Waiting { from: MemberId },

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
