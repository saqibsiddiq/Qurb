//! Two devices find each other on a network with no server in it.
//!
//! These bind real sockets and send real multicast packets over the loopback
//! interface. That is the point: the codec is unit-tested and says nothing
//! about whether a packet sent by one process is received by another, which is
//! the only thing local discovery has to do.

use qurb_keys::MasterKey;
use qurb_peer::local::{Beacons, Neighbours};
use qurb_signal::{Endpoints, MemberId};
use std::time::Duration;

/// A port per test, so tests running at once do not hear each other.
///
/// The group address is a constant of the protocol; the port is what a test can
/// vary. Derived from the test's own name so that adding one never collides.
fn port(name: &str) -> u16 {
    let hash = blake3::hash(name.as_bytes());
    let bytes: [u8; 2] = hash.as_bytes()[..2].try_into().unwrap();
    // Well above anything privileged, and inside the ephemeral range.
    40_000 + (u16::from_le_bytes(bytes) % 10_000)
}

fn endpoints(port: u16) -> Endpoints {
    Endpoints { public: None, local: vec![([127, 0, 0, 1], port).into()] }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_hears_another_on_the_same_network() {
    let master = MasterKey::from_bytes([11; 32]);
    let group = port("a_device_hears_another_on_the_same_network");

    let laptop = MemberId::from_bytes([1; 32]);
    let phone = MemberId::from_bytes([2; 32]);

    let (_listening, mut sightings) =
        Beacons::start(master.clone(), laptop, endpoints(41935), group).unwrap();
    let (announcing, _) = Beacons::start(master, phone, endpoints(51935), group).unwrap();

    announcing.announce_news().await;

    // Every device sends three beacons on starting, and those say there is no
    // news. Waiting for the one that does, rather than taking the first to
    // arrive, which is a race this test used to lose about one time in five.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(std::time::Instant::now() < deadline, "no beacon with news arrived");
        let seen = tokio::time::timeout(Duration::from_secs(5), sightings.recv())
            .await
            .expect("no beacon arrived within five seconds")
            .expect("the sightings channel closed");

        assert_eq!(seen.member, phone, "heard the wrong device");
        assert_eq!(seen.endpoints.candidates(), vec!["127.0.0.1:51935".parse().unwrap()]);
        if seen.news {
            break;
        }
    }
}

/// Multicast loopback means a device hears its own beacons. Acting on one would
/// have a device forever trying to sync with itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_does_not_hear_itself() {
    let master = MasterKey::from_bytes([12; 32]);
    let group = port("a_device_does_not_hear_itself");
    let alone = MemberId::from_bytes([1; 32]);

    let (beacons, mut sightings) =
        Beacons::start(master, alone, endpoints(41935), group).unwrap();

    beacons.announce_news().await;
    beacons.announce().await;

    let heard = tokio::time::timeout(Duration::from_millis(1200), sightings.recv()).await;
    assert!(heard.is_err(), "a device heard its own beacon");
}

/// Somebody else's qurb on the same café network. Their beacons decrypt to
/// nothing here, so they are neither seen nor mistaken for ours.
#[tokio::test(flavor = "multi_thread")]
async fn a_different_group_is_not_heard() {
    let group = port("a_different_group_is_not_heard");
    let ours = MemberId::from_bytes([1; 32]);
    let theirs = MemberId::from_bytes([2; 32]);

    let (_ours, mut sightings) =
        Beacons::start(MasterKey::from_bytes([13; 32]), ours, endpoints(41935), group).unwrap();
    let (stranger, _) =
        Beacons::start(MasterKey::from_bytes([99; 32]), theirs, endpoints(51935), group).unwrap();

    stranger.announce_news().await;

    let heard = tokio::time::timeout(Duration::from_millis(1200), sightings.recv()).await;
    assert!(heard.is_err(), "a stranger's beacon was accepted");
}

/// The address book is what `reach` consults, so a sighting has to land in it
/// in a form that can be dialled.
#[tokio::test(flavor = "multi_thread")]
async fn a_sighting_becomes_somewhere_to_dial() {
    let master = MasterKey::from_bytes([14; 32]);
    let group = port("a_sighting_becomes_somewhere_to_dial");
    let here = MemberId::from_bytes([1; 32]);
    let there = MemberId::from_bytes([2; 32]);

    let (_listening, mut sightings) =
        Beacons::start(master.clone(), here, endpoints(41935), group).unwrap();
    let (announcing, _) = Beacons::start(master, there, endpoints(51935), group).unwrap();
    announcing.announce().await;

    let seen = tokio::time::timeout(Duration::from_secs(5), sightings.recv())
        .await
        .expect("no beacon")
        .unwrap();

    let neighbours = Neighbours::new();
    assert!(neighbours.where_is(&there).is_none(), "somewhere to dial before anything was seen");

    neighbours.note(seen.member, seen.endpoints);
    let found = neighbours.where_is(&there).expect("the sighting was not recorded");
    assert_eq!(found.candidates(), vec!["127.0.0.1:51935".parse().unwrap()]);
}

/// A device that changes address — a laptop moving from Wi-Fi to a dock — has
/// to start saying so without being restarted.
#[tokio::test(flavor = "multi_thread")]
async fn a_moved_device_announces_its_new_address() {
    let master = MasterKey::from_bytes([15; 32]);
    let group = port("a_moved_device_announces_its_new_address");
    let here = MemberId::from_bytes([1; 32]);
    let there = MemberId::from_bytes([2; 32]);

    let (_listening, mut sightings) =
        Beacons::start(master.clone(), here, endpoints(41935), group).unwrap();
    let (moving, _) = Beacons::start(master, there, endpoints(51935), group).unwrap();

    moving.now_at(Endpoints { public: None, local: vec!["10.0.0.9:61935".parse().unwrap()] });
    moving.announce().await;

    // The three beacons every device sends at startup are still in flight, so
    // this waits for the one carrying the new address rather than the first.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(std::time::Instant::now() < deadline, "the new address was never announced");
        let seen = tokio::time::timeout(Duration::from_secs(5), sightings.recv())
            .await
            .expect("no beacon")
            .unwrap();
        if seen.endpoints.candidates() == vec!["10.0.0.9:61935".parse().unwrap()] {
            break;
        }
    }
}

/// The one that was found on hardware.
///
/// A device that runs only for the length of a sync pass — which is what a
/// phone does — starts with an empty address book and finishes long before the
/// next scheduled beacon. Unless it can *ask*: a probe is answered at once by
/// everyone who hears it, so the book is full within a moment of starting.
#[tokio::test(flavor = "multi_thread")]
async fn an_arriving_device_is_answered_rather_than_left_waiting() {
    let master = MasterKey::from_bytes([16; 32]);
    let group = port("an_arriving_device_is_answered_rather_than_left_waiting");

    let settled = MemberId::from_bytes([1; 32]);
    let arriving = MemberId::from_bytes([2; 32]);

    // A device that has been here a while. Its startup burst is long past, and
    // its next scheduled beacon is up to a full interval away.
    let (_settled, _ignored) =
        Beacons::start(master.clone(), settled, endpoints(41935), group).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // One arrives. It must learn about the other in about a second, not in
    // twenty.
    let started = std::time::Instant::now();
    let (_arriving, mut sightings) =
        Beacons::start(master, arriving, endpoints(51935), group).unwrap();

    let seen = tokio::time::timeout(Duration::from_secs(5), sightings.recv())
        .await
        .expect("the settled device never answered")
        .unwrap();

    assert_eq!(seen.member, settled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}, which means it waited for a scheduled beacon rather than being answered",
        started.elapsed()
    );
}
