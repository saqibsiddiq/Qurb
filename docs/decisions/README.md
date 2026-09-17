# Decision records

One file per significant technical decision. Numbered sequentially, never
deleted.

A decision belongs here if reversing it later would be expensive — a language, a
protocol, a storage format, a rule that data correctness depends on. Routine
implementation choices do not need a record.

## Format

Each record states the decision, the reasoning, what was rejected and why, the
tradeoff accepted, and how hard it would be to reverse. That last field is the
most useful one in practice: it tells you which decisions you are allowed to be
wrong about.

## Status values

- **Accepted** — in force.
- **Superseded by NNNN** — replaced. The file stays, with a pointer.
- **Proposed** — under consideration, not yet acted on.

Superseded records are never removed. The record of what was believed, and why,
is worth more than a tidy directory.

## Index

| # | Decision | Status |
|---|----------|--------|
| [0001](0001-hybrid-p2p-topology.md) | Hybrid P2P with central coordination | Accepted |
| [0002](0002-rust-core-with-native-shells.md) | Rust core, native UI shells | Accepted |
| [0003](0003-sqlite-plus-cas.md) | SQLite index alongside a CAS on disk | Accepted |
| [0004](0004-chunk-parameters.md) | 128 KiB / 512 KiB / 2 MiB chunk bounds | Accepted |
| [0005](0005-conflict-resolution.md) | Conflict resolution never discards edits | Accepted, extended by 0009 |
| [0006](0006-availability-gap.md) | Storage-only replicas, for offline availability | Accepted |
| [0007](0007-chunk-format.md) | On-disk chunk format, fixed before key management | Accepted |
| [0008](0008-watcher-delivery-guarantee.md) | Watcher delivers changes at least once | Accepted |
| [0009](0009-conflict-edge-cases.md) | Conflict cases 0005 did not cover | Accepted |
| [0010](0010-content-by-hash.md) | Content is requested by hash, never by path | Accepted |
| [0011](0011-peer-identity-pinning.md) | Peer identity is a pinned certificate fingerprint | Accepted, completed by 0014 |
| [0012](0012-key-hierarchy-and-recovery.md) | One root secret, derived keys, 24-word phrase | Accepted — key storage weak |
| [0013](0013-case-collisions.md) | Case collisions are refused, not resolved | Accepted |
| [0014](0014-pairing.md) | Pairing transfers a full fingerprint out of band | Accepted |
| [0015](0015-control-plane-in-rust.md) | Signalling and the relay are written in Rust | Accepted — billing open |
| [0016](0016-what-signalling-learns.md) | What the signalling server is allowed to learn | Accepted |
| [0017](0017-relay.md) | The relay carries datagrams, not messages | Accepted |
