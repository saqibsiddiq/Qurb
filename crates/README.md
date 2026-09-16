# crates/

Production code. Empty until Phase 1.

Code here is meant to last and is held to a real standard: tested, documented,
and honest about its limitations. This is the distinction from
[`../experiments/`](../experiments/), where throwaway code may cut corners as
long as it says which.

Nothing migrates silently from `experiments/` to here. The Phase 0 spike proved
the ideas; Phase 1 code gets written deliberately, informed by the spike rather
than copied from it.

Expected shape once Phase 1 begins, per
[../docs/architecture.md](../docs/architecture.md):

```
crates/
├── storage/     chunking, content-addressable store, SQLite index
├── crypto/      encryption at rest, Noise transport handshake
├── protocol/    wire formats shared between peers
└── engine/      sync state machine tying the above together
```

This is a sketch, not a commitment. The boundaries that matter will become clear
while building, and inventing them in advance tends to produce crates that exist
to satisfy a diagram.
