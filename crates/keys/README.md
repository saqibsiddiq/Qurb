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

## Three ways to keep the key

They defend against different things, and the difference is worth stating
because a user reading "end-to-end encrypted" will assume the strongest.

| | defends against | starts unattended |
|---|---|---|
| `file` | other users of the machine | yes |
| `keystore` | anyone reading the disk while it is locked | yes |
| `passphrase` | anyone who takes the disk *and* the session | no |

**File** is what was there before, and remains the default: a headless machine
may have neither a keystore nor anybody to type a passphrase, and a device that
cannot unlock itself is worse than one whose key sits in a file.

**Keystore** is the operating system's own — Keychain, the Windows Credential
Manager, the Secret Service. `keystore_available()` probes whether this machine
actually has a usable one, because a headless server has one in name only.

**Passphrase** wraps the key with Argon2id at 64 MiB and three passes, then
XChaCha20-Poly1305 with the header authenticated so a salt cannot be swapped in
from another file. It is the only option that survives a stolen disk, and the
only one that stops a device starting on its own.

The recovery phrase is unaffected by any of this. Two different secrets protect
the same key and neither interferes with the other: the phrase recovers the key,
the passphrase guards the copy on this disk.

**None of them help while the daemon is running and holding the key in memory.**
That is what it means to be a program that can decrypt your files.

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

- **Verified support on macOS and Windows.** The keystore path is written
  against a cross-platform library and tested here against the Secret Service on
  Linux. Keychain and the Credential Manager are exercised by nothing, and a
  claim about them would be a guess.
- **Key rotation.** Changing the master key means re-encrypting every chunk, and
  there is no mechanism for it.
- **Per-file keys.** The architecture describes deriving a key per file so that
  one leaked key exposes one file. Currently every chunk uses the same derived
  key, which is simpler and weaker.
- **Escape hatches for lost phrases** — social recovery, an escrowed key, a
  printed kit. Every consumer product in this space eventually adds one. Which
  compromise to make is still an open product decision.
