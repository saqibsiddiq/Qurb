//! Telling a device there is something for it.
//!
//! The service arranges meetings; this is the part where one device asks for a
//! meeting the other does not yet know it wants. It carries who, never what —
//! so the rendezvous service still learns nothing about anybody's files.

use qurb_keys::MasterKey;
use qurb_signal::{Endpoints, FromServer, GroupId, MemberId, SignalClient, SignalServer};
use std::net::SocketAddr;
use std::sync::Arc;

async fn running() -> (Arc<SignalServer>, String) {
    let server = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let url = format!("ws://{}", server.local_addr().unwrap());
    let serving = Arc::clone(&server);
    tokio::spawn(async move { serving.serve().await });
    (server, url)
}

fn somewhere(port: u16) -> Endpoints {
    let address: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    Endpoints { public: Some(address), local: vec![address] }
}

async fn join(url: &str, group: GroupId, member: MemberId, port: u16) -> SignalClient {
    SignalClient::connect_insecure(url, group, member, somewhere(port)).await.unwrap()
}

/// A peer that is connected hears immediately.
#[tokio::test(flavor = "multi_thread")]
async fn a_connected_peer_is_told_at_once() {
    let (_server, url) = running().await;
    let key = MasterKey::generate();
    let group = GroupId::derive(&key);
    let (one, two) = (MemberId::derive(&key, &[1u8; 32]), MemberId::derive(&key, &[2u8; 32]));

    let mut listener = join(&url, group, two, 4002).await;
    let _ = listener.peers().await;

    let speaker = join(&url, group, one, 4001).await;
    speaker.waiting_for(two).unwrap();

    let told = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match listener.next().await {
                Some(FromServer::Waiting { from }) => return Some(from),
                Some(_) => continue,
                None => return None,
            }
        }
    })
    .await
    .expect("timed out waiting to be told");

    assert_eq!(told, Some(one), "the wrong device was named, or none");
}

/// A peer that was away hears the moment it arrives.
///
/// This is the whole reason the note is kept rather than dropped: the device
/// that most needs telling is precisely the one that was asleep when the
/// change happened.
#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_was_away_hears_on_arrival() {
    let (_server, url) = running().await;
    let key = MasterKey::generate();
    let group = GroupId::derive(&key);
    let (one, two) = (MemberId::derive(&key, &[1u8; 32]), MemberId::derive(&key, &[2u8; 32]));

    // Only one device is here, and it has something for a device that is not.
    let speaker = join(&url, group, one, 4001).await;
    speaker.waiting_for(two).unwrap();

    // Long enough for the note to be filed before the other arrives.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut latecomer = join(&url, group, two, 4002).await;
    let told = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match latecomer.next().await {
                Some(FromServer::Waiting { from }) => return Some(from),
                Some(_) => continue,
                None => return None,
            }
        }
    })
    .await
    .expect("timed out waiting to be told");

    assert_eq!(told, Some(one), "arriving did not deliver what was waiting");
}

/// Saying it many times leaves one thing to say.
///
/// Two devices that change a thousand files between them should leave one
/// note, because the answer to "should I sync" is the same either way.
#[tokio::test(flavor = "multi_thread")]
async fn many_notes_collapse_into_one() {
    let (_server, url) = running().await;
    let key = MasterKey::generate();
    let group = GroupId::derive(&key);
    let (one, two) = (MemberId::derive(&key, &[1u8; 32]), MemberId::derive(&key, &[2u8; 32]));

    let speaker = join(&url, group, one, 4001).await;
    for _ in 0..50 {
        speaker.waiting_for(two).unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let mut latecomer = join(&url, group, two, 4002).await;

    // Drain for a moment and count.
    let mut notes = 0;
    let _ = tokio::time::timeout(std::time::Duration::from_millis(700), async {
        while let Some(message) = latecomer.next().await {
            if matches!(message, FromServer::Waiting { .. }) {
                notes += 1;
            }
        }
    })
    .await;

    assert_eq!(notes, 1, "fifty changes produced {notes} notes");
}

/// A device that has not announced cannot file notes, and a note is filed
/// under the group the connection announced into — so one group cannot reach
/// into another.
#[tokio::test(flavor = "multi_thread")]
async fn a_note_cannot_cross_groups() {
    let (_server, url) = running().await;
    let mine = MasterKey::generate();
    let theirs = MasterKey::generate();
    let outsider_group = GroupId::derive(&theirs);
    let my_group = GroupId::derive(&mine);
    let target = MemberId::derive(&mine, &[2u8; 32]);

    let mut victim = join(&url, my_group, target, 4002).await;
    let _ = victim.peers().await;

    // An outsider announces into its own group and names a member id it has
    // no business knowing.
    let outsider = join(&url, outsider_group, MemberId::derive(&theirs, &[9u8; 32]), 4003).await;
    outsider.waiting_for(target).unwrap();

    let heard = tokio::time::timeout(std::time::Duration::from_millis(700), async {
        loop {
            match victim.next().await {
                Some(FromServer::Waiting { from }) => return Some(from),
                Some(_) => continue,
                None => return None,
            }
        }
    })
    .await;

    assert!(heard.is_err(), "a note crossed from another group: {heard:?}");
}
