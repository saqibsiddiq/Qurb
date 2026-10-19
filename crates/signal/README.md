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

**A connection is held in a few KiB.** The WebSocket library's default buffers
are 128 KiB to read and 128 KiB to write, per connection; measured, that was
about 150 KiB per connected device. They are now 4 KiB and unbuffered, with the
unsent backlog bounded: about 22 KiB per device, 30 KiB with the service's own
TLS. See [experiments/service-capacity](../../experiments/service-capacity/README.md).

**Connecting gives up after five seconds** (`client::HANDSHAKE_TIMEOUT`). A
refused or unroutable address fails by itself; a service that accepts the
connection and never answers used to be waited on for ever, and with it a
phone's sync, whatever deadline the phone had set, and a desktop's start. Found
on a phone, recorded in
[decision 0020](../../docs/decisions/0020-sync-takes-a-deadline.md#found-on-a-phone).

## Cleaning up is load-bearing

A device left in the directory after it disconnects means peers are handed a
dead address and spend their punch attempts on it.

The subtle case is an *abrupt* disconnect — laptop shut, network dropped — which
is the common one rather than the exception. An early return on the resulting
error would skip the cleanup it was meant to trigger.

## Deployment

**Behind TLS.** The group identifier is a bearer secret: anyone holding one can
enumerate that group's addresses. `SignalClient::connect` refuses a plain
`ws://` URL to anywhere but the local network; `connect_insecure` exists for
tests and says what it is.

## It relays too

`SignalServer::relaying` gives it a relay to carry: a WebSocket opened at
`/relay` -- or a path ending in it, behind a proxy that adds a prefix -- is
handed over as a byte stream (`tunnel.rs`), decided from the request line
before the handshake. `qurb signal` does this unless given `--no-relay`, and
without a relay `/relay` is refused with a 404, so a device looking for one
is told at once. That is what lets a relay be published wherever the
rendezvous is: through a proxy that carries only TLS by name, on port 443,
past networks that let only web traffic out. On the client, `tunnel::open`
opens it under the rendezvous connection's rules, and `tunnel::beside` says
where it is from the rendezvous URL. See
[decision 0068](../../docs/decisions/0068-the-rendezvous-service-relays.md).

## Arrivals are pushed, not polled

When a device announces itself, every other member of its group is told, with
the addresses attached so acting on the news needs no second round trip.

This was the half that was missing, and it is what makes syncing with a phone
work at all. The server always knew the moment a device appeared — it told only
that device. Everyone else had to discover it by asking, and a peer that asks on
a backoff will not be asking during the twenty-odd seconds a phone is awake in a
background window. Worse, the backoff grows *because* the phone keeps being
absent, so the two drift further apart the longer it goes on.

Measured before this existed: a laptop retrying every 120 seconds against a
phone announcing for 25 never once caught it, across repeated attempts. With the
push, the daemon logs `a peer is reachable and has news` within a second and the
transfer completes inside the phone's window.

## Saying you have something for someone

Appearing is one half. The other is a device that is *already* connected and
has just changed something: the peer has no reason to ask, so without being
told it waits for its own next poll — up to two minutes on a desktop, a quarter
of an hour on a phone.

`Waiting { to }` says there is something for a member. It carries who and never
what: no filenames, no sizes, no counts. The service forwards it if that member
is connected, and **keeps it if not**, delivering it the moment they announce.
Keeping it is the point — the device that most needs telling is exactly the one
that was asleep when the change happened.

Notes collapse: fifty changes for one absent peer leave one note, because the
answer to "should I sync" is the same either way. That also bounds the memory,
since a group cannot hold more members than its limit.

Measured on one machine, two idle daemons: a file written on one appeared on
the other **one second later**, with the recipient logging the notice one
millisecond after the sender recorded the change.

The notice carries a blinded `MemberId`, not a fingerprint, so the service still
cannot tell which device arrived — see
[decision 0016](../../docs/decisions/0016-what-signalling-learns.md). A
recipient recovers the fingerprint by re-deriving the identifier for each peer
it already trusts, which is a handful of hashes against a person's own devices.

## Waking a device that cannot be told

Everything above works only while a device is holding a socket open, and a
phone does not: Android stops a background app's connection within minutes of
the screen going off. So the device most in need of being told something is the
one that cannot be told.

[`wake::Waker`](src/wake.rs) is the seam. The default does nothing — a service
with no credentials behaves exactly as it did before push existed, which is
correct rather than degraded. [`fcm`](src/fcm.rs), behind the `push` feature,
fills it in with Firebase.

Two rules decide when, and the second is the one worth stating:

- Only when the device could not simply be told.
- Only because the device asking is **there to sync with**. Waking a phone for
  a peer that is not itself online spends its battery to find nobody.

The poke carries nothing — no filenames, no sizes, not even which peer. A woken
device syncs with the peers it already knows, so there is nothing useful to put
in it and every reason not to: the push service sees the message.

Measured on a Galaxy S23 asleep with its screen off: a change on a laptop at
`13:22:18.579412`, a push at `.579876`, and the phone logging "woken by another
device" at `13:22:19.281` — **seven hundred milliseconds**. See
[decision 0028](../../docs/decisions/0028-waking-a-sleeping-device.md) for what
the dependency costs.

What Firebase refuses is told apart (`fcm::Refused`). A token it says is gone
-- `UNREGISTERED`, or not a valid token -- is forgotten by the service and not
tried again; until 2026-10-18 it was poked on every wake for as long as the
service ran. A refusal of the service's own access token is met by fetching a
fresh one and sending once more. The access token's hour is counted by the
wall clock: counted on `Instant`, which stops while Linux is suspended, a
token past its hour was still used after the laptop slept, and wakes were
refused until it was replaced.

## Not yet built

- **Abuse controls beyond the basics.** There are limits on connections, group
  size, message rate and message size, and none of them survive an attacker with
  many addresses. That needs infrastructure this service does not have.
- **Persistence of the directory.** Who is connected is in memory, so a
  restart makes every device re-announce, which heals itself on the next
  connection. Wake tokens are the exception: with push on they are kept in
  `wake-tokens.json` in the state directory, readable only by the service's
  user, because a forgotten token leaves a sleeping phone unreachable until it
  next syncs on its own.
- **APNs.** The Android half of waking a device is built and measured; iOS
  would use the same [`wake::Waker`](src/wake.rs) seam, and nothing has been
  written against it.
- **Horizontal scaling.** One process holds every connection, so devices must
  reach the same instance to find each other.

