# 0020 — Sync takes a deadline

**Status:** Accepted. Amended 2026-09-27: what `timed_out` means, the deadline
now covers starting, a dropped connector stops its background work, and a pass
stays open briefly for peers to collect — see [Found on a phone](#found-on-a-phone)
and [A pass that waits to be collected from](#a-pass-that-waits-to-be-collected-from).
Amended 2026-10-04 by [0050](0050-large-files-from-a-phone.md): a pass may go on
answering a device that is collecting from it past its window, when its caller
asks with `sync_serving` -- the background worker, running in the foreground.
**Date:** 2026-09-17

## Decision

The mobile entry point for syncing is:

```
sync_within(seconds) -> SyncOutcome
```

not `sync()`. It stops when the time is up and says so in
`SyncOutcome.timed_out`, which is **not an error**. A connector is built for
each pass and dropped at the end of it, rather than held for the life of the
process.

## Why a deadline

Both mobile platforms hand background work a window and kill anything that
outstays it. iOS `BGTaskScheduler` grants short and unpredictable windows and
penalises an app that overruns by granting fewer of them later. Android's
`WorkManager` is more generous and still finite.

So "sync until finished" is not a neutral default on a phone — it is the way an
app loses its background privileges, and it does so gradually and invisibly.
An app cannot wrap a blocking call in its own timeout either, because killing a
thread mid-transfer is not something either platform offers.

The deadline therefore has to be inside, where the loop is.

## Why running out of time is not an error

Because nothing was lost.

The engine commits each file as it lands rather than at the end of a plan, so a
pass that stops halfway leaves every completed file stored, indexed and
correct. The remainder is simply work the next window will do. Reporting that as
a failure would be false, and would push an app towards showing the user an
error for the normal case.

`timed_out` exists so the platform side can schedule another window rather than
wait for the next scheduled one. That is a signal, not a fault.

## Why a connector per pass

The desktop daemon builds one `Connector` and keeps it for hours, because a
laptop's address is stable for hours and rebuilding it would mean re-announcing,
re-discovering, and briefly disappearing from the rendezvous service.

A phone's address is not stable for hours. It changes every time the device
moves between Wi-Fi and cellular, which on a normal day is several times. A
long-lived connector holding a public address discovered an hour ago announces
somewhere nothing can reach, and the failure looks like the *other* device being
absent.

Building one per pass costs a STUN round trip and a re-announce each time. That
is a few hundred milliseconds against a window measured in tens of seconds, and
it buys an address that is correct now.

It also happens to fix, for mobile only, the Phase 3 gap where a connection that
fell back to the relay stays relayed after the network improves: the next pass
starts fresh and gets a fresh chance at a direct path.

## Serving only while syncing

The desktop daemon accepts incoming connections continuously. A phone accepts
them only for the duration of a pass.

This is not a compromise — it is what the platform permits. A backgrounded app
is suspended the moment its window closes, and a socket it left open stops being
serviced regardless of intent. Pretending otherwise would mean holding resources
the system is about to take back.

The consequence is stated plainly because it is significant: **two devices that
are both only briefly awake may never meet.** Sync needs both ends announced and
listening at the same moment, since a QUIC handshake's opening packets are the
hole punch. Two desktops manage this by being on all the time. Two phones, each
awake for seconds a day at their platform's discretion, may not overlap for days.

This is the strongest argument in the project for a storage-only replica
([decision 0006](0006-availability-gap.md)) being part of a normal setup rather
than an advanced option, and it is why the roadmap plans iOS as a good viewer
rather than a peer equal to a desktop.

## Alternatives rejected

**Async functions across the FFI.** UniFFI supports them, and they map to Kotlin
`suspend` and Swift `async`. Rejected because cancellation across an FFI
boundary is genuinely hard to get right, and the thing being cancelled is a
transfer holding a database transaction. A deadline the Rust side enforces is
one mechanism in one place; cancellation would be two mechanisms in three
languages.

**A callback the platform can use to say "stop now".** Same objection, plus it
puts the decision on the far side of the boundary, where it would be called from
a thread that knows nothing about what the loop is in the middle of.

**Let the app pass a huge deadline and effectively sync forever.** Still
available — `sync_within(3600)` is legal — and that is deliberate: in the
foreground, with the user watching a progress bar, it is the right call. What
matters is that the parameter exists and has to be thought about.

## Consequences

- An app must schedule its own repeat passes. There is no background loop inside
  this crate, because a loop that outlives its window is the problem being
  avoided.
- A pass that reaches no peers is indistinguishable from one where every peer
  was asleep, and both are ordinary. `SyncOutcome` counts `unreachable`
  separately from raising an error for exactly this reason.
- The per-pass connector means STUN runs per pass. On a metered connection that
  is a small repeated cost, which is why `Settings::discover` can turn it off —
  at the price of only reaching peers on the same network.

## Found on a phone

2026-09-27, Galaxy S23. The rebuilt app's Settings screen showed the background
worker's last record as *"no paired devices"*, on a phone paired with a desktop
that was switched off. Two defects were behind it, both in how the deadline was
applied rather than in the idea.

**A device that never answered was reported as time running out.** Reaching a
device through the rendezvous service waits up to twenty seconds for an
introduction, and the background worker's window was twenty seconds. So the
window closed while the phone was still waiting, and the pass came back as
`timed_out` with nothing reached and nothing unreachable. The worker read that
as "no paired devices" — wrong — and answered `timed_out` with a retry, which on
Android is exponential backoff. That is the three-hour gap between syncs that
the worker had already been fixed once for, arrived at by a different route: a
switched-off computer is the ordinary state of things, and the ordinary
schedule is the answer to it.

`timed_out` now means what this record always said it meant — work left over:
a device that answered was still being synced, or devices were left untried. A
device that has not answered when the window closes is counted in
`unreachable`, however long it was waited for.

**Starting was outside the deadline.** Each pass starts a connector, which
connects to the rendezvous service, and that handshake had no bound at all. A
service that accepts the connection and never answers held the pass for ever,
whatever deadline was asked for — found while writing the test for the first
defect, which hung for ten minutes. The handshake is now bounded at five
seconds (`qurb_signal::client::HANDSHAKE_TIMEOUT`), which also bounds the
desktop daemon's start and its reconnect loop, and the phone's connector start
now runs inside the pass's deadline; a window that closes before the phone
could try anybody counts every device as unreachable.

What was observed and what was inferred: the phone's record and the paired
device are observed. The phone's own log of that run had rolled over before it
was looked for, so its outcome was not read directly; the mechanism was
reproduced instead. `a_device_that_never_answers_is_unreachable_not_out_of_time`
in `crates/mobile-ffi/tests/syncing.rs` gives the phone a rendezvous address that
never answers and an eight-second window, and without the fix returns exactly
`reached: 0, unreachable: 0, timed_out: true`. The older test for an unreachable
device accepted either answer, which is how this went unnoticed.

**Devices were tried one after another** — recorded here as not fixed, and
fixed the same day. With two paired devices and the first switched off, waiting
for the first could use the whole window and the second was never tried; the
pass came back `timed_out`, was retried, and the same happened next time, since
the order is by name and never changes. Every device is now reached at once,
each bounded by what is left of the window, and each is synced as soon as it
answers; syncing itself stays one at a time. `a_device_that_is_off_does_not_starve_one_that_is_on`
in `crates/mobile-ffi/tests/syncing.rs` pairs a phone with a working desktop and
with a device, named to be tried first, announced at an address that never
answers: with the old loop and a six-second window it returned `reached: 0,
unreachable: 1, timed_out: true`, and now reaches and syncs the desktop.

## A pass that waits to be collected from

2026-09-27, the same phone, with its desktop running and chosen to keep the
phone's files (decision 0036). The phone reached the desktop in a tenth of a
second, and the desktop never received the file the phone had just made.

**"A connector per pass" did not mean what this record said.** Dropping the
connector at the end of a pass freed nothing it had started: the rendezvous
reconnect loop, the beacon sender and the beacon listener ran on for the life
of the process, and the reconnect loop held the QUIC endpoint, so its socket
stayed open. A minute after a pass the phone had six UDP sockets, three beacon
listeners — one per pass since the app started — and was still announcing the
addresses of passes long finished. The desktop, hearing those, dialled them
every eight seconds and timed out each time. A connector now stops everything
it started when it is dropped (`crates/peer/tests/lifetime.rs`, seen to fail
first with five tasks outliving it). After a pass the phone holds one socket.

**Serving only while syncing was too short once syncing got fast.** Every
device pulls what it wants, so a photo the phone made moves only when the
desktop dials back and asks, which it does the moment it hears the phone. But
on a local network the phone's own syncing now finishes in under a second, and
the pass ended there, before the desktop could come. So a pass that reached a
device and has something waiting for one — a send not yet collected, a shared
file nobody else holds, or an own file when a device keeps them — announces
that it has news and keeps answering until it has been collected or ten
seconds have passed (`LINGER`), and never beyond the window. A private file
with nobody chosen to keep it waits for no one, so it holds nothing open.

On the phone, with both changes: the desktop, still busy with the stale
addresses above, connected ten seconds after the phone finished its own syncing
— inside the wait — and took the file. Without the wait the pass would have
been closed for nine of those seconds. Whether the wait is still needed once
the stale addresses are gone was not measured; the test written for it
(`one_pass_is_enough_for_the_desktop_to_collect`) passes either way, because its
stand-in desktop dials continuously, and says so.

**Not covered:** a deletion. It does not hold a pass open, since nothing is
"waiting" in the sense above. On the phone the desktop still took a deletion
within a second of the pass starting, because hearing the phone is enough to
make it dial; a desktop busy at that moment would miss it until the next pass.
