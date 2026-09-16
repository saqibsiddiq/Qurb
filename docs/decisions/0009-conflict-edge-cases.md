# 0009 — Conflict cases decision 0005 did not cover

**Status:** Accepted
**Date:** 2026-09-09
**Extends:** [0005](0005-conflict-resolution.md), which remains in force.

## Context

Decision 0005 established the rule for concurrent changes: detect concurrency
with version vectors, keep both versions, and let a content-hash comparison
decide only which one keeps the original filename.

Implementing it surfaced three situations the rule does not describe. All three
are common in practice, and each has a wrong answer that looks reasonable.

## Decisions

### A concurrent edit beats a concurrent delete

One device deletes a file while another edits it, neither aware of the other.
**The edit survives and the deletion is dropped.**

The two outcomes are not symmetric in cost. A deletion that loses is recoverable
— the tombstone and its chunks stay for the retention window, and the user can
restore. An edit that loses is gone, and 0005's central promise is that no edit
is ever silently discarded.

Keeping "both" is not available here: there is no second file to write, because
one side's contribution is an absence.

### Concurrent changes to identical content are not a conflict

Two devices can reach the same bytes independently — the same file copied onto
each, or the same file deleted on each. The vectors are concurrent, but there is
nothing to disagree about, and producing a conflict file would be pure noise.

**No data moves, but the merged history is still recorded.** This second half is
not optional, and omitting it is a real bug rather than an inefficiency: if the
two versions stay concurrent, the *next* edit on either device is concurrent
with the other's history, and raises a conflict over content that never
disagreed. That fault was caught by the convergence simulation and by nothing
else — every unit test passed while it was present, because the user-visible
content converged correctly and only the recorded history diverged.

### Resolutions carry the merged vector

Every resolution of a concurrent case — conflict, resurrection, or merge —
records a version vector that is the merge of both sides.

This is what makes resolution terminate. A merged vector dominates both inputs
by construction, so the resolved state is strictly later than what it resolved,
and the next comparison sees a settled file rather than the same disagreement
again. Without it two devices re-detect one conflict forever, generating
conflict files on every pass.

## Consequences

The plan a device produces describes end states rather than decisions:
"this file, with this content and this vector" rather than "the local side
won". Both devices compute identical end states from identical inputs without
coordinating, which is what lets them converge without a negotiation protocol.

One detail follows from that requirement. When two devices merge identical
content, the display metadata — which device last touched it, and when — must
also be identical on both, or they still disagree. It is chosen by device id,
purely because both sides can compute that without consulting a clock. The
choice is arbitrary and affects only what is displayed.

## Reversibility

**Low cost while unbuilt.** After launch these become behaviours users rely on,
and the delete-versus-edit rule in particular is the kind of thing people form
expectations about quickly.
