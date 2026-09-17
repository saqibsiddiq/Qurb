# Phase 3 — Networking at scale

**Status:** in progress
**Target:** months 6–7
**Kill criterion:** direct-connection rate below ~70% across tested networks.

Phase 1 built a transport and Phase 2 hardened it. What neither did was answer
how two devices find each other, or how they decide they belong to the same
person.

## Progress

| area | status |
|---|---|
| device pairing | ✅ out-of-band invite, trust store, guest list enforced |
| STUN and hole punching | ✅ STUN client, NAT classification, punched sockets |
| signalling | ✅ [`qurb-signal`](../../crates/signal/) — in Rust, see 0015 |
| connection policy | ✅ [`connect`](../../crates/peer/src/connect.rs) |
| relay | ✅ [`qurb-relay`](../../crates/relay/) |
| signalling hardening | ✅ rate limits and resource caps |
| automatic fallback | ✅ direct first, relay when it fails |
| storage-only replicas | ✅ the answer to decision 0006 |

389 tests pass across nine crates; clippy is clean.

## Pairing

The gap recorded three times in earlier phases: the transport authenticated
peers against a fingerprint the caller had to supply, and nothing decided what
that fingerprint should be. A pin only protects you if you learned the right
one.

An invite now carries the inviting device's full fingerprint, its address, a
one-time token and an expiry, encoded for a QR code or for someone to read
aloud. Recorded as [decision 0014](../decisions/0014-pairing.md).

**The security is in the out-of-band channel, not the token.** An attacker in
the network path cannot change what is printed on a screen. Carrying the whole
fingerprint across that channel is what authenticates the inviter — it is the
same pinning every other connection already used, with the pin finally coming
from somewhere trustworthy.

The token authenticates nobody. It lets the inviter tell a device that saw the
invite from one that found the port open, and makes an invite single-use. Worth
being explicit about, because reading it as *the* secret would invite someone
later to shorten the fingerprint and rely on the token instead, which would be a
real weakening.

### Base32, and why the encoding is not arbitrary

Invites are base32 rather than hex because QR codes have an alphanumeric mode
covering exactly uppercase letters and digits, which encodes around 40% more
densely than the byte mode hex would force. A smaller QR code scans from further
away and in worse light, which is the entire job. The alphabet excludes `0`,
`1` and `8` because they are confusable with `O`, `I` and `B` when read aloud,
and decoding accepts lowercase and ignores grouping dashes, because people
retyping a code will not respect either.

### The binding this closes

[Decision 0011](../decisions/0011-peer-identity-pinning.md) recorded that
`DeviceId` in the index and `Fingerprint` on the network were unrelated, and that
nothing proved the device whose version vectors you were merging was the device
you had authenticated.

Schema v3 adds a `peers` table binding the two, written only by pairing. TLS
proves the peer holds the key behind a fingerprint; the table says whose device
that is. `PeerServer::bind_trusting` takes its guest list from there, so the
system finally has a notion of "my devices" rather than a list the caller
assembled.

### Tested against what it has to withstand

Fourteen tests, most of them attacks: a device that never saw the code, an
expired invite, a code replayed after someone else used it, an invite whose
fingerprint was swapped for an impostor's, a peer choosing a name full of
control characters and 500 characters long, and re-pairing a device that already
had a certificate.

The last one matters more than it looks. Re-pairing must *replace* an identity
rather than accumulate one, or a compromised certificate stays trusted forever.

The end-to-end test is the one that shows the point: a paired device syncs, and
a stranger with a perfectly valid certificate that knows the host's fingerprint
is refused.

## NAT traversal

Two devices on home networks have no address the other can dial. Each sits
behind a router doing address translation, and an unsolicited packet arriving
there is dropped because it belongs to no conversation the router knows about.

```
1. ask a public server what address our packets appear to come from   (STUN)
2. exchange those addresses through something both can reach          (signalling)
3. both send to the other at the same time                            (punching)
```

Step 3 is the trick: neither packet is expected, so the first ones are dropped —
but each *outbound* packet teaches its own router to expect a reply from that
address, so what follows gets through.

### The detail that would have been silent to get wrong

A router's mapping belongs to **one local port**. Discovering an address on one
socket and then connecting on another gets a different mapping, and the hole was
punched for an address nobody is listening on.

So the same socket does the STUN query, the punching, and then QUIC. That is why
the functions here take a socket rather than making their own, and why
`PeerClient::connect_on` exists alongside `connect`.

This is the kind of mistake that works perfectly in a lab and fails on every
real network, so `a_socket_keeps_its_port_across_every_stage` asserts the port
survives each handoff, and two further tests run a real QUIC session over
sockets that punched first.

