# 0027 — Plaintext rendezvous stops at the local network

**Status:** Accepted
**Date:** 2026-09-22

## Decision

A plain `ws://` rendezvous URL is accepted for **this machine and the local
network** — loopback, RFC 1918 and link-local addresses — and refused for
anywhere else. Reaching a rendezvous service across the internet requires
`wss://`, and the client now has TLS to do it with.

Carrier-grade NAT (`100.64/10`) is explicitly *not* local. A phone on mobile
data shares one of those with the rest of the carrier.

## Why this came up

qurb's whole promise is syncing from anywhere. Tested from mobile data for the
first time, it did not sync at all, and the reasons were instructive:

1. The phone was configured with `ws://192.168.1.4:9000` — the laptop's address
   on its home network, which from a mobile carrier is not an address at all.
   Nothing could have worked.
2. There was no rendezvous service reachable from the internet to point it at.
3. And the client could not have used one securely if there had been:
   `tokio-tungstenite` was built without a TLS feature, so `wss://` was not
   merely discouraged, it was impossible.

The third is the one worth a decision record, because the code already
contained the right rule and did not apply it. `SignalClient::connect` refused
plaintext to a remote host, exactly as its documentation said — and every
caller in the codebase used `connect_insecure` instead. The guard existed, was
documented, was tested, and was bypassed everywhere.

## Why the local network is exempt

The obvious rule is "always require TLS". It is wrong here, and would have
broken the arrangement qurb actually ships with today: a laptop and a phone on
one Wi-Fi, rendezvousing through a service on the laptop at `192.168.1.4`.

No certificate authority will issue a certificate for `192.168.1.4`. Requiring
TLS there means requiring something unobtainable, and the practical result
would be people disabling the check rather than satisfying it.

The risk is also different in kind. The identifiers are bearer secrets: anyone
who sees one can enumerate that group's addresses. On a home network the people
who can see the traffic are the people already in the house. Across the
internet they are whoever runs the café, the carrier, and every hop between.

So the boundary is drawn where the risk changes, rather than where it is
easiest to state.

## What this does not solve

**There is still nowhere public to point a device at.** The rendezvous service
runs wherever someone runs it, and today that is a laptop on a home network.
Syncing from anywhere needs it somewhere with a public name and a certificate.
That is a deployment decision, not a code one, and it is not made here.

Only the *rendezvous* needs to be publicly reachable. The measured NAT on the
development laptop is endpoint-independent, so once two devices learn each
other's addresses the QUIC connection between them is direct, and the files
never touch the service. A relay is the fallback for networks where that fails,
and it has the same deployment question.

**A hostname is treated as remote**, even one that resolves to a private
address. It could resolve anywhere, and a name is precisely the case where a
certificate is obtainable — so the rule costs nothing there.
