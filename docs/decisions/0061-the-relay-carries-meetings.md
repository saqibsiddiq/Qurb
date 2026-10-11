# 0061 — The relay carries meetings: one connection, several names

**Status:** Accepted, 2026-10-11. Amends [0017](0017-relay.md) (the relay's
wire format) and closes a gap [0060](0060-a-computer-keeps-private-folders-for-several-people.md)
left open in its step 2.
**Date:** 2026-10-11

## What was found

Running decision 0060 on the Android emulator, with the emulator as a guest
of a computer on the laptop (0060, step 6): the guest visited, chose the
computer to keep its Private Vault, and the computer never kept anything.

A device fetches by dialling the other. The computer has to dial the guest
to read what the guest shows it, and it could not: the emulator sits behind
its own address translation, and every address it announced timed out
(`10.0.2.15`, `10.0.2.16`, its public address through the router). That is
the emulator's network, but it is also a phone on mobile data behind a
carrier's NAT, which is the ordinary case for a guest's phone away from the
computer's Wi-Fi.

The relay exists for exactly this. It could not help: a device registers on
the relay under the identifier its own person's key gives it, and a guest's
meeting with a computer gives each of them other names (`MemberId::for_meeting`)
that neither can derive from the other's key. 0060 recorded this as a gap:
"the relay stays keyed to one person".

## Decision

**A relay connection may hold several names**, up to 64: a device's own,
then one for each meeting with another person's device. The wire format
grows two frames, and keeps the three it had:

| tag | frame | |
|---|---|---|
| 1 | `Register { member }` | now *adds* a name to the connection, rather than replacing the one it had |
| 2 | `Forward { to, payload }` | from the connection's first name, as before |
| 3 | `Deliver { from, payload }` | sent to a connection with one name, as before |
| 4 | `ForwardAs { from, to, payload }` | from one of the connection's names; refused for a name it did not register |
| 5 | `DeliverTo { from, to, payload }` | sent to a connection with more than one name, saying which it was for |

Each side then answers under the name it was called by. On the client, the
synthetic address QUIC sees stands for a pair (the peer, and which of this
device's names the two use), so a session started under a meeting's name
stays under it in both directions. A connection that never registers a
second name sees and sends only the original three frames.

In the connection policy (`qurb_peer::Connector`):

- `meet()` registers the meeting's name for this device on the relay;
- reaching a meeting's device falls back to the relay when no direct
  address answers, as reaching one's own devices always has;
- going through the relay uses the meeting's names for both ends.

## Why this, and not the others

- **A relay connection per meeting** needs no change to the relay: open a
  second connection under the meeting's name, with a QUIC endpoint of its
  own. But every endpoint needs something accepting on it, and those
  listeners are set up once, at start, in both the daemon and the phone's
  sync pass. Meetings begin later. Adding listeners as meetings start, in
  two places, is more machinery than two frame kinds, and a connection
  each is more for the relay to hold.
- **Serving over the connection the other side opened** (the computer
  reading the guest over the QUIC session the guest dialled) would make
  the relay unnecessary in this case, and would help a phone behind a NAT
  even with its own devices. It changes how both ends serve and fetch, and
  is a larger decision than this one. Not taken here; worth taking later.

## What the relay learns that it did not

One connection now holds a device's own name and its meeting names, so the
relay can tell that those names belong to one device. The names are still
blinded: it cannot tell whose they are, or what the device is. It could
already link a connection to its IP address. The relay is a server its
person runs (0017, *A server of your own*), so this is a statement about
that server, and it is made here rather than left to be discovered.

## Limits, stated plainly

- **Both ends must use the same relay**, as they must already use the same
  rendezvous service. Nothing yet tells a guest which relay and rendezvous
  service the computer uses: a guest's phone set to its own person's
  services does not meet a computer set to another's. Recorded as a gap in
  0060.
- **Only while the phone is in a sync pass.** A phone registers on the relay
  for the length of a pass, so the computer reaches it then, as it does
  directly.
- **A relay is updated before the devices that use it.** A relay built
  before this replaces a connection's name when given another, so a device
  updated first would lose its own name there the moment it met a guest. No
  relay runs anywhere yet (the owner's server is next); when one does, it
  goes first.
- **Push and local beacons stay keyed to one person.** A guest's phone is
  not woken when the computer has news for it, and is not found by beacon
  on the computer's Wi-Fi; the rendezvous service still carries its local
  addresses.

## Measured

On 2026-10-11, on the laptop (CachyOS, x86_64): a scratch rendezvous service
and relay on spare ports; a scratch computer, the real desktop application
under GTK's Broadway backend with a home of its own; the Android emulator
(`qurb-test`, Android 14, x86_64) as the guest, behind the emulator's
address translation; the debug APK built from this commit.

- Without this change, the computer could not reach the guest: every
  candidate address timed out, and nothing was kept.
- With it: "no direct path to a meeting's device; falling back to the
  relay", then "connected via the relay". The computer kept the guest's
  vault sealed: 235 bytes for a 63-byte file.
- Everything after that ran: the computer asked, the phone asked its person,
  the PIN was given, the key was sent, and the computer listed the file by
  its real name; opening it there unsealed 63 bytes into a directory only
  its owner can enter; locking closed it; and the file, freed on the phone
  once kept, came back to the phone when asked for.

Tests: `a_session_runs_to_a_second_name_and_answers_under_it`,
`nobody_forwards_as_a_name_they_did_not_register`,
`a_connections_names_go_with_it_and_are_bounded` (`crates/relay/tests`);
`a_guest_and_its_computer_reach_each_other_through_the_relay`
(`crates/peer/tests/guests.rs`), which fails with the meeting's
registration taken out.

Not measured: throughput through the relay for a guest; real phones; two
networks.
