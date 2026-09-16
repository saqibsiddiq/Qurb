# 0005 — Conflict resolution never discards an edit

**Status:** Accepted
**Date:** 2026-09-07
**Extended by:** [0009](0009-conflict-edge-cases.md), which covers cases this
record leaves unspecified: delete against edit, concurrent identical content,
and the vector a resolution must carry.
**Supersedes:** the "highest vector generation index wins" rule in the original
architecture document.

## Decision

Concurrent modifications are detected with vector clocks. When two changes are
genuinely concurrent, **both versions are kept.** The automatic rule decides only
which version keeps the original filename; the other is written alongside it,
renamed with the originating device and a timestamp.

Ordering, in full:

1. If one change causally precedes the other — its vector clock is
   component-wise less than or equal to the other's — the later change wins
   outright. This is not a conflict, just a normal update.
2. Otherwise the changes are concurrent. Both are retained. The version whose
   content hash sorts higher lexicographically keeps the original path; the
   other is renamed to `name.conflict-<device>-<timestamp>.ext`.
3. The user is notified that a conflict occurred.

## Reasoning

The original architecture document specified that "the mutation containing the
highest vector generation index takes precedence." This rule is not sound.

Concurrent vector clocks are by definition unordered — neither dominates. Any
tiebreak that compares magnitudes, such as taking the maximum component or the
sum, produces a winner that depends on **how much unrelated activity happened on
other devices.** A laptop that has been busy syncing other files would win
conflicts against a phone that has been idle, for no reason connected to the
edit in question. Worse, the rule is not stable: the same pair of edits can
resolve differently depending on what else was going on.

The hash comparison in step 2 is deterministic, is independent of unrelated
activity, and gives every device the same answer without coordination. But it is
arbitrary as a judgement about which edit the *user* wanted, which is exactly
why it must not be used to destroy the loser.

The rule that actually protects users is step 2's retention clause. A sync
engine that silently discards an edit is a sync engine that eats work, and users
do not forgive it — reasonably, since they usually cannot tell it happened until
much later.

## Tradeoff accepted

Users occasionally see conflict files in their folders, which is untidy and
requires an interface for resolving them. This is the correct trade: an
unwanted extra file is an annoyance, a lost edit is a betrayal.

## Note on timestamps

Wall-clock time is deliberately not used to order concurrent edits. Device
clocks disagree, sometimes by a lot, and a device with a wrong clock would win
or lose every conflict systematically. Timestamps appear only in conflict
filenames, where they help a human identify a version, not in the ordering rule.

## Reversibility

**Low cost while unbuilt.** After launch, changing conflict semantics changes
behaviour users have come to rely on, so it becomes a compatibility question
rather than a technical one.
