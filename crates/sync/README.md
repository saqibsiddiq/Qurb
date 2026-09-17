# qurb-sync

Deciding what changed, who changed it, and what to do when two devices
disagree.

This crate holds no data, touches no disk, and reads no clock. It takes two
views of the world and says what should happen. That makes it exhaustively
testable, which matters more here than anywhere else in the system: the failure
mode is losing someone's work.

## The problem

With a central server, "which version is current?" has an easy answer —
whatever the server says. There is no server here. Each device has its own
opinion and they have to converge without one.

```
local state ─┐
             ├─ reconcile ─→ adopt · offer · conflict · resurrect · merge
remote state ─┘
```

## Three ideas, in order of importance

**Version vectors, not timestamps.** A vector records how many changes from each
device a version has seen — `{laptop: 42, phone: 12}`. Comparing two answers the
only question that matters: did one happen after the other, or did they happen
without either knowing? Device clocks disagree, sometimes badly; causality does
not. Wall-clock time appears in this crate only inside conflict filenames.

**Concurrent means conflict.** When neither version has seen the other, no
amount of comparing decides which one the user meant. The content-hash tiebreak
decides only which keeps the original filename — it is arbitrary, which is
exactly why the loser is kept rather than discarded.

**Never silently discard an edit.** The one promise the system makes about
conflicts. It is why a concurrent edit beats a concurrent delete: a deletion
stays recoverable for the retention window, a discarded edit does not.

## Modules

| module | responsibility |
|---|---|
| `device` | device identity |
| `clock` | version vectors and their partial order |
| `version` | what one device believes about one path |
| `resolve` | deciding between two versions of a path |
| `reconcile` | deciding about a whole tree |

## Actions describe end states

`reconcile` returns what the world should look like, not what was decided:
"this file, with this content and this vector", rather than "the local side
won". Both devices compute identical end states from identical inputs, which is
what lets them converge with no negotiation protocol.

Every resolution of a concurrent case carries the **merged** vector. That is
what makes resolution terminate — a merged vector dominates both inputs, so the
resolved state is strictly later than what it resolved and the next comparison
finds a settled file. Without it, two devices re-detect the same conflict
forever.

## Testing

```bash
cargo test -p qurb-sync
```

The unit tests pin each rule. The convergence simulation in
`tests/convergence.rs` is what pins the *system*: two and three devices making
random edits, deletes, and syncs, which must all end agreeing — on content and
on history.

That distinction is not academic. It caught a bug every unit test missed: when
two devices reached identical content independently, the merged history was not
recorded, so the versions stayed concurrent forever and the next edit on either
side raised a conflict over content that never disagreed. The visible state was
correct the whole time.

`tests/properties.rs` now does this with `proptest`: twenty-one properties over
generated histories, with shrinking, so a failure arrives as the smallest
sequence that still breaks it. Failing cases are persisted as regressions.
`tests/adversarial.rs` covers clock skew and month-long absences — ordering
never consults wall-clock time, and those tests prove it rather than asserting
it.

## Not yet built

- **Move detection.** A rename is a delete plus an add, so a renamed large file
  transfers again rather than being recognised as the file it already is.
- **Directory-level operations.** Deleting a directory arrives as one removal
  per file.
- **Bounded vector growth.** A vector gains an entry per device that has ever
  touched a file and never loses one. Fine for a person's own devices, and it
  would need pruning before anything like shared folders.
