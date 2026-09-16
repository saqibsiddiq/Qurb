# 0013 — Case collisions are refused, not resolved

**Status:** Accepted
**Date:** 2026-09-16

## Decision

When a path differs only in case from one already present, the write is
**refused** and recorded as a per-file failure. It is not renamed, not merged,
and not silently allowed to overwrite.

Whether to apply the rule is probed from the filesystem by default and can be
overridden, because the hazard belongs to the set of devices rather than to the
local disk.

## The problem

`README` and `readme` are two files on Linux and one on macOS or Windows.

Writing the second destroys the first, which is a nuisance. The data loss comes
one step later: the next reconciliation sees one file where the index expected
two, reports the missing one as **deleted**, and propagates that deletion to
every other device. A single confusing merge on one machine becomes a file gone
everywhere, and the tombstone makes it look deliberate.

## Why refuse rather than resolve

**Renaming** is what conflict resolution does for concurrent edits, and it is
wrong here. A conflict is two versions of one file; this is two *different
files* that a filesystem cannot keep apart. Renaming one would invent a path the
user never chose, on every device including the ones that were handling both
files perfectly well.

**Merging** would discard one file's contents outright.

**Allowing the overwrite** is the current behaviour of most sync tools and is
the failure described above.

Refusing keeps both files intact on the devices that can hold them, keeps the
user's names, and surfaces the problem as something to decide rather than
something that already happened. The only thing lost is convenience: the user
must rename one of them, which is the decision only they can make.

## Probed, not assumed

macOS is case-insensitive by default but can be formatted otherwise; Linux is
usually but not always sensitive; a network mount can be anything regardless of
its host. So the probe creates a file and looks for it under a different case.

If the probe cannot run it reports "case-sensitive", which is the safe
direction: treating an insensitive filesystem as sensitive risks losing a file,
while the reverse merely declines to merge two paths that were always distinct.

## Configurable, because the hazard is the fleet's

A Linux desktop holding both `README` and `readme` is fine — until a phone
joins, at which point one of them is destroyed and the deletion comes back.

So `set_fold_case(true)` lets a case-sensitive machine behave as a
case-insensitive one. Anyone syncing with iOS or Windows should set it, and the
default of probing the local disk is right only for a fleet that is entirely
case-sensitive.

Choosing this automatically would need the engine to know what devices exist,
which is pairing, which is Phase 3.

## Consequences

Collisions are reported even where they are currently harmless, because that is
the only chance to warn before the device that cannot cope arrives.

A refusal is a per-file failure and does not stop the rest of a sync, for the
same reason an unreadable file does not abort a reconciliation.

## Limitation

Folding is ASCII-only, via SQLite's `lower()`. Turkish dotted I and other
script-specific rules are missed. Unicode case folding is entangled with
normalisation — macOS stores NFD, Linux usually NFC, and two paths can differ in
bytes while being the same name — and that is a larger piece of work which this
decision does not attempt.

## Reversibility

**Low cost.** The check is one query and one guard, and nothing has been stored
differently because of it.
