# qurb-relay

The fallback path, for when two devices cannot reach each other directly — a
symmetric NAT at both ends, or UDP blocked outright.

```
device ──TCP──► relay ◄──TCP── device
         a QUIC session runs inside, end to end
```

## It carries datagrams, not messages

This is the decision everything else follows from.

A relay that understands application messages is a relay that can read and alter
them, and preventing that means a second cryptographic protocol solving a problem
the first one already solved.

So the relay forwards **datagrams**, and an ordinary QUIC session runs inside:
same pinned certificates, same handshake, same encryption. Nothing above the
transport knows the relay is there. The relay sees ciphertext addressed to an
identifier it cannot link to a person, and can only pass bytes on or drop them —
and dropping is what every router between any two computers can already do.

`RelaySocket` implements quinn's `AsyncUdpSocket`, so a relay connection *is* a
socket as far as QUIC is concerned. Relayed peers are given addresses from the
RFC 5737 documentation range, which is never routed — if one appears in a log it
is unambiguously relayed rather than somewhere a packet really went.

The mapping between those addresses and relay identifiers goes both ways and is
allocated on demand, because a device does not know in advance who will call it.
A socket that could only dial one known peer would be unable to listen, and a
device that cannot be *reached* over the relay makes the fallback useless in
exactly the case it exists for.

`the_relay_never_sees_the_content` sends a distinctive string through a relay
that records every byte it forwards, and asserts the string never appears.

## TCP, when everything else is UDP

The data plane is QUIC over UDP because it is faster. The relay is TCP because
**it exists for networks where UDP does not work**, and belongs on port 443 for
the same reason. A fallback that needs the thing being fallen back from is not a
fallback.

## Who may use it

Devices register under the identifier the rendezvous service uses, derived from
the user's master key, so the relay learns nothing about whose traffic it
carries. A connection that has not registered cannot forward anything, so this
is not an open proxy for whoever finds the port.

Registration is self-asserted. The identifier is not guessable, so claiming
someone else's means already knowing it — which means being inside a group whose
devices have paired. **The residual risk:** someone who learns an identifier can
receive that device's relayed packets. They cannot read them, but they can stop
the real device getting them. See
[decision 0017](../../docs/decisions/0017-relay.md).

## Not yet built

- **Relay selection**, and moving between relays.
- **Upgrading back to direct.** A connection that fell back stays relayed even
  after the device moves to a network where punching would work.
- **Fairness and quotas.** One device can use all the capacity.
- **TLS of its own.** Termination is a reverse proxy's job today.
