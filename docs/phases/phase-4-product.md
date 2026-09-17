# Phase 4 — Desktop product

**Status:** in progress
**Target:** months 8–10

Turning a working engine into something a person can run. The phase the roadmap
warns is not the fun part and is a full quarter.

## Progress

| area | status |
|---|---|
| a daemon that runs | ✅ [`qurb`](../../crates/qurb/) |
| the commands around it | ✅ init, enrol, pair, join, run, status, verify, config |
| running the services | ✅ `qurb signal`, `qurb relay` |
| push, rather than polling | ✅ ~430ms, measured |
| onboarding and the recovery phrase | ◐ works, in a terminal |
| installers | ⬜ not started |
| signed updates with rollback | ⬜ not started |
| observability | ◐ structured logs, nothing more |
| the interface | ⬜ not started |

389 tests pass across nine crates; clippy is clean.

## The daemon

Until now nothing outside tests and examples could be run, and the demo put two
devices in one process — which meant the one thing that mattered for the Phase 3
kill criterion, *two machines on two networks*, was impossible.

`qurb run` watches a directory, keeps the store in step with it, serves peers on
every path it has, and pulls from each paired device it can reach.

## Three bugs that only running it could find

Every one of these passed the whole test suite.

### An invite offered an address nobody could dial

`qurb pair` bound `0.0.0.0` and put that in the invite. It is a true statement of
where the socket is listening — every interface — and completely useless to a
peer, which cannot connect to it.

Every test bound `127.0.0.1` explicitly and so never saw it. The failure appeared
at the *joining* device, which reported that it could not connect, making it look
like the joiner's problem.

The fix asks the routing table rather than listing interfaces: a UDP socket
*connected* to an arbitrary address sends nothing but makes the operating system
choose a source address, and the one it chooses is the one that would really be
used. Listing interfaces instead means guessing between a wired connection, a
wireless one, a virtual machine bridge and three container networks.

### Two devices started together took two minutes to find each other

The first sync attempt happens before the other device has announced, which fails
— and the retry backoff went straight to two minutes. Starting both daemons at
once, as anyone would, produced a system that appeared to do nothing at all.

The two failures are indistinguishable at the moment they happen and want
opposite treatment: a device still starting up should be retried in seconds, one
that is switched off should be left alone. So the backoff now doubles from five
seconds to a two-minute cap. Simultaneous startup went from 120 seconds to **9**.

### An edit took thirty seconds to cross

A device syncs when *it* changes something, or when its timer fires. An edit made
on the laptop therefore reached the desktop only when the desktop next asked.

**Now fixed.** A device holds one request open against each peer — "tell me when
your state differs from this" — and the answer arrives when it does.

Measured, two devices on this machine, the same edit three times: **433ms, 432ms,
431ms**, against up to ten seconds before. Most of what remains is the watcher
deliberately waiting to see whether the file is still being written, which is a
floor worth having rather than latency to remove. Thirty seconds of complete
idleness produced no log activity at all, so the responsiveness is not bought
with chatter.

It does not break the rule that **a peer can ask and never tell**: the device
that wants to know is the one asking, and the answer simply arrives later than
usual. A peer still cannot make anything happen.

A counter rather than a flag, because a flag can be missed — a peer told
"something changed" cannot distinguish a notification it has already acted on
from a new one. It says what it last saw, and gets an immediate answer if
anything has happened since. The counter deliberately does not survive a restart:
a peer holding a number from before sees one that does not match, concludes the
device it was watching has been away, and looks. Which is right.

## What running it confirmed

Two devices, two directories, real sockets: paired over the LAN, synced in both
directions, propagated a deletion, and resolved a genuine conflict — both
versions kept, the same conflict filename computed independently on each device,
and both agreeing on the resulting file list afterwards. `qurb verify --deep`
reports everything agrees.

## Still to do

- **Noticing a device paired while running.** The guest list is read at startup.
- **Running as a service** — a systemd unit, a launch agent, a Windows service.
- **Installers**, and the update mechanism with rollback.
- **The interface.** Everything so far is a terminal, and the onboarding screen
  that asks someone to write down 24 words is the highest-stakes part of the
  product.
