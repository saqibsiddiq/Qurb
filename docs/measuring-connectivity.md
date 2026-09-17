# Measuring the direct-connection rate

The Phase 3 kill criterion is a direct-connection rate of roughly 70% across
real networks. Below that, bandwidth cost scales with users and the economics
of the whole project change.

It cannot be measured from one machine, and it has not been measured. Everything
needed to do it now exists.

## What is actually being measured

Not "does this network allow hole punching", which `qurb netcheck` answers on its
own. The number that matters is **how often two devices on two different
networks establish a direct path**, because a connection needs both ends to
cooperate and one symmetric NAT is survivable while two are not.

So the unit of measurement is a *pair of networks*, not a network.

## The cheap first pass

`qurb netcheck` on each network you care about. It reveals your public address
to two public STUN servers, exactly as any video-call client does.

```bash
qurb netcheck
```

Run it on home wifi, a phone hotspot, an office, a café, and anywhere with a
corporate firewall. Record the verdict for each. A network that reports
"needs a relay" will need one against *every* peer; a network that allows direct
connections still might not reach a particular peer.

This bounds the answer without proving it: if most networks allow direct
connections, the pairwise rate will be high.

## The real measurement

Two machines, two genuinely different networks — not two machines on the same
wifi, and not a VPN, which changes the answer entirely.

**Somewhere reachable**, run the rendezvous service and a relay:

```bash
qurb signal 0.0.0.0:9000
qurb relay 0.0.0.0:9001
```

A small VPS is enough. Neither serves TLS of its own, so put them behind a
reverse proxy before they face the internet — the identifiers devices announce
under are bearer secrets.

**On each machine**, set up a device and point it at those:

```bash
qurb init ~/Sync
qurb config ~/Sync signal=wss://your-server:9000 relay=your-server:9001
```

Enrol the second with the first's recovery phrase, then `qurb pair` on one and
`qurb join` on the other.

**Then watch what happens.** Run both daemons with debug logging:

```bash
RUST_LOG=qurb=debug,qurb_peer=debug qurb run ~/Sync
```

A direct connection logs `connected` with the peer's address. A fallback logs
`no direct path; falling back to the relay`. The relay's own output reports how
many bytes it has carried, which is zero if every connection went direct.

Repeat across pairs of networks. The rate is direct connections over attempts.

## What counts as passing

Above ~70%, relay capacity is a modest fixed cost and the architecture holds.

Below it, the cost of carrying other people's data starts to track user growth,
which is the thing the whole design was meant to avoid. That would not be a
reason to stop — it would be a reason to revisit
[decision 0006](decisions/0006-availability-gap.md), because a paid replica that
is also a relay looks very different when most traffic needs relaying anyway.

## Recording the result

In [phases/phase-3-networking.md](phases/phase-3-networking.md), with its
conditions: which networks, which carriers, how many attempts, and the date.
A rate without that is not evidence, and mobile carriers in particular differ
enough that one country's answer is not another's.
