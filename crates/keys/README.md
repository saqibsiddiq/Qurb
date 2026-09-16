# qurb-keys

The root secret, the keys derived from it, and the 24 words that are the only
way back to it.

```
MasterKey ──HKDF──► chunk encryption
     │              device identity
     │              metadata authentication
     │
     └──BIP-39──►   24 words on paper
```

## What makes this different from ordinary key handling

The servers hold nothing. There is no reset, no support ticket, no escrow. If a
user loses both the key and the phrase, the data is gone — not by policy, but as
a fact about the mathematics, because nothing else can decrypt their chunks.

Every choice here follows from that:

- **The phrase carries a checksum**, so a mistyped or transposed word is caught
  at the door. Without one, a wrong phrase would be accepted, installed, and
  found to open nothing only much later — by which time the correct phrase may
  be gone.
- **`Vault::restore` refuses to overwrite** an existing key. Overwriting would
  orphan every chunk already stored: still on disk, encrypted under a key that
  no longer exists anywhere.
- **`Opened::Created` hands over the phrase exactly once**, at the only moment
  it can be produced. There is deliberately no `phrase()` method, because one
  would imply it could be asked for later.
- **Keys are redacted in `Debug` and wiped on drop.** A key that reaches a log
  file has leaked, and the usual way that happens is a struct being printed
  during debugging.
- **Purposes are an enum, not a string.** Two purposes sharing an info string
  would silently produce the same key for both; a typo should not be able to
  cause that.

## BIP-39, not hand-rolled

The encoding is simple; the wordlist is not. It is chosen so four letters
identify any word, similar words are avoided, and it sorts usefully. Getting
that wrong produces a phrase people mis-transcribe, which for a zero-knowledge
product means losing their data. The `bip39` crate supplies the canonical list
and checksum, and a test pins a known phrase against a known key so a change of
library cannot silently change what a phrase means.

## The gap, stated plainly

The master key is written to a file readable only by its owner. That protects it
from other users on the machine and from a backup that excludes it. It does
**not** protect it from anyone who can read the disk — malware running as the
user, a stolen unencrypted drive, a filesystem backup that includes it.

The real answer is the operating system's keystore: Keychain on macOS, DPAPI or
the Credential Manager on Windows, the Secret Service on Linux. Each is a
separate platform integration and none is built. The threat model should say so
rather than implying more.

## Testing

```bash
cargo test -p qurb-keys
```

The unit tests prove a phrase rebuilds a key. `tests/recovery.rs` proves
something different and more important: that the words on a piece of paper turn
back into the user's *files*, through derivation, the chunk cipher, and the
on-disk format. It is the highest-stakes property in the product, and the one
where being wrong is discovered far too late.

## Trying it

```bash
cargo run -p qurb-keys --example enrol -- /tmp/device-a
```

## Not yet built

- **OS keystore integration**, as above. The largest gap.
- **Passphrase protection** of the key file, as an interim measure for users who
  want it before keystore support exists.
- **Key rotation.** Changing the master key means re-encrypting every chunk, and
  there is no mechanism for it.
- **Per-file keys.** The architecture describes deriving a key per file so that
  one leaked key exposes one file. Currently every chunk uses the same derived
  key, which is simpler and weaker.
- **Escape hatches for lost phrases** — social recovery, an escrowed key, a
  printed kit. Every consumer product in this space eventually adds one. Which
  compromise to make is still an open product decision.
