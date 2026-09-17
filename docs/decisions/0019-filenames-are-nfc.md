# 0019 — Filenames are normalised to NFC

**Status:** Accepted
**Date:** 2026-09-17

## Decision

Every path stored in the index or sent to a peer is in **Unicode Normalization
Form C**, the composed form. Normalisation happens at one place —
`qurb_watcher::logical_path`, where a path on disk becomes a logical path — and
nowhere else.

Where two files in one directory normalise to the same logical path, one is
indexed and the rest are reported to the user. Nothing is renamed automatically.

## The problem

`é` can be written two ways: one code point (U+00E9), or `e` followed by a
combining acute accent (U+0301). They render identically, mean the same thing,
and are different strings — therefore different filenames.

Filesystems disagree about which to keep:

| | what it stores |
|---|---|
| ext4, NTFS, most network mounts | exactly the bytes given |
| HFS+, APFS (macOS, iOS) | decomposed, always |

So a file created as `café` on a Mac is read back as `cafe` + U+0301.

Without normalisation, syncing that file is an unbounded duplication loop:

1. Linux has `café` composed, and sends it.
2. The Mac writes the composed name. Its filesystem stores the decomposed one.
3. The Mac's next scan finds a path it has no record of. It concludes that the
   composed file was deleted and a decomposed one created, and sends both.
4. Linux, which keeps the two apart, now has two files.
5. Go to 1.

This is not a rare edge case. It is every accented filename, every user with a
Mac and a Linux machine, every time.

## Why NFC rather than NFD

Either would work as a canonical form; what matters is that there is one.

NFC because it is what the web, most editors, and most sources of filenames
already produce. On Linux and Windows the normalisation pass therefore finds
nothing to change, which makes the common case free and the change invisible.

Choosing NFD would mean rewriting the majority of the world's filenames to match
the minority of filesystems that decompose.

## Why not resolve collisions automatically

On a filesystem that keeps the spellings apart, a user can genuinely have two
files whose names normalise to the same path. The index is keyed by that path
and can hold one.

Renaming one of them would be a program silently altering a user's filenames to
resolve a problem the user cannot see and did not cause. Refusing to sync the
directory would be worse. So: index the first, report the rest, and let the
person decide.

## The tie-break, and why it is not obvious

The first version kept "the first by sort order" and was wrong. The entries were
sorted by *logical* path — and the colliding entries have the same logical path
by definition, so the tie fell through to directory-read order, which differs
between machines. Two devices resolving the same collision could keep different
files under one name, which is worse than either outcome alone.

The rule must depend only on the filenames, because that is all two devices
reliably share. It is now:

1. The spelling already in normalised form wins. It is what every other device
   will produce for this path, and what a non-decomposing filesystem stores
   unchanged.
2. Byte order breaks anything left. Arbitrary, but identical everywhere.

## Consequences

- `qurb_watcher::normalize` is the canonical form; anything constructing a
  logical path outside `logical_path` must call it. The FFI's `import_file` and
  `remove` do.
- A macOS or iOS device will write the composed name and read back the
  decomposed one on every scan. That is correct and costs one normalisation per
  path per scan. `is_nfc_quick` answers from an ASCII prefix without
  allocating, so the overwhelmingly common case is nearly free.
- `SyncStats::collided` counts skipped files, and both `qurb status` and the
  daemon report them. A silent skip would be indistinguishable from data loss.
- Case collisions remain a separate problem with separate handling. The two are
  siblings — both are "this filesystem cannot tell these names apart" — but they
  have different causes and different advice for the user.
- Existing indexes predate this. A store built before it that contains
  decomposed paths will re-index those files under composed names at the next
  scan, which reads as a delete and an add. No migration was written because no
  store outside this repository exists yet; one would be needed before release.
