//! What this network will let you do.
//!
//! ```bash
//! cargo run --release -p qurb-peer --example netcheck
//! ```
//!
//! Classifies the router between this machine and the internet, which decides
//! whether devices can reach each other directly or need a relay. The Phase 3
//! kill criterion is a direct-connection rate of roughly 70%, and this is the
//! instrument it is measured with — so run it on every network that matters:
//! home, phone hotspot, office, café. The worst result is the one that sets the
//! bandwidth bill.
//!
//! It reveals this machine's public IP address to the STUN servers it asks,
//! exactly as any VPN or video-call client does.

use qurb_peer::nat::{self, NatBehaviour};
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

fn main() {
    let timeout = Duration::from_secs(3);

    // One socket for every query. That is the whole test: whether the external
    // address depends on who is being asked.
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not open a UDP socket: {e}");
            std::process::exit(1);
        }
    };

    println!("local   {}", socket.local_addr().expect("local address"));
    println!();

    let mut servers = Vec::new();
    for name in nat::DEFAULT_STUN_SERVERS {
        match name.to_socket_addrs() {
            Ok(mut addrs) => match addrs.find(|a| a.is_ipv4()) {
                Some(addr) => servers.push((name, addr)),
                None => println!("  {name:<28} no IPv4 address"),
            },
            Err(e) => println!("  {name:<28} could not resolve: {e}"),
        }
    }

    let mut seen = Vec::new();
    for (name, addr) in &servers {
        let started = Instant::now();
        match nat::reflexive_address(&socket, *addr, timeout) {
            Ok(public) => {
                println!("  {name:<28} -> {public}   ({:?})", started.elapsed());
                seen.push(public);
            }
            Err(e) => println!("  {name:<28} -> no answer: {e}"),
        }
    }

    let behaviour = match seen.len() {
        0 => NatBehaviour::Blocked,
        1 => NatBehaviour::Inconclusive,
        _ if seen.windows(2).all(|w| w[0] == w[1]) => NatBehaviour::EndpointIndependent,
        _ => NatBehaviour::Symmetric,
    };

    println!();
    match behaviour {
        NatBehaviour::EndpointIndependent => {
            println!("verdict  ENDPOINT-INDEPENDENT MAPPING (cone NAT)");
            println!();
            println!("  The same external address whoever is asked, so this router's");
            println!("  mapping does not depend on the destination. Hole punching should");
            println!("  work and connections from here should mostly be direct.");
        }
        NatBehaviour::Symmetric => {
            println!("verdict  SYMMETRIC NAT");
            println!();
            println!("  A different external port per destination, so the address a peer");
            println!("  learns is not the address it can reach. Punching will usually fail");
            println!("  and connections from here will need a relay.");
            println!();
            println!("  One symmetric end is survivable if the other is not. Two is not.");
        }
        NatBehaviour::Blocked => {
            println!("verdict  NO STUN RESPONSE");
            println!();
            println!("  Nothing answered. Usually UDP blocked outbound, which means every");
            println!("  connection from here needs a relay on a port that is allowed.");
            println!("  Worth retrying before concluding -- an outage looks the same.");
        }
        NatBehaviour::Inconclusive => {
            println!("verdict  INCONCLUSIVE");
            println!();
            println!("  Only one server answered, and one answer cannot say whether the");
            println!("  mapping depends on the destination. Retry.");
        }
    }

    println!();
    println!("direct connections possible from here: {}", if behaviour.can_punch() { "yes" } else { "no" });
}
