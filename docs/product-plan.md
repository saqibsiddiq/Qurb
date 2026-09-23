# Turning the engine into a product

A plan written against what the repository actually contains, not against what
the architecture document describes. Read
[CODEBASE.md](CODEBASE.md) first; this assumes it.

**Status: phase 0 done.** The data model is settled and built — see
[decisions/0029](decisions/0029-two-areas-shared-and-private.md). The rest of
this plan stands as written.

---

## 1. The thing that has to be settled first

The product specification and the implemented engine describe **two different
data models**, and the difference is not cosmetic.

### What the engine does today

One namespace, shared by every paired device.

```
        laptop                phone
          |                     |
          +------- one converging tree -------+
                  every path, everywhere
```

Verified in the source, not inferred:

| claim | where |
|---|---|
| A syncing device wants every path | `Role::Syncing => true` — `crates/engine/src/role.rs` |
| A peer is sent the whole tree | `Request::Tree => Response::Tree(store.tree()?)` — `crates/peer/src/server.rs` |
| `tree()` is everything the device knows | `Store::tree` → `Db::all_versions` |
| Any chunk is served to any paired peer | `Request::Chunk` has no access check |
| Authorisation is pairing, and nothing else | there is no ACL anywhere in `server.rs` |

So **every paired device today sees every file and may fetch any content.**
That is the Dropbox shape: add a file anywhere, it appears everywhere.

### What the specification describes

Per-device private vaults, and explicit sends between them.

```
        laptop                         phone
          |                              |
   +------+------+                  +----+----+
   | Phone vault |  <-- send only   | own vault|
   | Tablet vault|                  +---------+
   +-------------+
        no browsing
```

Three rules from the specification that the engine cannot currently express:

1. A device's vault is **private**: other devices may send into it and may not
   read it.
2. Sending is a **copy**, and the original stays where it was.
3. The desktop may know a phone's vault *exists* without being able to list it.

### Why this cannot be a UI layer

The specification forbids fake implementations (§80) and forbids exposing
private vault contents through a remote browsing interface (§51.6). Presenting
vaults as folders inside the shared tree would satisfy neither: the desktop
would still be able to read the phone's vault over the wire, whatever the UI
chose to draw. The privacy would be a drawing, not a property.

Making it a property needs three core additions:

- **A namespace per vault** in the index, so a path belongs to a vault rather
  than to one flat tree.
- **Authorisation in the protocol** — `Tree`, `Manifest` and `Chunk` answering
  only for what the asking device is entitled to. Today they answer everything.
- **A transfer primitive** — "put this content into that device's vault" —
  alongside the existing "converge these two trees".

None of that replaces the engine. Chunking, storage, encryption, conflict
resolution, transport, pairing and the storage cap are all unaffected. It is an
addition of scope and access control around them, and it is the largest single
piece of work in this plan.

### The decision, made

**Both.** The shared area stays and behaves as it always has; private vaults
are added beside it, enforced in the protocol rather than in the interface.
[Decision 0029](decisions/0029-two-areas-shared-and-private.md) records why,
and the three audiences the first attempt got wrong.

The desktop shell will be **Tauri**, which the roadmap originally called for.

---

## 2. What already exists and maps directly

Much of the product is already built and simply has no interface. Verified by
running it, not by reading about it.

| product requirement | engine reality |
|---|---|
| Pairing by QR or code (§8, §9) | built; `qurb pair` renders the QR, Android scans it |
| Direct connection, relay fallback (§36, §37) | built; `Connector::race` then `reach_via_relay` |
| "Connected directly" / "through an encrypted relay" | the distinction exists and is logged |
| Storage allocation (§5A, §28) | built; `limit` in config, enforced by the daemon |
| Free local space (§14) | built; `Store::evict`, and it **refuses** without another known holder |
| Available locally / remotely (§19) | built; `files.materialised` |
| Download a freed file (§30) | built; `qurb fetch` sets a durable want |
| Conflicts keep both versions (§24) | built; vector clocks, deterministic conflict names |
| Recovery phrase (§34) | built; 24-word BIP-39 with verification |
| Android share sheet (§39) | built; works with no network |
| DocumentsProvider (§40) | built |
| Background sync (§38) | built; WorkManager, and push when configured |
| Activity / transfers data (§26, §27) | the events exist; there is no store or API for them |
| Search (§31) | `Db` has the index; there is no search API |

The gap for most of these is an **API and a screen**, not an engine change.

## 3. What exists but is honestly incomplete

| thing | state |
|---|---|
| Desktop interface | one GTK window: status and a storage slider. Not a product shell |
| Selective sync (§29) | `PinSet` exists but is wired only to `Role::Replica`; an ordinary device takes everything |
| Replica storage cap | a replica cannot free space at all — no folder to evict from |
| Notifications (§45) | `notify-rust` is a dependency and nothing sends one |
| Windows (§44) | never built, never run, no CI. The engine is portable Rust; the daemon, watcher and tray on Windows are unverified |
| Installers (§69) | `packaging/install.sh` puts it in one user's menu. Not a package |
| Updates (§70) | nothing |

