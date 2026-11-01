# 0047 — Versions, installing, and upgrading

**Status:** Accepted; its "no automatic updater" superseded by [0078](0078-the-beta-updates-itself.md) on 2026-11-01
**Date:** 2026-09-28 (brief §69, §70)

## Decision

No automatic updater (brief §70: "do not build a dangerous auto-updater
casually"). What exists instead:

**Every build says what it is**, in three parts, because each matters for a
different question:

```text
qurb 0.1.0 · protocol qurb/2 · index schema 15
```

- the **version** of the program;
- the **protocol** it speaks to other devices — the TLS ALPN, `qurb/2`. Two
  devices with different protocols refuse to connect rather than misunderstand
  each other. *Every device must be updated together when this changes* (product
  plan §9);
- the **index schema** it writes — how many migrations it has.

`qurb version` on the command line, Settings → Version in the window and on the
phone.

**The index only migrates forward, and is copied before it does.** Migrations
are appended, never edited, and run when the index is opened. Before one runs
on an existing index, the index is copied as it was, with SQLite's
`VACUUM INTO`, to `index.before-schema-<n>.db` beside it — one copy, the
latest. That copy, with the older build, is the way back from an upgrade.

**An index from a newer build is refused**, not opened: "this index was
written by a newer qurb". An older build guessing at a schema it does not know
is how an index gets damaged.

**The configuration file**: a setting missing from an older file takes its
default, so upgrading never needs the file edited. A setting the build does not
know — written by a newer one — is refused, by name, rather than ignored: a
setting silently dropped is worse than a clear refusal. So a *downgrade* past a
new setting needs that line removed from `.qurb/config`, and says which.

**Installing and upgrading, by platform:**

| | install | upgrade | remove |
|---|---|---|---|
| Arch and its derivatives | `cd packaging/arch && makepkg -si` | the same, from a newer checkout | `pacman -R qurb` |
| other Linux, one user | `packaging/install.sh` | the same | `packaging/install.sh --uninstall` |
| Android | `./scripts/android-app.sh release` and install the APK | install the newer APK over it — **signed with the same key** | uninstall the app |

Each person's folder, key and settings live in their home and are touched by
none of these.

## The Android signing key

A release APK is signed with a key kept outside the repository
(`~/.config/qurb/signing.properties`, naming `~/.android/qurb-release.jks`);
generated 2026-09-28 at the owner's request. **Android refuses an update
signed by a different key**, and the only way past that is uninstalling, which
deletes the app's data — the phone's key and index. So:

- The key and its properties file must be backed up, and never lost.
- The phone that has been used for development runs a **debug** build, signed
  with the development key. Moving it to a release build means uninstalling
  once; that has not been done.

## What an automatic updater would need, and why it is not built

- Somewhere to publish releases, and a signing key for them separate from the
  build machine.
- A check that runs without leaking to the publisher which files a person has
  — at most "a qurb exists here".
- Staging the new binary, switching over at a restart, and keeping the old one
  and the index copy for a rollback.
- Coordination across a person's devices when the protocol changes, because a
  laptop updated alone stops talking to a phone that was not.

None of that exists, and a half of it would be worse than none: an updater that
can install is the most valuable thing on the machine to an attacker. The
package manager (on Linux) and the Play Store or an APK (on Android) are the
update paths until it does.
