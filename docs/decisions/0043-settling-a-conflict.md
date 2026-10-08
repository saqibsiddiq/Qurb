# 0043 — Settling a conflict

**Status:** Accepted — each version previewed since 2026-10-08
**Date:** 2026-09-28

## Decision

When two devices change a file without either seeing the other, both versions
are kept, one under a conflict name ([0005](0005-conflict-resolution.md),
[0009](0009-conflict-edge-cases.md)). A person now settles it, on any device,
with one of three choices (brief §24):

- **Keep this version** — the one under the file's own name. The other goes to
  Recently deleted ([0042](0042-recently-deleted.md)).
- **Keep the other** — it takes the file's name; the version that had it goes
  to Recently deleted.
- **Keep both** — the other is renamed from
  `plan.conflict-a1b2c3d4-2026-09-28-101502.md` to `plan (phone).md`, after the
  device that made it.

Each is an ordinary change on the device where it is chosen — a rename, a
delete, a new version — so it reaches every other device by the usual rules and
the conflict is settled once for all of them. The two that need the other
version's bytes are offered only where they are.

The window shows conflicts at the top of Files and as a line on Home; the phone
as a card on Home; the command line as `qurb conflicts`.

## Why these

"Nothing was lost" has to stay true after the choice as well as before it. So
the version not chosen is never deleted outright: it is in Recently deleted,
restorable for thirty days, on the device where the choice was made.

## Found by name

A conflict is recognised by its file name, read back by
`qurb_sync::conflict_origin`, the inverse of the function that writes it. The
activity log has a `conflicted` event, but only on the device that noticed the
conflict — every other device just receives the copy as a new file, and has to
recognise it too. The name is on every device.

The parser is strict: eight lower-case hex digits, a UTC timestamp, then the
extension or nothing. A file somebody called `plan.conflict-notes.md` is theirs,
not a conflict, and is never offered for deletion as one.

**The consequence that lasts:** the conflict-name format is now read as well
as written. Changing `conflict_path` means changing `conflict_origin` with it,
and conflict copies already on people's disks keep the old format — a change
would have to go on recognising both.

## A device's name in a path

"Keep both" puts the other device's name in a file name. That name is chosen by
the other device, so path separators and control characters are removed from
it first; a test settles a conflict labelled `../../evil\n` and checks the
result stays beside the original.

## Seen before choosing

Since 2026-10-08 each version that is on this device is shown, side by side:

- an image, as itself;
- a text, by its start;
- anything else, by who made it, when and its size, as before.

The window's `preview` command reads the file from the folder and gives the
page an image as a data URL (under 6 MiB) or up to 2 KiB of text that is
UTF-8. The phone's `Previews.kt` does the same in the app. Nothing is shown
for a version not here: fetching one to look at it is not built.

## Not done

- A conflict inside a phone's own vault is found and settled the same way, and
  is rare: only its owner writes there.
- Which version kept the name is decided by content hash (0005), so "this
  version" is not necessarily the newer one; the screens say which device made
  each and when, rather than calling either one newer.
