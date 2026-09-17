//! Two devices finding each other.
//!
//! The tests that matter here are about what the server does *not* do as much
//! as what it does: it must introduce devices that belong together, refuse to
//! introduce ones that do not, and learn nothing about either beyond where they
//! are.

use qurb_keys::MasterKey;
use qurb_signal::{Endpoints, FromServer, GroupId, MemberId, SignalClient, SignalServer};
use std::sync::Arc;
use std::time::Duration;

fn endpoints(port: u16) -> Endpoints {
    Endpoints {
        public: Some(format!("203.0.113.5:{port}").parse().unwrap()),
        local: vec![format!("192.168.1.{}:{port}", port % 250 + 1).parse().unwrap()],
    }
}

/// A running server, and the URL to reach it.
async fn server() -> (Arc<SignalServer>, String) {
    let server = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let url = format!("ws://{}", server.local_addr().unwrap());

    let running = Arc::clone(&server);
    tokio::spawn(async move { running.serve().await });
    (server, url)
}

async fn join(url: &str, master: &MasterKey, fingerprint: [u8; 32], port: u16) -> SignalClient {
    SignalClient::connect_insecure(
        url,
        GroupId::derive(master),
        MemberId::derive(master, &fingerprint),
        endpoints(port),
    )
    .await
    .expect("connect to the signalling server")
}

