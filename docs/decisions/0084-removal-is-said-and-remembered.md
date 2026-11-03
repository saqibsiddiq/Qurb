# 0084 — Removing a device is said to it, and remembered

**Status:** Accepted, 2026-11-03. Engine built and tested (builds 1 to 4 of
[connections.html](../design/system/connections.html)); the desktop's
screens for it are being rebuilt to the mockups (D21 to D23), and the phone's
("saqib removed this phone") are not built yet. **Not yet tried between the
owner's laptop and S23.** Build 5, a permanent identity for computers, is
not built: see *Left undone*.

## What was asked

The owner, 2026-11-03: "if either of the devices uses remove button, then
it'll be disconnected from both the sides, and the phone's data becomes
inaccessible, until and unless the same phone device connects again." Asked for
the whole flow of connecting and disconnecting with its edge cases
(connections.html), then: "build everything we have discussed."

## What was true before

`Store::remove_device` forgot the device here and nothing else. The removed
device was never told: it went on dialling, was refused at the TLS handshake,
and showed "unreachable" for ever. What this computer kept for it stayed under
its device id, but nothing remembered which device that was: added again — a
fresh pairing, a new row — it was a second device, and its files here were
orphaned.

## Decided

1. **Removing remembers.** `Store::remove_device` writes the device's id,
   fingerprint, name and relation to `removed_devices` (migration V32) before
   forgetting it. What it keeps here stays, under its id, locked to everyone.
2. **The removed side is told.** A listener lets in the trusted and the
   devices it removed (`accepted_fingerprints`); a removed device's every
   request is answered `Response::Removed` (status 32), which the client turns
   into `Error::Removed`. The tolerant requests that otherwise swallow errors
   (`tree_since`, `tree_digest`) pass it through. On hearing it, the daemon
   (`sync_one`) and the phone (`sync_reached`) forget that peer and remember
   it as `by_them`. Devices removed *by* the other side are not let back in:
   being removed by someone is not a reason to accept them.
3. **A goodbye, best effort.** `Request::Goodbye` (tag 37) tells the other
   device at once, on a connection already open: the daemon says it before
   closing the connection of a device it removed (`Peers::goodbye`, 3 s); the
   phone on the kept-folder connection if one is open, never dialling for it.
   Without one, the next request hears `Removed` anyway.
4. **The same device coming back is recognised.**
   - The same install pairs again under its own device id: pairing
     (`trust_peer`) clears it from `removed_devices`, on either side, and its
     files are its own again with nothing to copy.
   - Set up again since (a new device id), it says its permanent identity
     (`Request::Become`, decision 0079); a fingerprint this computer removed
     (as one of its person's own devices) is trusted again *under its old
     device id*, its kind carried over, and the passing pairing is that
     device's earlier self (`same_device`). Another person's removed device
     is refused this.
   - While it asks to pair, the desktop says so (D23): `pairing_state.back`
     is what is kept here for it, when its fingerprint is one removed here.
     That is the same install only: a phone set up again pairs under a passing
     identity, and is known only after its `Become`.
5. **A removed device's files are listed, locked, with Delete.**
   `storage_parts.removed` on the desktop; `delete_removed` moves them to
   Recently deleted (`Store::delete_kept_for`), as any deletion. The device
   is still recognised if it comes back, with nothing kept.

## Measured

Nothing on hardware yet. Tests, on the owner's laptop (Ryzen, CachyOS),
`cargo test --workspace --exclude qurb-desktop`, all passing:
`crates/peer/tests/removal.rs` — a removed device is told so; a stranger is
still refused; a goodbye is heard and remembered as `by_them`; a removed phone
set up again is the device it was, its kind kept and nothing left removed; the
same install paired again is removed no longer.

## Left undone

- **Build 5, a permanent identity for computers.** A computer set up again is a
  new device to its phones. The phone's hand-over (0079) does not carry over
  safely: a phone that took a computer's new fingerprint would refuse its old
  one, and the daemon holds its identity for its whole run, so the two could
  not reach each other until the daemon restarted. Needs the daemon to take a
  new identity while running, or at a restart it arranges. Rare (a computer's
  identity survives everything but losing its store), so left.
- **The phone's screens** for being removed ("saqib removed this phone", Add
  it again) and recognition: the engine says it (`removed_devices`, `by_them`);
  nothing draws it yet.
- **A removed device that never connects again** is remembered for ever; no
  expiry. Its files stay until deleted.
- `Request::Changes`, the one request answered outside `answer`, is not
  short-circuited for a removed device; it can learn only a generation number.