### What the tests can and cannot show

There is no NAT on loopback, so **none of this demonstrates traversal**. It
demonstrates the mechanism traversal needs. Whether traversal works is the kill
criterion, and it can only be measured across real networks.

### The STUN implementation, and one thing the spike got wrong

The Phase 0 spike parsed binding responses without checking the transaction
identifier. That is a real hole: an off-path attacker who can guess the port can
answer before the server does and choose what address this device believes it
has. The implementation now requires the identifier to match, which turns that
into a 96-bit guess.

Also handled properly rather than optimistically: attributes that are not
`XOR-MAPPED-ADDRESS` are skipped rather than treated as fatal, because servers
send `SOFTWARE` and others and a parser that gives up on the first unknown
attribute works against some servers and not others. A declared length longer
than the packet is refused. Arbitrary traffic on the port — and a UDP port
receives plenty — is ignored rather than parsed.

### Measured on this network

```
stun.l.google.com:19302      -> 192.140.152.117:35323   (21ms)
stun.cloudflare.com:3478     -> 192.140.152.117:35323   (17ms)

verdict  ENDPOINT-INDEPENDENT MAPPING (cone NAT)
```

The same external port from two different operators, so the mapping does not
depend on the destination and punching should work. This is the second
favourable sample of this network — the Phase 0 spike saw a different public
address, so the ISP has rotated it since, and the behaviour is unchanged.

**Two samples of one network is not a measurement of the kill criterion.** That
needs cellular, office and public wifi, and `netcheck` is the instrument:

```bash
cargo run --release -p qurb-peer --example netcheck
```

It supersedes the Phase 0 spike's `quictest stun`, which remains only as part of
that throwaway experiment.

## Signalling

The first server-side code in the project, and the piece hole punching cannot do
without: both routers must see an outbound packet at roughly the same time, and
nothing until now could arrange that.

### It is in Rust, against the architecture

Recorded as [decision 0015](../decisions/0015-control-plane-in-rust.md). The
architecture chose Go for the control plane, written before any of this existed
and never elevated to a decision record.

What changed is that signalling has to speak the same vocabulary as the client —
identifiers derived from the key hierarchy in `qurb-keys`. In Go those
derivations would be reimplemented and kept in step by hand, and a drift between
two implementations of a key derivation presents as "this device cannot find its
peers" with nothing obviously wrong at either end.

Go remains the better choice for accounts and billing, where its Stripe and
WebAuthn libraries are genuinely more mature. Services can be separate
processes; deciding the whole control plane must be one language would be the
actual mistake.

### What the server is allowed to learn

[Decision 0016](../decisions/0016-what-signalling-learns.md). A rendezvous
service necessarily learns that some addresses belong together. Run naively it
would also learn *whose* they are, building a map of every user, their devices
and their locations — a more valuable database than the files it was carefully
prevented from seeing.

So devices announce under identifiers derived from their master key: every
device of one user derives the same group, and a member identifier depends on
the fingerprint pairing already established, so a peer can compute where to look
while the server sees opaque bytes.

It still sees IP addresses. That is inherent, and it is why the data plane never
goes near it.

### Two choices that look arbitrary

**Websockets over TCP, not QUIC.** Reusing the data plane's transport would be
tidier and would fail exactly when needed: if UDP is blocked — precisely when a
device most needs coordinating — a UDP control channel cannot tell it that it
needs a relay.

**The connection is held open** rather than being a request and a response,
because a device that polls to learn it should punch will always be late.

### Two bugs the tests found

**A dropped client left its connection open.** The reader task parks on the
socket waiting for a message that may never come, so it could not notice on its
own that nobody was listening. One leaked connection per abandoned client, and
the server going on offering a stale address.

**Cleanup was skipped on abrupt disconnection.** The server's read loop used `?`
on the incoming message, so a client vanishing — laptop shut, network dropped —
propagated an error and returned before the code that removes it from the
directory. Ungraceful disconnection is the common case, not the exception, so it
had to leave by the same door as everything else.

Both were found by `a_device_that_leaves_stops_being_offered`, which asserts the
group count returns to zero. Neither would have shown up in a test that only
checked the happy path, and in production both would have presented as peers
wasting punch attempts on addresses of devices that were no longer there.

## Wiring it together

The pieces existed separately and nothing sequenced them. `Connector` is the
policy that does: bind a socket, ask STUN where it appears from, announce, ask
the rendezvous service for a peer, be told to punch, race every address the peer
offered.

### Both sides dial, and that is not an implementation detail

A router only lets a packet in if it has recently seen one go out to that
address. So both routers need to send something — and only the device making the
call would normally do so. The device sitting idle has sent nothing, so the
caller's packets arrive at a router that has never heard of them.

