//! The meeting point over real public Nostr relays (decision 0082): two
//! devices of one group, one asks for the other, both are told to punch.
//! Needs the internet, so ignored by default:
//!
//!     cargo test -p qurb-signal --test nostr_live -- --ignored --nocapture

use qurb_signal::{Endpoints, FromServer, GroupId, MemberId, SignalClient};
use std::time::{Duration, Instant};

fn at(port: u16) -> Endpoints {
    Endpoints { public: Some(format!("203.0.113.7:{port}").parse().unwrap()), local: vec![], onion: None }
}

#[tokio::test]
#[ignore]
async fn two_devices_meet_over_public_relays() {
    let mut group = [0u8; 32];
    getrandom::fill(&mut group).unwrap();
    let group = GroupId::from_bytes(group);
    let (a, b) = (MemberId::from_bytes([1; 32]), MemberId::from_bytes([2; 32]));

    let started = Instant::now();
    let mut laptop = SignalClient::connect("nostr", group, b, at(4002)).await.expect("the laptop reaches no relay");
    let mut phone = SignalClient::connect("nostr", group, a, at(4001)).await.expect("the phone reaches no relay");
    println!("both listening after {:?}", started.elapsed());
    // Subscriptions settle on every relay.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let asked = Instant::now();
    phone.connect_to(b).unwrap();
    let request = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match laptop.next().await {
                Some(FromServer::ConnectRequest { from, endpoints }) => return (from, endpoints),
                Some(_) => continue,
                None => panic!("the laptop's channel closed"),
            }
        }
    })
    .await
    .expect("the laptop never heard the phone ask");
    assert_eq!(request.0, a);
    assert_eq!(request.1, at(4001));
    println!("the laptop heard the ask after {:?}", asked.elapsed());

    laptop.accept(a, at(4002)).unwrap();
    let laptop_punch = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(FromServer::Punch { peer, endpoints }) = laptop.next().await {
                return (peer, endpoints);
            }
        }
    })
    .await
    .expect("the laptop was not told to punch");
    assert_eq!(laptop_punch, (a, at(4001)));

    let phone_punch = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match phone.next().await {
                Some(FromServer::Punch { peer, endpoints }) => return (peer, endpoints),
                Some(_) => continue,
                None => panic!("the phone's channel closed"),
            }
        }
    })
    .await
    .expect("the phone never heard the laptop agree");
    assert_eq!(phone_punch, (b, at(4002)));
    println!("both told to punch {:?} after the ask", asked.elapsed());
}
