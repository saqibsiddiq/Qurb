# 0082 — Each user's computer is their own server; no instant wake-up

**Status:** Accepted as the release direction, 2026-11-02; amended the same day (*No money of the owner's*, below). Not built.
**Date:** 2026-11-02

## What was asked

The owner, on the release: users must not depend on his laptop, which runs
today's rendezvous and relay through Tailscale Funnel
([staging.md](../staging.md)); they should be on their own hardware; and the
product is to stay simple. Then: no instant wake-ups -- the phone app is how a
user reaches things, and the desktop tells them when the phone is needed.

## Decided

- **The user's computer is the server for their own devices.** The phone
  reaches it directly. The app makes it reachable without setup:
  - the home router asked to open a port (UPnP / NAT-PMP / PCP), renewed;
  - IPv6 used where the networks have it, the path opened from both ends;
  - its current address, signed with its own key, published to the public
    BitTorrent DHT (the pkarr method) whenever it changes, and looked up by
    the phone: no domain, no dynamic-DNS account;
  - no web certificates: connections carry the devices' own identities.
- **The phone tests it**: the first time it is off the home Wi-Fi it tries
  the computer's public address; success marks the computer *Reachable from
  anywhere*.
- **Plan B, switched on by itself when that fails**: a small introduction
  service on Cloudflare (Workers and Durable Objects, about $5 a month,
  estimated from published prices to cover hundreds to about a thousand
  users, not measured), only to introduce the two devices so they can
  hole-punch. It carries no files and stores nothing. Reached at a domain we
  own, so it can move without an app update.
- **No relay of ours.** The users whose connection blocks both direct paths
  (carrier-grade NAT, no IPv6, hole punching failing -- estimated at 1 in 10
  to 1 in 20, from published figures, not measured) work on their home
  Wi-Fi, and may set their own server in Settings › Advanced.
- **No instant wake-up.** Nothing holds a Google push key. When the desktop
  needs the phone -- an approval to open the person's folder -- the desktop
  says so, and opening the app connects at once. Everything else waits for
  the phone's background sync (about every 15 minutes) or the app being
  opened.
- **What the user sees**: one status on the desktop's device page --
  *Reachable from anywhere*, *Reachable through QurbSpace helper*, or *Only on
  your home Wi-Fi* with why, and the optional own-server link.

## Consequences

- The owner's laptop leaves the picture: the beta's built-in rendezvous
  address moves to the domain we own.
- Firebase push (decision on push in `crates/signal`) is no longer part of
  the release; removing it from the phone app is a follow-up, which also
  makes the app lighter.
- Supersedes the staging arrangement of [staging.md](../staging.md) for the
  release, not for the beta now running.

## Not yet known

- The owner's own connection: whether his laptop has public IPv6 and his
  router answers UPnP.
- Speeds over the internet, direct, and desktop to desktop: not measured.
- Whether pairing passes the computer's rendezvous to the phone today.

## Amended, 2026-11-02: no money of the owner's

The owner: the project is open source and earns him nothing, so nothing in
it may cost him money. So there is **no paid service anywhere**, Cloudflare
included, and plan B changes:

- **The meeting point is public Nostr relays**, free, run by volunteers and
  companies, no account. The desktop listens on several (4–5); the phone posts
  its current address there, encrypted to the desktop, as an ephemeral event
  relays do not keep; both then hole-punch and the relays play no further
  part. A relay sees encrypted bytes between two keys, never files or names.
  Measured from the owner's laptop on 2026-11-02, 13:50 IST: 7 of 10 public
  relays carried a test event back, in 0.18–0.43 s; the other three timed
  out, dropped it, or failed certificate checks. A 24-hour repeat every 30
  minutes is running; its figures choose the built-in list.
- **Better hole punching** (Tailscale's published technique: a spread of
  likely ports at once) for the stricter NATs.
- **The IPv6 nudge**: a device that cannot be reached from outside says so in
  plain words, and asks the person to have their provider turn IPv6 on.
- **Tor as the slow last resort**, run inside the app (Arti, in Rust): for
  opening files away from home when no direct path exists; backups wait for
  home Wi-Fi, also out of fairness to volunteers' bandwidth.

## The owner's connection, measured 2026-11-02

Router PPC 2K15X; WAN IPv4 10.107.0.128 (private: behind the ISP's CGNAT,
public address 192.140.152.113); IPv6 configured but down; no UPnP or
NAT-PMP. STUN from three servers saw the same public port as the local one
each time: endpoint-independent mapping, kept port. A packet from the phone
on Airtel mobile data to that port was not let in: the CGNAT filters
unsolicited packets. So his laptop is reached only by hole punching with an
introduction, which is what the meeting point is for. That afternoon, from
12:59, Tailscale Funnel's public route to his laptop failed mid-TLS and did
not recover after restarting tailscaled or resetting the Funnel, while
Tailscale's status page said all was well: the dependency this decision
removes.

## The meeting point, built and tried, 2026-11-02

Built as a `nostr` signalling address (`crates/signal/src/nostr.rs`), spoken
to the connector exactly as the rendezvous service is. Live over the public
relays from the owner's laptop, three runs of `tests/nostr_live.rs`: the ask
heard in 0.17–0.21 s, both sides told to punch 0.40–0.52 s after it.

Then on the owner's devices, both switched to `nostr` (the laptop's config
`signal = nostr`; the phone built with it as its default), no Tailscale: the
S23 on Airtel mobile data opened the laptop's *Shared*, and the laptop logged
`connected peer=f63a0f05 candidate=106.202.97.42:47568` at 14:30:27 IST --
**a direct connection to the phone's mobile address**, through both carriers'
CGNATs, introduced by the public relays. The first attempts before it timed
out on the addresses the phone had announced while on Wi-Fi; not yet looked
into, and to be with the hole-punching work.