The answer is that **both sides dial**. A QUIC handshake begins with packets that
are themselves the hole punch, so when the rendezvous service tells two devices
to punch, each attempts a connection. Whichever handshake completes carries the
traffic; the other exists only to have sent something.

The responder's dial is deliberately doomed — it pins its own fingerprint, which
the peer does not have, so the handshake cannot succeed. That is fine and worth
saying out loud: the packets left the building, which was the entire job.

### A constraint that explains the shape of everything

**Once QUIC owns the socket, nothing else can send through it.** So the STUN
query happens before the endpoint is built, on the same socket, and everything
after that is done by handshakes.

That is why `Connector::start` does discovery before constructing the endpoint,
and why there is no explicit punch step in the normal path even though
`nat::punch` exists. Getting this backwards would produce code that looks right
and cannot work.

### Candidates are raced, not tried in turn

In parallel, because a candidate that is simply unreachable fails by timing out,
and trying three in sequence means waiting three timeouts to discover the last
one worked. Local addresses are listed first, so when several succeed the
cheapest path wins — two devices on one network should not route through the
internet to reach each other.

### The end-to-end test

`two_strangers_pair_find_each_other_and_sync` is the first test in which every
layer runs at once. Two devices that have never spoken: one creates a key, the
other enrols with its recovery phrase, they pair out of band, announce to the
rendezvous service, are introduced, race candidates, connect, and sync a file.

It does not prove traversal — there is no NAT on loopback. It proves the
*sequence* is right, which is the part that could be silently wrong.

The second test is the one that matters for the product: a device that is
switched off must be **reported, not waited on**. That is the case the relay and
the always-on replica exist for.

## The relay

The fallback for devices that cannot reach each other directly, and the
component that costs money per byte forever.

### It carries datagrams, not messages

The obvious design relays application messages — the relay understands "fetch
this chunk" and passes it along. It is also the wrong one, because a relay that
handles application messages is one that can read and alter them. Preventing that
means encrypting and authenticating at the application layer: a second
cryptographic protocol, solving a problem the first one already solved, in a
codebase that would then have two.

So the relay forwards datagrams and an ordinary QUIC session runs inside. Nothing
above the transport knows it is there. Recorded as
[decision 0017](../decisions/0017-relay.md).

The cost is an `AsyncUdpSocket` implementation making a relay connection look
like a socket — a contained piece of work paid once, against a second protocol
maintained forever.

`the_relay_never_sees_the_content` is the test that matters: a distinctive string
goes through a relay that records every byte it forwards, and the string never
appears. A 4 MiB transfer also survives intact, which exercises congestion
control and retransmission rather than just a handshake.

### TCP, when everything else is UDP

Because **the relay exists for networks where UDP does not work**, and belongs on
port 443 for the same reason. A fallback that requires the thing being fallen
back from is not a fallback.

### The risk being accepted

Registration is self-asserted. The identifier is derived from the user's master
key and therefore unguessable, so claiming someone else's means already knowing
it — which means being inside a group whose devices have paired.

Someone who does learn one can receive that device's relayed packets. They cannot
read them, but they can stop the real device getting them: a denial of service by
somebody who already had to be close enough to learn a derived secret. Closing it
properly means the relay verifying device keys, which means the relay knowing
device identities, which is what the design exists to avoid.

## Hardening the signalling server

Rate limits and resource caps: connections held at once, devices per group,
messages per second per connection, and a maximum message size enforced by the
websocket layer before a frame is assembled in memory.

The token bucket refills continuously rather than in steps, because stepwise
refill lets a caller send a full burst at the end of one window and another at
the start of the next — twice the configured rate.

Messages over the limit are **dropped rather than answered**. Replying would let
a caller spend the server's bandwidth by spending only its own.

The group cap has one exception that matters: re-announcing an existing member is
always allowed, because that is how a laptop that changed networks updates its
address, and a device locked out of its own group by a limit meant for strangers
would be a bad failure.

None of this survives a determined attacker with many addresses. That needs
infrastructure this service does not have, and saying so is more useful than
implying the limits are a defence they are not.

## Falling back automatically

`Connector::reach` now tries the direct path and, when every candidate fails,
goes the long way round. That join is the last piece of the connection story.

### A device has two ways in, and must listen on both

The change this forced was in the relay rather than in the policy. `RelaySocket`
was written for one peer: it knew who it was talking to and sent everything
there. That is enough to *dial* through a relay and useless for being *reached*
through one — and a device that cannot be reached over the relay makes the
fallback useless in exactly the case it exists for, because the peer whose direct
attempt failed will try the relay and find nobody home.