/// Wait for a particular message, so a test fails with "never arrived" rather
/// than hanging.
async fn expect(client: &mut SignalClient, what: &str) -> FromServer {
    match tokio::time::timeout(Duration::from_secs(5), client.next()).await {
        Ok(Some(message)) => message,
        Ok(None) => panic!("the connection closed while waiting for {what}"),
        Err(_) => panic!("{what} never arrived"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_sees_the_others_in_its_group() {
    let master = MasterKey::generate();
    let (_server, url) = server().await;

    let mut first = join(&url, &master, [1; 32], 4001).await;
    assert!(first.peers().await.unwrap().is_empty(), "the first device should be alone");

    let mut second = join(&url, &master, [2; 32], 4002).await;
    let seen = second.peers().await.unwrap();

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].member, MemberId::derive(&master, &[1; 32]));
    assert_eq!(seen[0].endpoints, endpoints(4001));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_are_told_to_punch_at_the_same_moment() {
    // The reason this is a held-open channel rather than a request and a
    // response. Hole punching needs both routers to see an outbound packet at
    // roughly the same time, and a device that has to poll will always be late.
    let master = MasterKey::generate();
    let (_server, url) = server().await;

    let mut alice = join(&url, &master, [1; 32], 4001).await;
    let mut bob = join(&url, &master, [2; 32], 4002).await;
    alice.peers().await.unwrap();
    bob.peers().await.unwrap();

    alice.connect_to(bob.member()).unwrap();

    // Bob hears that Alice wants him, and where she is.
    let request = expect(&mut bob, "a connect request").await;
    let FromServer::ConnectRequest { from, endpoints: alice_endpoints } = request else {
        panic!("expected a connect request, got {request:?}");
    };
    assert_eq!(from, alice.member());
    assert_eq!(alice_endpoints, endpoints(4001));

    bob.accept(from, endpoints(4002)).unwrap();

    // Both are told to punch, each learning the other's addresses.
    let to_alice = expect(&mut alice, "a punch instruction for Alice").await;
    let to_bob = expect(&mut bob, "a punch instruction for Bob").await;

    match (to_alice, to_bob) {
        (
            FromServer::Punch { peer: alice_sees, endpoints: alice_got },
            FromServer::Punch { peer: bob_sees, endpoints: bob_got },
        ) => {
            assert_eq!(alice_sees, bob.member());
            assert_eq!(bob_sees, alice.member());
            assert_eq!(alice_got, endpoints(4002), "Alice was given the wrong addresses");
            assert_eq!(bob_got, endpoints(4001), "Bob was given the wrong addresses");
        }
        other => panic!("expected two punch instructions, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_group_cannot_see_or_reach_another() {
    // Two unrelated users on one server. Nothing either does may reveal the
    // other, because the server matches on an identifier derived from a key it
    // does not hold.
    let mine = MasterKey::generate();
    let theirs = MasterKey::generate();
    let (server_handle, url) = server().await;

    let mut my_device = join(&url, &mine, [1; 32], 4001).await;
    let mut their_device = join(&url, &theirs, [1; 32], 4002).await;

    assert!(my_device.peers().await.unwrap().is_empty(), "saw a device from another group");
    assert!(their_device.peers().await.unwrap().is_empty(), "saw a device from another group");
    assert_eq!(server_handle.group_count(), 2, "the groups were not kept apart");

    // Even naming the other's identifier gets nowhere: the lookup happens inside
    // the asking connection's own group.
    my_device.connect_to(MemberId::derive(&theirs, &[1; 32])).unwrap();
    let reply = expect(&mut my_device, "a refusal").await;
    assert!(
        matches!(reply, FromServer::Error { .. }),
        "reaching across groups was allowed: {reply:?}"
    );

    // And the other device heard nothing at all.
    let quiet = tokio::time::timeout(Duration::from_millis(400), their_device.next()).await;
    assert!(quiet.is_err(), "a device in another group was contacted: {quiet:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_networks_replaces_the_old_address() {
    // A laptop does this several times a day. An address left behind is one a
    // peer will waste time punching towards.
    let master = MasterKey::generate();
    let (_server, url) = server().await;

    let mut laptop = join(&url, &master, [1; 32], 4001).await;
    laptop.peers().await.unwrap();

    let mut desktop = join(&url, &master, [2; 32], 4002).await;
    assert_eq!(desktop.peers().await.unwrap()[0].endpoints, endpoints(4001));

    // The laptop moves.
    laptop.announce(endpoints(4009)).unwrap();
    laptop.peers().await.unwrap();

    // A device joining now sees only the new address.
    let mut phone = join(&url, &master, [3; 32], 4003).await;
    let seen = phone.peers().await.unwrap();
    let laptop_entry = seen.iter().find(|p| p.member == laptop.member()).expect("laptop present");
    assert_eq!(laptop_entry.endpoints, endpoints(4009), "the stale address survived");
    assert_eq!(seen.len(), 2, "the laptop appeared twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_leaves_stops_being_offered() {
    let master = MasterKey::generate();
    let (server_handle, url) = server().await;

    let mut laptop = join(&url, &master, [1; 32], 4001).await;
    laptop.peers().await.unwrap();
    {
        let mut phone = join(&url, &master, [2; 32], 4002).await;
        assert_eq!(phone.peers().await.unwrap().len(), 1);
    }
    drop(laptop);

    // Give the server a moment to notice both connections ending.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        server_handle.group_count(),
        0,
        "an empty group was left behind, which is a leak keyed on something anyone can generate"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reaching_a_device_that_is_not_connected_is_refused_cleanly() {
    let master = MasterKey::generate();
    let (_server, url) = server().await;

    let mut lonely = join(&url, &master, [1; 32], 4001).await;
    lonely.peers().await.unwrap();

    lonely.connect_to(MemberId::derive(&master, &[9; 32])).unwrap();
    let reply = expect(&mut lonely, "a refusal").await;
    assert!(matches!(reply, FromServer::Error { .. }), "got {reply:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn nonsense_does_not_bring_the_server_down() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let master = MasterKey::generate();
    let (_server, url) = server().await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    for junk in ["", "{}", "null", r#"{"type":"nope"}"#, "\u{0}\u{1}", &"x".repeat(50_000)] {
        let _ = socket.send(Message::Text(junk.to_string().into())).await;
    }
    let _ = socket.send(Message::Binary(vec![0xFF; 1000].into())).await;
    drop(socket);

    // The server must still be serving.
    let mut device = join(&url, &master, [1; 32], 4001).await;
    assert!(device.peers().await.is_ok(), "the server stopped working after junk input");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_never_announces_can_do_nothing() {
    // Otherwise a client could use the server as a directory of other people's
    // addresses without ever saying who it is.
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let master = MasterKey::generate();
    let (_server, url) = server().await;
    let mut present = join(&url, &master, [1; 32], 4001).await;
    present.peers().await.unwrap();

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let probe = serde_json::to_string(&qurb_signal::FromClient::Connect {
        to: MemberId::derive(&master, &[1; 32]),
    })
    .unwrap();
    socket.send(Message::Text(probe.into())).await.unwrap();

    let reply = tokio::time::timeout(Duration::from_secs(3), socket.next()).await;
    match reply {
        Ok(Some(Ok(Message::Text(text)))) => {
            let parsed: FromServer = serde_json::from_str(&text).unwrap();
            assert!(matches!(parsed, FromServer::Error { .. }), "got {parsed:?}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }

    // And the announced device was not disturbed.
    let quiet = tokio::time::timeout(Duration::from_millis(400), present.next()).await;
    assert!(quiet.is_err(), "an unannounced client reached a real device");
}

#[tokio::test(flavor = "multi_thread")]
async fn plaintext_to_a_remote_server_is_refused() {
    // Rendezvous identifiers are bearer secrets. The insecure path exists but
    // has to be asked for by name.
    let master = MasterKey::generate();
    let result = SignalClient::connect(
        "ws://signal.example.com:9000",
        GroupId::derive(&master),
        MemberId::derive(&master, &[1; 32]),
        endpoints(4001),
    )
    .await;

    match result {
        Err(qurb_signal::Error::InsecureUrl { .. }) => {}
        Err(other) => panic!("refused, but for the wrong reason: {other}"),
        Ok(_) => panic!("a rendezvous identifier was sent unencrypted to a remote server"),
    }
}

// -- limits ------------------------------------------------------------------

/// A server with limits tight enough to reach in a test.
async fn strict_server(limits: qurb_signal::Limits) -> (Arc<SignalServer>, String) {
    let server =
        Arc::new(SignalServer::bind_with("127.0.0.1:0".parse().unwrap(), limits).await.unwrap());
    let url = format!("ws://{}", server.local_addr().unwrap());
    let running = Arc::clone(&server);
    tokio::spawn(async move { running.serve().await });
    (server, url)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_cannot_grow_without_bound() {
    // Memory anyone can spend, if unbounded.
    let master = MasterKey::generate();
    let (_server, url) = strict_server(qurb_signal::Limits {
        max_members_per_group: 2,
        ..Default::default()
    })
    .await;

    let mut first = join(&url, &master, [1; 32], 4001).await;
    first.peers().await.unwrap();
    let mut second = join(&url, &master, [2; 32], 4002).await;
    second.peers().await.unwrap();

    let mut third = join(&url, &master, [3; 32], 4003).await;
    let reply = expect(&mut third, "a refusal").await;
    assert!(matches!(reply, FromServer::Error { .. }), "a full group accepted another: {reply:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_moved_can_always_re_announce() {
    // The limit above must not stop an existing member updating its address,
    // which a laptop changing networks does constantly.
    let master = MasterKey::generate();
    let (_server, url) = strict_server(qurb_signal::Limits {
        max_members_per_group: 1,
        ..Default::default()
    })
    .await;

    let mut only = join(&url, &master, [1; 32], 4001).await;
    only.peers().await.unwrap();

    only.announce(endpoints(4099)).unwrap();
    let reply = expect(&mut only, "an updated peer list").await;
    assert!(
        matches!(reply, FromServer::Peers { .. }),
        "re-announcing was refused by the group limit: {reply:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flood_is_dropped_rather_than_answered() {
    // Answering would let a caller spend the server's bandwidth by spending
    // only its own.
    let master = MasterKey::generate();
    let (_server, url) = strict_server(qurb_signal::Limits {
        messages_per_second: 5,
        burst: 5,
        ..Default::default()
    })
    .await;

    let mut device = join(&url, &master, [1; 32], 4001).await;
    device.peers().await.unwrap();

    for _ in 0..200 {
        let _ = device.announce(endpoints(4001));
    }

    // Far fewer replies than requests, and the server is still alive.
    let mut replies = 0;
    while tokio::time::timeout(Duration::from_millis(250), device.next()).await.is_ok() {
        replies += 1;
        if replies > 50 {
            break;
        }
    }
    assert!(replies < 50, "the server answered {replies} of 200 flooded messages");

    let mut fresh = join(&url, &master, [2; 32], 4002).await;
    assert!(fresh.peers().await.is_ok(), "the server stopped serving after a flood");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_message_does_not_reach_the_parser() {
    // Bounded by the websocket layer, before a frame is assembled in memory.
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let master = MasterKey::generate();
    let (_server, url) = strict_server(qurb_signal::Limits {
        max_message: 4096,
        ..Default::default()
    })
    .await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let _ = socket.send(Message::Text("x".repeat(100_000).into())).await;
    drop(socket);

    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut device = join(&url, &master, [1; 32], 4001).await;
    assert!(device.peers().await.is_ok(), "the server stopped serving after an oversized message");
}
