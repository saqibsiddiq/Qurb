//! Tor end to end on the live network: an onion service published, reached,
//! and datagrams carried both ways through the bridges (decision 0082).
//!
//! Ignored by default -- it needs the internet and takes up to a minute:
//!
//!     cargo test -p qurb-peer --features tor-host --test tor_live -- --ignored --nocapture

#![cfg(feature = "tor-host")]

use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

#[tokio::test]
#[ignore]
async fn datagrams_cross_tor_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    qurb_peer::tor::set_dir(dir.path().to_path_buf());

    // The computer's side: an echo where its QUIC endpoint would be.
    let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = echo.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65_535];
        loop {
            let Ok((n, from)) = echo.recv_from(&mut buf).await else { return };
            let _ = echo.send_to(&buf[..n], from).await;
        }
    });

    let started = Instant::now();
    let onion = qurb_peer::tor::host(port).await.expect("the service published");
    println!("published {onion} in {:.1?}", started.elapsed());

    // A new service takes a while to be findable; keep asking.
    let reached = Instant::now();
    let bridge = loop {
        match qurb_peer::tor::dial(&onion).await {
            Ok(address) => break address,
            Err(e) if reached.elapsed() < Duration::from_secs(180) => {
                println!("not yet: {e}");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Err(e) => panic!("never reached: {e}"),
        }
    };
    println!("reached in {:.1?}", reached.elapsed());

    // The phone's side: its QUIC endpoint, sending to the bridge.
    let phone = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut buf = vec![0u8; 65_535];
    for size in [1, 1200, 1452] {
        let sent: Vec<u8> = (0..size).map(|i| i as u8).collect();
        let round = Instant::now();
        phone.send_to(&sent, bridge).await.unwrap();
        let (n, _) = tokio::time::timeout(Duration::from_secs(30), phone.recv_from(&mut buf))
            .await
            .expect("an answer in time")
            .unwrap();
        assert_eq!(&buf[..n], &sent[..], "the same {size} bytes back");
        println!("{size} bytes there and back in {:.2?}", round.elapsed());
    }
}