So the socket now maps relay identifiers to synthetic addresses in both
directions, allocating on demand. A peer that has never been seen gets an address
when its first packet arrives, which is how a device that was only listening
learns to answer.

The relay connection is held open rather than dialled when needed, for the same
reason: being reachable is not something that can be arranged after someone has
already failed to reach you.

### Trust does not loosen because the path got longer

The relayed connection uses the same pinned certificate as a direct one. The
relay carries the handshake without being party to it, so
`identity_is_pinned_just_as_hard_over_the_relay` asks the relay to forward to a
stranger's fingerprint and the connection is refused — by the client, not by the
relay, which would have forwarded it quite happily.

### Testing a fallback that never triggers

On one machine every direct attempt succeeds, so the automatic path would never
be exercised. `reach_via_relay` forces it. That also has a real use: a network
already classified as blocking UDP does not need to discover that again for every
connection.

## Running it

`demo` starts a rendezvous service and a relay, brings up two devices in two
real directories, pairs them, connects them, and keeps syncing:

```bash
cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
```

Until now nothing outside the tests exercised pairing, signalling or the relay,
so there was no way to see the system work without reading a test.

### A bug that only running it could find

The demo failed at its first attempt, on the second device to call out.

`reach` opened its **own** signalling connection and announced under the
device's identity. When it returned, that connection dropped — and the server
correctly read the disconnection as the device going away. So a device stopped
being reachable the moment it finished reaching somebody, and the failure
presented as *the other device* being absent, which is the wrong end entirely.

Every test passed throughout, because each one had a single device calling out
while the other only listened. It took two devices both initiating for the
problem to exist at all.

The fix is architectural rather than a patch: one signalling connection per
device, held for its lifetime and owned by one task that multiplexes. Requests
to connect are answered, each `Punch` is handed to whoever asked for it, and a
`Punch` nobody asked for means somebody is calling *us*, so we dial back purely
to punch.

## Still to do

- **Measuring the kill criterion.** Everything needed now exists, including a
  daemon for a second machine and `qurb netcheck` for the quick first pass. The
  method is written down in
  [measuring-connectivity.md](../measuring-connectivity.md); it needs two
  machines on two genuinely different networks and so cannot be done from here. `netcheck` classifies one network at a time; the real figure needs
  two devices on two genuinely different networks — cellular, office, café —
  trying to reach each other. This cannot be done from one machine, and until it
  is, the direct-connection rate is a guess. It is also the number the relay bill
  depends on, so it is worth doing before there is a bill.
- **Relay selection**, fairness and quotas. One device can currently use all the
  capacity of a relay, there is no notion of choosing a nearby one, and a device
  connects to whichever relay it was configured with.
- **Noticing that a direct path has become possible.** A connection that fell
  back to the relay stays there, even after the device moves to a network where
  punching would work. Tailscale re-probes; this does not.
- **TLS on the servers themselves.** Both the signalling service and the relay
  expect termination by a reverse proxy. That is a normal deployment, but it is
  a requirement rather than an option, and neither refuses to start without it.
- **DERP relay.** One line in the architecture document and a substantial
  service in practice — the component that costs money per byte forever.

## The availability gap, closed

[Decision 0006](../decisions/0006-availability-gap.md) had been open since Phase
1 and is now decided: **the engine supports storage-only replicas**, and where
one runs — the user's own machine, or ours for a fee — is a deployment question
rather than an architectural one.

The reasoning that settled it was that the four options it proposed were not
alternatives. A user-provided node, a paid pin and peer-assisted replication all
need the same engine capability; only accepting the limitation avoids it. So the
question was never which option, but whether to have any, and once the capability
exists the rest is choosing where to run a container.

### The behaviour that had to be switched off

A syncing device decides a file is gone by walking its tree and not finding it. A
replica has nothing on disk by design, so **the same inference would tombstone
the entire library and propagate those deletions to every device that trusted
it.**

That is the most dangerous thing in the change and it looks like a no-op:
`reconcile` on a replica returns immediately. `reconciling_a_replica_does_not_delete_everything`
is there because a future refactor that "tidies up" that early return would
destroy data everywhere, quietly, and only on the deployment nobody tests.

Materialising files is the other thing switched off — twice the space for a copy
nobody reads.

Partial replication is built in rather than deferred: "hold everything" is the
expensive answer, and retrofitting a selector would have meant revisiting every
path that assumes a replica mirrors its peers.

### Timing

Doing this before signalling was the point. Signalling will otherwise bake in the
assumption that every peer has a filesystem, and unpicking that later is far
more expensive than a role enum now.
