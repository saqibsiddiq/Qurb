# List sizes

Throwaway. How many bytes a phone receives for a computer's file list, sent
whole and as changes only
([decision 0072](../../docs/decisions/0072-only-what-changed-in-a-list.md)),
measured on a copy of a real index. It also checks that the list rebuilt from
the changes has the whole list's fingerprint.

For each device the index has paired, `src/main.rs`:

- encodes its list as `TreeStream` would send it, and counts the bytes;
- counts entries that share a key, meaning a place and a path; a rebuild
  cannot tell two such entries apart, so they would make every rebuild fail;
- makes a change to the copy with plain SQL, so the index's triggers move its
  counter as they would for a real change. Then it answers as the server
  answers `TreeSince`, timed, encodes the `TreeDelta`, rebuilds the list from
  the one held before, and compares fingerprints.

```bash
cargo build --release -p list-sizes
sqlite3 ~/qurb/Shared/.qurb/index.db ".backup $SCRATCH/index.db"   # a copy; the copy is changed
./target/release/list-sizes $SCRATCH/index.db
```

## Conditions

2026-10-20. The owner's laptop (Intel i5-13420H, 12 threads, CachyOS, Linux
7.2). Its index at schema 27, copied while the daemon ran, onto tmpfs, so the
cache was warm. Release build. Three runs, each on a fresh copy. During the
second and third, the phone app was being built on the same machine.

## Results

The S23 as it is paired now: 38,006 entries, **7,941,674 bytes sent whole**.
No two entries share a key. A second device is paired under the same name,
SM-S911B; it lists 6,681 entries and gave the same results.

| change since the list the phone holds | entries sent | bytes sent | server's answer took | rebuilt list |
|---|---:|---:|---:|---|
| nothing | 0 | 45 | 64–97 ms | matches |
| one shared file edited | 1 | 194 | 57–84 ms | matches |
| one shared file deleted | 1 | 166 | 51–89 ms | matches |
| ten of the phone's own files recorded again | 10 | 2,215 | 51–83 ms | matches |
| a thousand of them | 1,000 | 208,125 | 62–83 ms | matches |

The server's time is mostly reading the whole list and fingerprinting it. A
`TreeDigest` answer (decision 0071) did the same work.

## Corners cut

- Run on the laptop alone. Nothing went over a network, and no phone rebuilt
  the list. The bytes counted are the message itself, without QUIC's framing
  or encryption.
- The changes are made with SQL, not by the engine. They move the same
  columns a real change moves, but they are not real changes.
- One index, one afternoon.
