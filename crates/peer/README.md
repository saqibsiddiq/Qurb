# qurb-peer

Talking to another device: the wire format, a QUIC transport, and the client
that turns a sync plan into bytes over the network.

Sits **above** the engine rather than below it. The engine knows nothing about
networks; this crate adapts it to one.

```
PeerServer     serves a store, read-only, to peers it recognises
PeerClient     what do you have · what is this file made of · send me a chunk
NetworkSource  plugs the client into the engine's ContentSource seam
```

## Transfer is incremental

Fetching a file asks first for its **manifest** — the list of chunks it is made
of — and then only for the chunks this device does not already hold.

Measured on a 200 MB file, inserting 16 bytes at offset 1000, over loopback on
this machine:

| | first sync | after the 16-byte edit |
|---|---|---|
| chunks requested | 306 | **1** |
| bytes transferred | 190.73 MiB | **248.10 KiB** |
| share of the file | 100% | **0.1%** |

An insertion shifts every byte after it, which is the case fixed-size blocks
handle worst — Dropbox's 4 MB blocks would have re-sent the entire file. This is
the payoff of [decision 0004](../../docs/decisions/0004-chunk-parameters.md),
and the reason content-defined chunking is worth its complexity.

The saving is on the **wire only**. Reassembly is still whole-file: the client
concatenates chunks, hands the result to the engine, and the engine re-chunks it
to store. Network cost scales with the edit; local CPU still scales with the
file.

## Several chunks at once

A file's chunks are asked for eight at a time (`IN_FLIGHT`), and written and
checked in order as they arrive. QUIC carries each request on its own stream
and the server answers them concurrently; asking for one and waiting for it
before asking for the next made a file move at the pace of the round trip
rather than the link. The cost is memory — eight chunks, 4 MiB at the average
size — and it is the reason this was once left for later: a phone's ceiling
([decision 0018](../../docs/decisions/0018-file-contents-never-cross-the-ffi.md))
is far above it.

## A fetch that was cut off carries on

Until 2026-10-04 an interrupted fetch started again from nothing, and an
800 MB video pulled from a phone whose pass kept ending never arrived. Now
the partial file is kept, and the next attempt
([`PeerClient::resume_point`](src/client.rs)) chunks it: boundaries are chosen
by content, so the start of a file chunks the same whether or not the rest is
there. The chunks that match the sender's manifest, in order, are kept; the
file is cut back to the end of the last of them and only the rest is fetched.
Nothing is kept unchecked — a partial file left by another version of the same
path keeps only what the two share. See
[decision 0050](../../docs/decisions/0050-large-files-from-a-phone.md).

## Identity

Both ends present a self-signed certificate and check the other against a
fingerprint given in advance. **Both also verify the handshake signature**, so a
peer must hold the matching private key rather than merely replay a public
certificate — skipping that is the standard way pinned TLS is got wrong, and it
fails open.

A device's fingerprint is the BLAKE3 hash of its certificate. Hashing the whole
certificate rather than the key inside it avoids parsing X.509 to compare
identities and is equivalent here: the certificate is self-signed, so it binds
the key it contains, and a device keeps exactly one.

`PeerClient::connect` demands a fingerprint rather than offering a way to skip
one. An API with a "trust whoever answers" path would make the insecure option
the easy one.

## Pairing

Where the fingerprint comes from. An invite carries the inviter's **full**
fingerprint, its address, a one-time token and an expiry, encoded for a QR code
or for someone to read aloud.

The security is in the out-of-band channel: an attacker in the network path
cannot change what is printed on a screen. The token authenticates nobody — it
lets the inviter tell a device that saw the invite from one that found the port
open, and makes an invite single-use.

Both sides record the other in a trust store binding device id to fingerprint,
and `PeerServer::bind_trusting` takes its guest list from there. See
[decision 0014](../../docs/decisions/0014-pairing.md).

## The wire format

Hand-rolled and explicit. Every field is length-prefixed and bounded, so a
malformed or hostile message fails a check rather than allocating whatever it
asks for — without a cap, one four-byte length field is an out-of-memory attack.

Framing is left to QUIC: each exchange is one bidirectional stream, so the
stream's own end marks the end of the message. Streams are cheap and
independent, which is why a large chunk in flight does not hold up the small
requests behind it.

Decoding is tested against truncation at every offset, trailing bytes, absurd
lengths, unknown tags, and invalid UTF-8.

## The server is read-only

A peer can ask what this device has and ask for its bytes. It cannot tell this
device to change anything. Incoming versions are adopted by the *local* engine
only after it has run them through reconciliation, so nothing a peer says is
applied without this side deciding it should be.

There is one message that goes the other way, and it is worth knowing why it is
not an exception to the rule. `Got { content }` says "I now hold these bytes" —
the only request in the protocol that asks for nothing. It changes no file and
no version; it records that somebody else has a copy, which is what lets this
device stop calling that content undelivered and what a storage cap consults
before dropping a local copy. It is credited to the certificate the connection
authenticated with, never to anything the message claims, so one peer cannot
report delivery on another's behalf.

## Trying it

```bash
cargo run --release --example transfer -- /tmp/dev-a /tmp/dev-b
```

