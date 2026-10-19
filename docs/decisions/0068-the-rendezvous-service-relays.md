# 0068 — The rendezvous service relays too, and a device finds it there

**Status:** Accepted, 2026-10-15, from the owner's report of that day.
Built and deployed on the owner's laptop the same day: the relay answers at
`wss://saqib.tail86283c.ts.net/relay` through the Tailscale Funnel, and the
laptop's daemon is registered on it. **Not yet seen carrying a sync between
the S23 on another network and the laptop**: that is the test that decides
it. Amends [0017](0017-relay.md), whose relay was reached only over TCP on a
port of its own.
**Date:** 2026-10-15

## What was asked

The owner, 2026-10-15:

> i was unable to sync files when my phone was connected to my another
> phone's wifi even though the application is designed in a way that it can
> sync file as long as both the devices are online

## What was found

Nothing of the failed attempt survived to read: the phone's log had rotated
past it, and the laptop's was going nowhere after a restart. What stood
regardless:

- **No relay ran anywhere.** Built and tested since Phase 3, it was "next
  after design" and never deployed. A phone on a carrier's network, or on a
  hotspot sharing one, is often behind a NAT that hole punching does not get
  through; without a relay that is the end of it.
- **It could not have been published the way the rendezvous is.** The owner's
  rendezvous service runs on the laptop and is reached from outside through
  Tailscale Funnel, which carries TLS, routed by name. The relay spoke plain
  TCP on a port of its own, and nothing routes that to it.
- The phone's relay setting was empty, as nothing had ever said what to put
  there.

## Decision

**A rendezvous service carries a relay as well, at `/relay` on its own
address, and a device with no relay set uses that one.**

- `qurb signal` hands a WebSocket opened at `/relay` (or a path ending in it,
  behind a proxy that adds a prefix) to a relay in the same process
  (`qurb_relay::Relay`); `--no-relay` turns it off, and `/relay` is then
  refused with a 404 so a device is told at once. Every other path is
  signalling, as before. The path is read from the request line before the
  WebSocket handshake and handed back to it.
- Each WebSocket message carries bytes of a stream; the relay's own framing
  runs inside unchanged (`qurb_signal::tunnel`), and inside that the same
  QUIC session with pinned certificates as over any relay. The service sees
  nothing it can read, as before.
- A device's relay is the one its settings name: `host:port` for a relay on
  a port of its own, as before; a `ws://` or `wss://` URL for one carried by
  a rendezvous service; `off` for none. **Empty means the relay beside its
  rendezvous service** (`qurb_peer::relay_for`): the same address at
  `/relay`, a pinned certificate kept. The owner's devices need nothing set:
  the laptop finds `ws://localhost:9000/relay`, the phone
  `wss://saqib.tail86283c.ts.net/relay`.
- The relay connection is opened while the connector asks STUN for its
  public address, not after: through the Funnel it is a TLS WebSocket's
  round trips, at the start of every pass a phone makes.
- An idle tunnel is pinged every 30 seconds, so a proxy on the way does not
  close a connection that is only quiet.

## Why this way

- **It gets through.** A WebSocket over TLS on 443 is web traffic, which is
  what a restrictive network lets out last, and what any proxy publishes: a
  Funnel, Caddy on a VPS, a load balancer. A relay on a port of its own needs
  that port open, and a publisher that carries raw TCP.
- **Nothing to configure.** Every device already knows its rendezvous
  service; a relay beside it is found from that. A setting nobody knew to fill
  in was a relay nobody used.
- **One service to run.** The owner's plan is his own VPS (2026-09-27); one
  process, one port, one name there, as on the laptop now.

## Costs, accepted

- **Relayed bytes cross the rendezvous host.** On the laptop through the
  Funnel, that is Tailscale's infrastructure and the laptop's upload; how
  fast, and whether Funnel limits it, is unmeasured. A relay on a port of its
  own remains possible, and preferable for a service carrying many people.
- **The relay is reachable by anyone who can reach the rendezvous.** A
  connection must register before it forwards, but registration is
  self-asserted (0017): two connections that each register a name of their
  own can relay between themselves. Someone would have to write a client for
  qurb's framing to do it; it costs the host bandwidth, never a file. On the
  laptop it is turned off by adding `--no-relay` to the service's
  `ExecStart`.
- **An older rendezvous service does not answer `/relay` as a relay.** It
  takes the WebSocket for signalling and ignores the relay's binary frames,
  so a device would hold a relay connection that never delivers, and fall back
  to it in vain. The only one was the laptop's, replaced the same day.

## Rejected

- **TLS on the relay's own port, published by Funnel's TCP mode.** Funnel can
  forward TLS by name to a port, but that is a second publishing arrangement
  to keep, a second certificate story on a VPS, and still a port a
  restrictive network may close.
- **The relay inside the rendezvous's signalling messages.** Signalling is
  JSON text in small, rate-limited messages, by design; datagrams through it
  would need its limits lifted for everyone.

## Amended 2026-10-18: staying on the relay

Found reading the code while hunting faults, not in use: a device's relay
connection was opened once. When it went -- the rendezvous service restarted,
or a laptop's daemon started at login before it -- the device stayed off the
relay until it was itself restarted, and the socket reported the loss to QUIC
as a failed socket, which ends the whole relayed endpoint. The laptop's daemon
holds one connector for hours, so a restart of the service there cost the
phone its way round for the rest of the session.

Now the relay socket dials again when its connection goes, waiting one second
and doubling to a minute, and registers every name it holds on the new
connection -- its own and each meeting's (0061). Meanwhile what QUIC sends
through it is dropped, as over any lost path. A device whose relay cannot be
reached when it starts keeps the socket, knows it is down, and does not dial a
peer through it until it is up; the same for a rendezvous service that carries
no relay, which a device now asks again once a minute rather than never.
Tested by restarting a relay under a connected socket
(`a_socket_finds_its_relay_again_after_a_restart_with_every_name`) and by
starting a socket before its relay (`a_socket_started_before_its_relay_gets_on_it`).

## Reversing it

`--no-relay` on the service, and `relay = off` on a device. The relay on its
own port is unchanged, and a setting naming one still wins.
