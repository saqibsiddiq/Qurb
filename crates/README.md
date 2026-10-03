# crates/

Production code.

Code here is meant to last and is held to a real standard: tested, documented,
and honest about its limitations. This is the distinction from
[`../experiments/`](../experiments/), where throwaway code may cut corners as
long as it says which.

Nothing migrates silently from `experiments/` to here. The Phase 0 spike proved
the ideas; Phase 1 code was written deliberately, informed by the spike rather
than copied from it.

## What is here

Each crate has a README saying what it guarantees and what it does not do yet.
[../docs/CODEBASE.md](../docs/CODEBASE.md) §4 maps their files, and §7 gives an
order to read them in.

| crate | package | what it is |
|---|---|---|
| [storage/](storage/) | `qurb-storage` | chunking, compression and encryption at rest, the content-addressable store, the SQLite index |
| [watcher/](watcher/) | `qurb-watcher` | filesystem events turned into settled changes |
| [sync/](sync/) | `qurb-sync` | version vectors, conflicts, reconciling two trees — pure logic, no disk or network |
| [engine/](engine/) | `qurb-engine` | what a change means, and doing it: the run loop, a replica, repair, syncing with a peer |
| [keys/](keys/) | `qurb-keys` | the master key, the 24 words, and how the key is kept on disk |
| [peer/](peer/) | `qurb-peer` | reaching another device: QUIC, pinned identity, pairing, NAT traversal, local discovery |
| [signal/](signal/) | `qurb-signal` | the rendezvous service, and waking a sleeping phone |
| [relay/](relay/) | `qurb-relay` | forwarding encrypted traffic when no direct path exists |
| [qurb/](qurb/) | `qurb-cli` | the program a person runs — the `qurb` command and the daemon, as a library too |
| [desktop/](desktop/) | `qurb-desktop` | the desktop window, hosting that daemon |
| [tray/](tray/) | `qurb-tray` | the same daemon behind a tray icon, for desktops that have one |
| [mobile-ffi/](mobile-ffi/) | `qurb-mobile` | the engine as a phone calls it, through UniFFI |

The Android app that calls `qurb-mobile` is outside this directory, in
[../android/](../android/).

## How it differs from the plan

Before Phase 1 this file sketched four crates — `storage`, `crypto`,
`protocol`, `engine` — and said the real boundaries would show themselves while
building. They did, differently. Encryption at rest went into `storage`, where
the on-disk format lives; transport encryption is QUIC's TLS rather than a
Noise handshake, so it lives in `peer`; and the wire format belongs to `peer`
too, since nothing else speaks it. What appeared instead were seams the sketch
did not have: `sync` apart from `engine`, holding no data and touching no disk
so that conflict logic can be tested exhaustively; `keys`, the root secret and
the 24 words on their own; and, once there was a product to build, the
services, the program and the three front ends.
