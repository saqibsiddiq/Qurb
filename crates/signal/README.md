# qurb-signal

The rendezvous service: how two devices behind home routers learn where the
other is, and agree to punch a hole at the same moment.

```
announce   I am here, at these addresses
connect    I want to reach this member of my group
punch      both sides told at the same moment
```

## What it is not allowed to know

No filenames, no chunks, no keys. It does not even learn *whose* devices these
are — identifiers are derived from a master key the server does not hold:

```
group  = KDF(master, "rendezvous-group/v1")
member = KDF(master, "rendezvous-member/v1", fingerprint)
```

Every device of one user derives the same group, so the server can match them
without being told they are related. A member identifier depends on the
fingerprint, which pairing has already established, so a peer can work out where
to look while the server sees only opaque bytes.

Run naively — devices registering under an account name, or their real
fingerprints — the server would learn every user, their devices and their
locations over time. That is a more valuable database than the files it was
carefully prevented from seeing.

**What it still sees:** IP addresses, and which groups are active when. That is
inherent to being a rendezvous point, and it is why the data plane never goes
near it. See [decision 0016](../../docs/decisions/0016-what-signalling-learns.md).

## Two design points that look arbitrary and are not

**Websockets over TCP, not QUIC.** The data plane is QUIC, and reusing it here
would be tidier — and would fail exactly when needed. If UDP is blocked, which is
precisely when a device most needs coordinating, a UDP control channel cannot
tell that device it needs a relay. The channel that arranges the fallback has to
work where the fallback is needed.

**The connection is held open.** Hole punching needs both routers to see an
outbound packet at roughly the same time, and a device that polls to discover it
should punch will always be late. The server tells both sides at the same
moment, which is the feature the transport was chosen for.

## Cleaning up is load-bearing

A device left in the directory after it disconnects means peers are handed a
dead address and spend their punch attempts on it.

The subtle case is an *abrupt* disconnect — laptop shut, network dropped — which
is the common one rather than the exception. An early return on the resulting
error would skip the cleanup it was meant to trigger.

## Deployment

**Behind TLS.** The group identifier is a bearer secret: anyone holding one can
enumerate that group's addresses. `SignalClient::connect` refuses a plain
`ws://` URL to anywhere but the local machine; `connect_insecure` exists for
tests and says what it is.

## Not yet built

- **Serving TLS itself.** Termination is currently a reverse proxy's job.
- **Rate limiting and abuse controls.** Anyone can open a connection and
  announce into a group they invent.
- **Persistence.** The directory is in memory, so a restart makes every device
  re-announce. Acceptable for a rendezvous point; not for anything else.
- **Horizontal scaling.** One process holds every connection, so devices must
  reach the same instance to find each other.
- **Wiring into the engine.** The pieces — discovery, punching, rendezvous —
  exist separately. Nothing yet sequences them into "try direct, fall back".
