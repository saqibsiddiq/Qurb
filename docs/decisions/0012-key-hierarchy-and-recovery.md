# 0012 — One root secret, derived keys, and a 24-word phrase

**Status:** Accepted — with key storage explicitly weak
**Date:** 2026-09-16

## Decision

A single 256-bit master key is generated per user from the operating system's
CSPRNG. Every other key is derived from it with HKDF-SHA256 under a versioned,
purpose-specific label. The master key is encoded as a 24-word BIP-39 phrase,
which is the only way to recover it.

The master key is stored on disk in a file readable only by its owner.

## Reasoning

**Derivation rather than separate keys** means one thing to back up and one
thing to lose. HKDF is one-way, so a derived key that leaks reveals nothing
about the master or about keys derived for other purposes. The labels are
versioned so a future change to what a key protects can be a new label rather
than a silent change of meaning.

**Purposes are a closed enum**, not a caller-supplied string. Two purposes
accidentally sharing a label would produce the same key for both — undetectable
by any test that does not specifically look for it, and exactly the sort of
thing a typo causes.

**BIP-39 rather than a hand-rolled encoding.** The encoding is trivial; the
wordlist is the hard part. It is chosen so four letters identify any word, and
confusable words are excluded. A phrase people mis-transcribe means lost data
here, because there is no second chance.

**24 words, not 12.** 12 words is 128 bits, which is ample against brute force.
24 carries the full 256 bits of the key, so the phrase is the key rather than a
seed stretched into one — a simpler thing to reason about and to document.

**The checksum is load-bearing.** The realistic failure is a person transposing
two words while copying from paper. Without a checksum that produces a valid but
different key, and an account that appears empty. With one, it is rejected while
the correct phrase is presumably still to hand.

## Consequences

**The phrase can be shown exactly once.** `Vault::open_or_create` returns
`Opened::Created { key, phrase }` on first use and `Opened::Existing(key)`
after. There is deliberately no method that returns the phrase later, because
offering one would imply it could be recovered — it cannot, without the key it
came from.

**Restore will not overwrite.** Installing a different key over an existing one
would orphan every chunk already stored: still on disk, encrypted under a key
that exists nowhere. Replacing a key must be a deliberate act, so the file has
to be removed by hand.

**Onboarding is the highest-stakes screen in the product.** A non-technical
person must be persuaded to write down 24 words *before* they have any
investment, because afterwards nobody can help them. That is a product problem
this decision creates and does not solve.

## The weakness we are accepting for now

A file with owner-only permissions is not key protection. It defends against
other users on the machine and against a backup that excludes it. It does not
defend against anything that can read the disk.

The real answer is the platform keystore — Keychain, DPAPI, Secret Service —
which is three separate integrations and is not built. This is recorded as a
known gap rather than presented as a design, because the difference matters: a
user reading "end-to-end encrypted" would reasonably assume more than this
provides on a compromised machine.

## Still open

**What happens when someone loses their phrase.** Every consumer product in this
space eventually adds an escape hatch — social recovery, an escrowed key, a
printed recovery kit. Each trades away some of the zero-knowledge property.
Choosing which compromise to make is better done on paper, now, than under
pressure from an upset user. Nothing here decides it.

## Reversibility

**The derivation is cheap to extend** — a new purpose is a new label — and
**expensive to change**, since altering an existing label changes the key and
orphans everything encrypted under the old one. The phrase format is fixed by
BIP-39 and should be treated as permanent.
