# 0082 — Each user's computer is their own server; no instant wake-up

**Status:** Accepted as the release direction, 2026-11-02. Not built.
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
