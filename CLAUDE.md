# Working agreements

## Documentation is part of the work

Not a follow-up task. Specifically:

- **[docs/CODEBASE.md](docs/CODEBASE.md) is the from-scratch guide.** It must
  stay current. Any change to how the system fits together — a new subsystem, a
  changed data flow, a moved directory, a corrected assumption — is updated
  there in the same piece of work that causes it. Update its "last verified"
  date only after checking the whole file against the code, not after editing
  one section.
- **Decisions with lasting consequences get a record** in `docs/decisions/`,
  numbered, never deleted. Reversed decisions are marked superseded with a
  pointer to their replacement.
- **Each phase gets a document** in `docs/phases/` written during the phase,
  recording what was built, what was measured under what conditions, and what
  was deliberately left undone.

## Structure

- `crates/` — code meant to last, held to a real standard.
- `experiments/` — throwaway spikes. May cut corners, provided the README says
  which ones. Nothing migrates silently to `crates/`; code that graduates gets
  rewritten deliberately.
- `docs/` — see above.

## Claims

Performance claims carry their measurement and its conditions — machine, corpus,
warm or cold cache. "Fast" is not a specification. A number without conditions is
not evidence.

Where a measurement contradicts a plan, the contradiction is documented rather
than quietly resolved. The Phase 0 chunk-parameter correction is the model:
[decisions/0004](docs/decisions/0004-chunk-parameters.md).

## Known gaps are stated, not hidden

Every document that describes something incomplete says so explicitly. The
project's value depends on being able to trust its own documentation about what
does and does not work yet.