The Phase 1 kill criterion — generate a tree, sync it, verify it, edit part of
it, sync again, verify again. Needs roughly 20 GiB free at 100k files:

```bash
cargo run --release --example scale -- /tmp/scale 100000
```

## A peer can waste your time, not corrupt your store

`tests/hostile.rs` runs against peers that send the wrong bytes, promise chunks
they lack, answer with nonsense, reply to the wrong question, or go silent
mid-session. After every one, the local store still verifies clean and holds
nothing.

Content is addressed by hash, so every byte that arrives is checked against what
was asked for. Authentication says *who* is talking; it says nothing about
whether they are telling the truth — and relays, which are Phase 3, will forward
traffic nobody here controls.

## Getting through a router

`nat` asks a public STUN server what address this machine's packets appear to
come from, and classifies the router by asking two operators from **one socket**:
if the external port is the same either way, the mapping does not depend on the
destination and hole punching can work.

The socket matters more than it looks. A router's mapping belongs to one local
port, so the socket that did the discovery and the punching must be the one QUIC
then runs on — `PeerClient::connect_on` exists for exactly that. Using a fresh
socket works in a lab and fails on every real network.

```bash
cargo run --release --example netcheck
```

## Reaching a peer, start to finish

`Connector` is the policy that sequences everything else:

```
start    bind a socket, ask STUN where it appears from, announce
reach    ask the rendezvous service for a peer, be told to punch
race     try every candidate at once, keep the first that answers
```

**Both sides dial.** A router only lets a packet in if it has recently seen one
go out to that address, so a device sitting idle has punched nothing and the
caller's packets arrive at a router that has never heard of them. A QUIC
handshake begins with packets that *are* the punch, so both devices dial when
told to. Whichever handshake completes carries the traffic; the responder's is
deliberately doomed — it pins its own fingerprint — and exists only to have sent
something.

**Discovery happens before the endpoint is built.** Once QUIC owns the socket
nothing else can send through it, which is why `Connector::start` runs STUN
first and why there is no explicit punch step afterwards. Getting this backwards
produces code that looks right and cannot work.

**The relay is the fallback, not the plan.** When every candidate fails,
`reach` goes the long way round — same pinned certificate, because the relay
carries the handshake without being party to it. The relay connection is held
open rather than dialled on demand, since being reachable is not something that
can be arranged after someone has already failed to reach you.

**Dropping a connector stops everything it started.** The rendezvous
reconnect loop, the beacon sender and listener, and with them the socket. A
daemon keeps one connector for hours and would not notice; a phone builds one
per sync pass, and until this was so every pass left its background work
running for the life of the process — announcing an address nothing accepted
on any more. `tests/lifetime.rs` counts the runtime's tasks before and after.

**Candidates are raced, not tried in turn.** An unreachable address fails by
timing out, so three in sequence means three timeouts before discovering the
last one worked. Local addresses come first, so two devices on one network do
not route through the internet to reach each other.

## Pairing stops when the invite does

`PairingHost::wait` is bounded by the invite's remaining lifetime and returns
`Error::InviteExpired` when it runs out.

That is worth stating because it did not, and the failure was invisible. `wait`
took `now` as a parameter and checked `is_expired(now)` *inside* the accept
loop — so the clock it compared against was the one captured before the wait
began, and the check could never fire however long the wait lasted. Nothing else
bounded the loop, so `qurb pair` sat advertising a code that had stopped working
five minutes in, with the screen still saying "Waiting...". It was found on a
terminal that had been waiting two and a half hours.

## The guest list is live

`TrustList` is shared with the TLS verifier rather than copied into it, so a
device paired while a listener is running is accepted without a restart.

It used to be a snapshot taken at startup. Pairing happens in a separate
`qurb pair` process, so the daemon could not know about it — and "pair once"
silently meant "pair once, then restart the daemon". The daemon now re-reads the
trust store every five seconds, which is a handful of rows describing one
person's own devices, and syncs immediately when something new appears:
measured at **5 seconds** from scanning a code to the daemon acting on it,
against 120 before.

The list is replaced wholesale rather than added to, because the trust store is
the authority: a device *forgotten* there must stop being accepted here too, and
a set that only grew would keep letting it in.

## Not yet built


- **Upgrading back to direct.** A connection that fell back to the relay stays
  relayed, even after the device moves to a network where punching would work.
- **A measured direct-connection rate.** Hole punching works between the
  networks tested here — a home network, and a phone on one mobile carrier
  reaching it directly. The rate across real networks
  is the number the relay bill depends on, and it needs a second machine — see
  [measuring-connectivity.md](../../docs/measuring-connectivity.md).
- **Transitive trust.** Pairing A to B and B to C does not pair A to C, and
  removing a device is done on each device that trusted it
  ([decision 0041](../../docs/decisions/0041-removing-a-device.md)); nothing
  tells the others.
- **Tree paging.** The whole tree is sent in one message, capped at 64 MiB. A
  large library needs incremental exchange rather than a full dump per sync.
- **A measured gain from the two sections above** over a real link. They
  were built after an 800 MB video failed to cross from a phone; the speed
  before and after, on that phone and Wi-Fi, has not been measured yet.