## 4. What does not exist at all

- Per-device vaults and their access control (§3, §4)
- Send-a-copy transfers (§11, §12, §21, §22)
- A sharing model (§23)
- Transfer history and activity storage (§26, §27)
- Search (§31)
- A design system (§73)
- Android↔Android verified in practice (§20) — the peers are symmetric, so it
  should work, and it has never been run

## 5. Decisions that need recording before code

Each of these changes architecture and therefore needs a decision record, per
CLAUDE.md:

1. ~~**Vaults, or one namespace.**~~ Decided: both —
   [0029](decisions/0029-two-areas-shared-and-private.md).
2. ~~**The desktop UI toolkit.**~~ Decided: Tauri.
3. **What sharing means** (§23) — copy or reference, whether deletion
   propagates, how revocation works. The specification requires this be defined
   rather than implied.
4. **Replica eviction**, before any storage UI promises apply to replicas.
5. **Selective sync for ordinary devices** — extending `PinSet` beyond
   replicas, and what happens to content a device un-pins.

## 6. Sequence

The specification's phases (§74) are sound. Reordered only where the repository
says something must come first.

**0. Decide the data model.** ✅ Done. Two areas, enforced in the protocol.
What remains from it: nothing yet *writes* to a vault, which is the transfer
primitive phase 5 needs.

**1. An API the UI can use.** The engine's surface is `reconcile`,
`plan_against`, `apply_plan`. A product needs to ask "what devices, what
transfers, what is available where" and to be *told* when those change. That is
a query-and-subscribe layer over the existing store and daemon — not a second
engine, and not a second database.

**2. Transfer and activity as first-class records.** The daemon knows what it
did; nothing writes it down. Both screens need it, and so does "why isn't my
file here" (§82).

**3. Desktop shell**: onboarding, storage selection, identity, recovery phrase,
home, settings.

**4. Pairing and devices**, on the existing pairing infrastructure.

**5. Transfers**: send, receive, progress, Downloads destination, notifications.

**6. Android product UI.**

**7. Cross-device flows**, including Android↔Android, offline, relay.

**8. Storage**: dashboard, local/remote, free space, fetch, selective
availability.

**9. Sharing**, once §5.3 is decided.

**10. Security, search, activity, polish, packaging, verification.**

## 7. Rules this plan holds itself to

From the specification, and from what this repository has already learnt the
hard way:

- The engine stays authoritative. No sync logic in Kotlin, JavaScript or the
  desktop UI; no second database.
- No screen ships on mock data. A mock is acceptable while a screen is being
  built and must not survive into a path a user can reach.
- The CLI keeps working. It is the diagnostic interface and several of this
  session's bugs were only findable through it.
- Safety invariants are not negotiable for UI convenience — in particular that
  the last known copy of anything is never dropped to satisfy a number someone
  typed into a settings box.
- Every phase updates the documentation as part of the phase.

## 8. What "done" means here

A feature is done when the UI exists, real data flows through it, error and
offline states exist, tests exist, documentation exists, and it works on
hardware where hardware is involved. Not when it compiles.

## 9. Known limitations that must not be papered over

- **Linux has no placeholder filesystem API.** An evicted file is absent from
  the folder. The UI must represent it and offer to fetch it; a FUSE mount to
  make a screenshot nicer is not worth a daemon whose failure takes somebody's
  folder with it.
- **Android decides when background work runs.** Push makes a change arrive in
  under a second when it is configured; without it, fifteen minutes is the
  floor and dozing makes it longer. The product must not promise otherwise.
- **The direct-connection rate is unmeasured.** A phone has synced over mobile
  data, but through an overlay network that did the traversal. What qurb's own
  hole punching achieves against a carrier NAT is still unknown, and it is the
  number the relay bill depends on.
- **Two devices that are never awake together never meet**, unless something
  always-on is in the picture.

## 10. Testing

The existing suite is 495 tests across eleven crates, and the classes that
matter here already exist: property-based convergence, crash injection,
corruption repair, hostile peers, concurrent collection. New work extends those
rather than starting a parallel tradition.

What this plan adds: API tests, UI state tests, and the end-to-end matrix in
§55 — desktop↔android, android↔android, desktop↔desktop — plus the real-hardware
behaviour in §56, which cannot be answered on an emulator.

## 11. What is not in this plan

Accounts, billing, a hosted service, and anything requiring a company to exist.
Those are Phase 6 in [roadmap.md](roadmap.md) and unchanged by this.

## 12. What I need decided

See §5. The first one blocks the rest.
