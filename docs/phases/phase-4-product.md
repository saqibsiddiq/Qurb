# Phase 4 — Desktop product

**Status:** in progress
**Target:** months 8–10

Turning a working engine into something a person can run. The phase the roadmap
warns is not the fun part and is a full quarter.

## Progress

| area | status |
|---|---|
| a daemon that runs | ✅ [`qurb`](../../crates/qurb/) |
| the commands around it | ✅ init, enrol, pair, join, run, status, verify, reclaim, fetch, free, send, cancel, holders, remove-device, conflicts, share, keep, deleted, restore, activity, ls, find, config, protect, version |
| running the services | ✅ `qurb signal`, `qurb relay` |
| push, rather than polling | ✅ ~430ms, measured |
| protecting the key at rest | ✅ keystore and passphrase |
| storing files in parallel | ✅ 487 → 830-888 files/s |
| onboarding and the recovery phrase | ✅ in the window: phrase shown and confirmed, the storage question asked ([0038](../decisions/0038-the-storage-question-during-setup.md)) |
| installers | ◐ a pacman package for Arch and a per-user install script; no `.deb`, Flatpak or AppImage |
| signed updates with rollback | ⬜ deliberately not built ([0047](../decisions/0047-versions-and-upgrades.md)); the index is copied before it migrates, and a newer one is refused |
| observability | ◐ structured logs, nothing more |
| the interface | ◐ a window that does what the command line does — see [features.md](../features.md); not yet designed, which is next |
| one daemon per folder | ✅ an advisory lock, not a convention |
| pairing | ✅ scan a QR code, once, and it stays paired |
| a folder that needs no path | ✅ `~/qurb`, with a registry (it was `~/Downloads/qurb` until [0037](../decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)) |
| files sent to this desktop | ✅ saved to `Downloads/qurb` as ordinary files |
| transfer progress | ✅ both directions, live, with rate and time left |
| cancelling a send | ✅ before it is collected; ⬜ stopping one mid-transfer |
| storing each file once | ✅ the folder *is* the payload store |
| a storage cap | ✅ limit, eviction, fetch-back — and a slider |
| a replica anybody can run | ✅ `qurb replica` |
| syncing from another network | ✅ needs a reachable rendezvous; see anywhere.md |
| changes crossing at once | ✅ ~1s between idle daemons, no polling |
| the services deployable | ✅ systemd units, TLS, ports written down |
| garbage collection running | ✅ every 5 minutes, 7-day retention |
| removing a device | ✅ trust ends at once, open connections too ([0041](../decisions/0041-removing-a-device.md)) |
| recently deleted | ✅ 30 days, restored on every device ([0042](../decisions/0042-recently-deleted.md)) |
| settling a conflict | ✅ keep one, the other, or both ([0043](../decisions/0043-settling-a-conflict.md)) |
| a folder shared with chosen devices | ✅ in tests with three devices; not yet between real ones ([0044](../decisions/0044-sharing-with-chosen-devices.md)) |
| a folder kept only remotely | ✅ in tests with two devices ([0045](../decisions/0045-a-folder-kept-remotely.md)) |
| the passphrase in the window | ✅ and a Security section ([0046](../decisions/0046-the-window-asks-for-the-passphrase.md)) |
| versions | ✅ program, protocol and index schema, everywhere ([0047](../decisions/0047-versions-and-upgrades.md)) |

753 tests in 88 test binaries on Linux (2026-09-29, debug build, the
development laptop): 746 pass, and the seven that need multicast failed on a
network that does not carry it (see *The design, in the window*); clippy is
clean.

## The interface

A system tray icon — [`crates/tray`](../../crates/tray/) — that runs the daemon
rather than talking to one. A socket between them would buy attaching to an
already-running daemon and cost a second surface to design, version and secure;
two daemons on one store collide on the SQLite lock regardless, so the choice is
really which single process runs. `qurb run` stays for machines with no screen.

This needed the daemon to say what it is doing. It had only ever had its log,
which is right for a terminal and useless to an interface: a person wants "up to
date, three devices, last synced two minutes ago", not a stream of events to
reconstruct it from. The daemon now publishes a `Status` on a `watch` channel —
watch rather than broadcast, because a display only wants the current value and
an icon catching up through stale summaries would be showing the past.

**It will not say "up to date" when no device has been reached.** A contented
icon beside a store that has not spoken to another device in a week is the kind
of lie that makes people stop trusting a sync tool, so `State::Alone` exists and
reads "no devices reachable". It also separates *no paired devices* from *paired
but unreachable*: one is a setup step, the other a network problem.

### GNOME has no tray, and says nothing about it

The nastiest part. GNOME removed the system tray and ships no StatusNotifier
host, so an icon there is invisible — and creating one still **succeeds**.
Nothing returns an error. A program trusting that return value is running,
unseen and unquittable.

So the session bus is asked whether anything has registered
`org.kde.StatusNotifierWatcher` before anything is built. With no watcher the
program says so, explains how to get a tray back, and keeps syncing with status
on stderr.

The check has to come before GTK is touched for a second reason, found by
running it: `tray-icon` is GTK-backed on Linux and *panics* if a menu is
constructed before `gtk::init`. An error path alone would never have run.

### Two bugs the fallback exposed at once

Having a display made two invisible problems visible immediately.

It reported **"0 files" with two in the store**, because counting happened after
the networking that had just failed. What a device holds is knowable without
reaching anything, and an interface showing zero because a service is down is
worse than showing nothing.

And when the daemon died, the status still said **"syncing"** — the failure
returned without reporting it, so the display kept showing the last thing that
had been true. It now records why it is stopping before it stops.

Neither was a new bug. Both had been there all along, invisible because nothing
was looking.

## Protecting the key at rest

The largest security gap the project had. The master key sat in a file readable
only by its owner, which defends against other users of the machine and against
nothing that can read the disk.

Three options now, and the difference between them is worth stating because a
user reading "end-to-end encrypted" will assume the strongest:

| | defends against | starts unattended |
|---|---|---|
| `file` | other users of the machine | yes |
| `keystore` | anyone reading the disk while it is locked | yes |
| `passphrase` | anyone who takes the disk *and* the session | no |

File remains the default, which looks like timidity and is not: a headless
machine may have neither a keystore nor anybody to type a passphrase, and a
device that cannot unlock itself is worse than one whose key sits in a file.
`qurb protect` changes it, and says what each option does before it does
anything.

Changing the protection changes the lock and not the contents — the key is read
out and written back, so nothing it protects becomes unreadable. The old copy is
removed only once the new one is in place, because a device that loses its key
halfway through being made safer has been made catastrophically less safe.

The keystore path is tested here against the Secret Service. Keychain and the
Windows Credential Manager go through the same library and are exercised by
nothing, which the documentation says rather than implying otherwise.

## The daemon

Until now nothing outside tests and examples could be run, and the demo put two
devices in one process — which meant the one thing that mattered for the Phase 3
kill criterion, *two machines on two networks*, was impossible.

`qurb run` watches a directory, keeps the store in step with it, serves peers on
every path it has, and pulls from each paired device it can reach.

## Three bugs that only running it could find

Every one of these passed the whole test suite.

### An invite offered an address nobody could dial

`qurb pair` bound `0.0.0.0` and put that in the invite. It is a true statement of
where the socket is listening — every interface — and completely useless to a
peer, which cannot connect to it.

Every test bound `127.0.0.1` explicitly and so never saw it. The failure appeared
at the *joining* device, which reported that it could not connect, making it look
like the joiner's problem.

The fix asks the routing table rather than listing interfaces: a UDP socket
*connected* to an arbitrary address sends nothing but makes the operating system
choose a source address, and the one it chooses is the one that would really be
used. Listing interfaces instead means guessing between a wired connection, a
wireless one, a virtual machine bridge and three container networks.

### Two devices started together took two minutes to find each other

The first sync attempt happens before the other device has announced, which fails
— and the retry backoff went straight to two minutes. Starting both daemons at
once, as anyone would, produced a system that appeared to do nothing at all.

The two failures are indistinguishable at the moment they happen and want
opposite treatment: a device still starting up should be retried in seconds, one
that is switched off should be left alone. So the backoff now doubles from five
seconds to a two-minute cap. Simultaneous startup went from 120 seconds to **9**.

### An edit took thirty seconds to cross

A device syncs when *it* changes something, or when its timer fires. An edit made
on the laptop therefore reached the desktop only when the desktop next asked.

**Now fixed.** A device holds one request open against each peer — "tell me when
your state differs from this" — and the answer arrives when it does.

Measured, two devices on this machine, the same edit three times: **433ms, 432ms,
431ms**, against up to ten seconds before. Most of what remains is the watcher
deliberately waiting to see whether the file is still being written, which is a
floor worth having rather than latency to remove. Thirty seconds of complete
idleness produced no log activity at all, so the responsiveness is not bought
with chatter.

It does not break the rule that **a peer can ask and never tell**: the device
that wants to know is the one asking, and the answer simply arrives later than
usual. A peer still cannot make anything happen.

A counter rather than a flag, because a flag can be missed — a peer told
"something changed" cannot distinguish a notification it has already acted on
from a new one. It says what it last saw, and gets an immediate answer if
anything has happened since. The counter deliberately does not survive a restart:
a peer holding a number from before sees one that does not match, concludes the
device it was watching has been away, and looks. Which is right.

## What running it confirmed

Two devices, two directories, real sockets: paired over the LAN, synced in both
directions, propagated a deletion, and resolved a genuine conflict — both
versions kept, the same conflict filename computed independently on each device,
and both agreeing on the resulting file list afterwards. `qurb verify --deep`
reports everything agrees.

## Pairing, once — and a folder nobody has to name

Three things stood between the daemon and something a person could be handed.

### "Paired" did not survive the daemon restarting — or rather, starting

The TLS verifier held its list of trusted devices as a `Vec<Fingerprint>`
snapshot, taken when the connection layer was built. `qurb pair` runs as a
*separate process*, so a daemon that was already running never learned about a
device paired afterwards. Pairing appeared to work — it wrote the fingerprint —
and then nothing connected until the daemon was restarted.

The snapshot is now a `TrustList`: an `Arc<RwLock<Vec<Fingerprint>>>` shared
between the verifier and the daemon, replaced wholesale every five seconds from
disk. Wholesale rather than appended, so that *forgetting* a device takes effect
too. Learning a new fingerprint also triggers an immediate sync rather than
waiting for the next interval.

Measured on two fresh devices on the development laptop, 2026-09-22: **five
seconds** from `qurb pair` completing to the devices connected, against never
without a restart.

### The pairing code was eight lines of base32 read off a screen

Now `qurb pair` renders the same invite as a QR code in the terminal, and the
Android app scans it with CameraX and ML Kit. The invite is unchanged — same
bytes, same expiry, same one-time use — so nothing about the security argument
moves. What changes is that a phone no longer needs a keyboard, and a person no
longer needs to transcribe a fingerprint to know they paired with the right
machine.

### Every command wanted a path

`qurb run ~/qurb` is fine once. It is not fine as the thing a person types
daily, and it is impossible as the thing a desktop launcher does. Commands now
default to a folder: `$XDG_DOWNLOAD_DIR/qurb`, falling back to
`~/Downloads/qurb`, with a small registry under `$XDG_CONFIG_HOME/qurb/folders`
recording which folders exist so the most recently used one wins. The legacy
`~/qurb` is still found if it is there.

Since 2026-09-25 a *new* folder goes in `~/qurb` again. `Downloads/qurb` became
where files sent to this device are saved, and the two must never overlap — see
[decisions/0037](../decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).
A folder already in Downloads stays there and is still found.

Several people on one computer is answered by several operating-system
accounts rather than by a qurb-level notion of a user — see
[decisions/0023](../decisions/0023-one-person-per-account.md).

## Every file cost twice its size

Content lived in two places on every syncing device: the file in `~/qurb`, and
compressed, encrypted chunks of the same bytes in `~/qurb/.qurb/chunks`. Nobody
had noticed because the test corpora were small and the store was never
compared against the folder that produced it.

Measured on the development laptop, 2026-09-22: 7.6 MB of files in `~/qurb`,
7.5 MiB of chunk payloads holding exactly that same content.

The fix is that a materialised file *is* the payload store for the chunks it
holds — the chunk store keeps only what the folder cannot supply, and reads
that find no payload seek into the file and hash-check what they find.
[decisions/0024](../decisions/0024-the-file-is-the-payload-store.md) records
what that costs, which is not nothing: editing a file now makes its superseded
versions unreadable rather than recoverable.

`qurb reclaim` frees the duplicates an older store still holds. On the real
laptop store: 7.5 MiB across 15 chunks, `qurb verify --deep` clean afterwards.

This was found while sizing up the storage cap, and it had to be fixed first: a
cap that counts every file twice is a cap on half of what the user thinks they
are limiting.

## Telling a device how much disk it may use

`qurb config <dir> limit=10G`. Over the limit, qurb drops local copies of the
coldest files and keeps everything the index knows about them, so the path
still syncs, still lists, and comes back on `qurb fetch`.

The interesting part of this feature is not the freeing. It is the two refusals.

**A file is dropped only when another device is known to hold those exact
bytes**, recorded when this device adopts a version another device made. So a
device never evicts content it originated — the phone keeps the photo it took,
the desktop may drop the copy it was sent. A device that cannot free enough
stays over its limit and says so in the log, which looks like a bug and is not:
a limit is a promise about disk, and no number in a settings box outranks the
only copy of someone's work.

**Dropping a file must not look like deleting it.** A syncing device decides a
file was deleted by not finding it during a scan — so without care, a device
short of disk drops a file, the next scan calls that a deletion, and the file
is deleted on every other device. The index now records whether this device is
*holding* each file separately from whether the file is there, set before the
unlink and never after, and both the scan and the watcher skip a file that is
missing on purpose.

Measured on the development laptop, 2026-09-22, two devices on loopback over a
real QUIC connection with a 2 MiB limit and 4.8 MiB of content:

| | device that received the files | device that made them |
|---|---|---|
| dropped | 1 file, 3.0 MB | nothing |
| ended at | 1.9 MiB, under the limit | 5.0 MB, 2.8 MB over, deliberately |

Neither device recorded a deletion, and the other device still held both files.
`qurb fetch` brought the dropped file back on the following sweep, byte-identical.

Garbage collection is part of this and had never run: `Store::gc` was written
and tested in Phase 1 and had no caller outside tests, so superseded chunks
accumulated without limit — 82 MB of them on this laptop for one deleted file.
The daemon now collects every five minutes with a seven-day retention window,
before it checks the limit. Collecting first costs the user nothing; only then
is it fair to drop copies of files they still have.

Two things this leaves undone. An evicted file **vanishes from the folder** on
Linux, which has no placeholder API — `qurb status` is the only place that says
it still exists. And `qurb fetch` waits for the next sweep rather than nudging
the daemon, so a file can take up to two minutes to come back.

## A window with a slider in it

The tray icon was the interface, and on GNOME there is no tray, so on the most
common Linux desktop the interface was a window that could only be *read*. The
one setting people actually want to change — how much of the disk qurb may
have — was reachable only by typing `qurb config <dir> limit=10G`.

So the window now has it: a usage bar, a checkbox for whether there is a limit
at all, and a slider that runs from 1 GiB to the size of the filesystem the
folder is on. Offering more than the disk holds would be offering a number that
cannot mean anything.

Two details decided the design.

**The slider writes the settings file, and nothing else.** Not a socket to the
daemon, not shared memory — the file the daemon already reads. That makes
`qurb config limit=10G` in a terminal and the slider in the window literally
the same act, and neither can leave the other showing something stale. The
daemon re-reads it on every maintenance pass, so a change applies without a
restart; a setting that needs a restart is a setting people think is broken.

**The window reads the file too, rather than the daemon's published status.**
The daemon only republishes on a sync pass, so a slider driven from the status
would sit at its old value for up to two minutes after someone moved it —
visibly snapping back under their finger.

## Two daemons on one folder

Found by running one: the desktop app launched from the applications menu and
`qurb run` typed into a terminal are the same daemon with different faces, and
neither knew about the other. Two of them on one store contend on the SQLite
write lock, answer as the same device on the network, both reconcile the same
directory and both enforce the same cap — and none of that reports an error. It
is slow and confusing rather than broken, which is worse.

`crates/qurb/src/lock.rs` takes an advisory `flock` on the store for the life
of the daemon. The kernel releases it when the process dies, which a lock file
holding a PID could not promise after a crash. The second daemon now refuses
with a sentence saying what is already running.

The same run turned up two smaller things. The window showed nothing when the
daemon failed underneath it — it reported files and folders from a process that
had stopped — so a problem now appears in the window in red. And the folder
registry had test folders in it from an afternoon's work, which meant the app
launched from the applications menu opened a throwaway directory in `/tmp`
rather than the real one. That one is a lesson about test hygiene rather than a
defect: the registry is per-user configuration, and tests must not write to it.

## Nobody waits for a poll

Two idle daemons, both connected to the same rendezvous service, still took up
to two minutes to move a file neither was busy with — because a device that had
just changed something had no way to say so, and the peer had no reason to ask.

`Waiting { to }` says there is something for a member: who, never what. The
service forwards it to peers that are connected and keeps it for peers that are
not, delivering it the moment they appear. That second half is the one that
matters, because the device most in need of telling is the one that was asleep
when the change happened.

Measured, two idle daemons on this laptop: a file written on one was on the
other **one second later**. The sender recorded the change at 27.087, the
recipient logged the notice at 27.088, and the transfer finished at 27.099.

Notes collapse — fifty changes for one absent peer leave one note, since the
answer to "should I sync" is the same either way — and a device never nudges
back the peer the news came from, which would loop for ever.

## Two devices that were never reachable enough

Two connection defects turned up together, and both had been there all along.

**A device announced one address**: whichever its default route used. On a
laptop at home that is `192.168.1.5`, which is nothing at all to a phone on a
mobile carrier — so every connection from outside the house depended on
punching a hole through the home NAT, and an overlay network's address, which
would have worked first time, was never mentioned. `Endpoints.local` was
already a list and the candidates were already raced; only the filling-in was
missing.

That broke two tests immediately, and both were real.

Announcing the machine's other interfaces when the socket is bound to *one*
address advertises places nothing is accepting — a peer races addresses that
can only fail and concludes the device is unreachable. It enumerates only for a
wildcard bind now.

And racing four addresses instead of one grew a 128 MiB transfer from about
30 MiB of heap to 105. Dropping a handshake that has *already finished* leaves a
connection established at the far end holding the buffers of a transfer nobody
will use, and a device reachable on both a local network and an overlay hits
that every time. The losing paths are closed as they land.

**A rendezvous service restart disconnected every device permanently.** The
signalling connection went with it and never came back; the daemon kept syncing
on its timer, so nothing looked broken — it had simply stopped being reachable
and stopped being able to say it had news. Every deploy of a hosted service
would have done that to everyone. It reconnects now, doubling from a second to
a minute and resetting after a connection that lasted. Verified live: a daemon
left for two minutes with no service reconnected twenty-six seconds after one
appeared.

## The window

Recorded here late: the desktop application was built in this phase and this
document did not say so. It is [`qurb-desktop`](../../crates/desktop/README.md),
a Tauri window that runs the daemon inside itself
([decision 0032](../decisions/0032-the-interface-hosts-the-daemon.md)): home,
files with where each one's contents are, devices, activity, storage with the
allowance, sending by dropping a file on the window, settings, and setting a
device up from nothing — the 24 words shown, three of them confirmed, and never
written down ([decision 0033](../decisions/0033-the-phrase-on-a-screen.md)).
Pairing is a QR code, a typed code or a spoken one, with a countdown. Three
things raise a notification and nothing else does: a file sent to you, one
collected, and a failure.

The applications menu launched `qurb-tray`, the smaller front end, rather than
this window, until 2026-09-27: it opens the window now
([decision 0040](../decisions/0040-the-menu-opens-the-window.md)).

## Files sent to a desktop go to Downloads

**2026-09-25.** A file somebody sends this desktop is saved as an ordinary file
in `Downloads/qurb`, outside the folder, and qurb stops tracking it: it is not
scanned, not counted against the limit, and deleting it is the person tidying
their Downloads. That is
[decision 0037](../decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md),
which records what was built, how it was verified, and what is not done yet —
notably that a phone cannot send one yet, so phone to desktop is unexercised.

Building it turned up two defects that had nothing to do with Downloads and
were serious anyway.

### A path is an instruction

A version from another device names a path, and the receiving device joins it
onto its folder and writes there — or, for a deletion, deletes there. **Nothing
checked the path.** Not the wire format, which bounds its length and checks it
is UTF-8, and not the engine. A paired device could have sent `../../.bashrc`,
an absolute path, or `.qurb/config`, and this device would have written it, or
deleted it.

Pairing does not make that safe. It proves which device is talking; a device
that is stolen or compromised keeps its pinned identity, and the whole point of
the hostile-peer tests in Phase 2 was that authentication says nothing about
whether a peer is telling the truth. Those tests covered wrong bytes, nonsense
and silence. They did not cover a well-formed message naming somewhere it
should not.

Now refused in two places, with one rule —
[`qurb_sync::is_safe_path`](../../crates/sync/src/path.rs): relative, no empty,
`.` or `..` components, no NUL. The wire refuses a tree containing such a path
as a whole, since a peer that sends one is not a working copy of this software.
The engine refuses it again before writing or deleting, and also refuses
anything its ignore rules exclude, which keeps a peer out of qurb's own store
inside the folder.

Checked by disabling the engine's check and running
`crates/engine/tests/hostile_paths.rs`: every hostile version was written, one
file landed a directory above the test's own temporary directory, and a file
outside the folder was deleted. With the check, all of them are refused.

Backslashes are not refused. On Linux they are ordinary characters in a name;
on Windows they would be separators, and that is one of the things supporting
Windows will mean revisiting.

### Taken once, for good

A delivery is taken once, keyed by content, so that the sender offering it
again changes nothing. The only record of having taken one was the received
file's own row — and once the person deleted the file, that row was a tombstone,
which garbage collection expires after the retention window. The daemon keeps
tombstones for seven days, so a file sent to a desktop and deleted there would
have arrived again once its tombstone expired. (A phone was spared only because
nothing on a phone collects garbage at all — which is its own gap: tombstones
and released chunks there are never reclaimed.) Shown by
`a_deleted_delivery_is_not_offered_again_after_collection` in
`crates/engine/tests/received_files.rs`, which collects with no retention
window and fails without the fix; not waited out for a real week.

A `taken` table, never expired, is the record now; the index migration fills
it from everything an existing device had received. Found because a delivery
saved to Downloads has no row at all, which would have made the same thing
happen on the very next sync.

## A transfer you can watch

**2026-09-25.** A file moving between this device and another now shows on a
Transfers screen: how much of it, how fast, and about how long is left — in
both directions.

The engine reports bytes as they are written, but only for content that
actually crosses from the other device. A rename or a copy of something
already here costs a lookup, and showing it as a transfer would be showing work
that is not happening. The daemon publishes what it is told on the same status
channel that carries "syncing" and "up to date" — live state, never the index,
as [decision 0032](../decisions/0032-the-interface-hosts-the-daemon.md) says it
must be — and at most four times a second, so a fast transfer is not spending
its time describing itself.

**Seen working**, on the development laptop: the real window receiving from a
second device on the same machine, both release builds, stores on tmpfs. A
1.2 GiB file showed its bar filling at 153 to 207 MiB/s with the time left
counting down, and moved to *Finished* the moment it arrived. Those rates are
what the screen displayed over loopback, not a measurement of anything a real
network would do.

Watching it found one thing no test did: *Finished* was drawn only when the
screen opened, so a file that had arrived vanished from *Arriving now* and
appeared nowhere. It is redrawn now whenever something stops arriving.

**Sending is the harder direction**, because a sender never sees a file move.
The other device asks for chunks by hash and never says which file they belong
to, and it never says it has finished — it just stops asking. So the peer
server reports each chunk it serves, and to whom, and the daemon traces the
chunk back to the send it is part of with one indexed query. Chunks of shared
files are not traced: they are synced, not sent, and the receiving device is
the one with something to say about them. A send that has not moved for ten
seconds stops being drawn as moving; the other device's `Got` is what moves it
to *Finished*.

Seen working the same way: the window on the sending device, a second device
collecting a 500 MiB send, the bar reading "phone is collecting it" at
138 MiB/s and the file moving to *Finished — delivered to phone* once it was
collected. Watching it found two more things the tests did not: *Finished*
listed a send as done the moment it was queued, and did not update when the
send was collected. Both fixed.

Not built: cancel, pause and retry.

## Several files, and folders

**2026-09-25.** A send can be any number of files and folders, from the window
or from `qurb send`. A folder goes whole under its own name and arrives as a
folder, on a desktop's Downloads and on a phone alike. What to send is worked
out in one place, `qurb_cli::send`, which reads only the filesystem so the
window can store each file separately rather than hold its lock for a whole
folder.

Three rules came out of writing the tests for it: links inside a folder are not
followed, since they can point anywhere including back up the tree; a qurb store
inside a folder is never sent, since it holds the device's keys; and two things
picked together with one name are both sent, the second as `notes (2).txt`,
because under one name the second would replace the first before either
arrived.

Two older defects were found on the way. `qurb send report.pdf to laptop`, the
short form, opened `report.pdf` as the qurb folder; only the long form had ever
worked. And sending an empty file recorded no *sent* event, so it never
appeared in history.

**Verified** with two devices on the laptop: from the command line, a folder
with a nested subfolder, an empty file and a link, plus two files both called
`notes.txt`, arrived as sent — the link skipped and named, the second
`notes.txt` as `notes (2).txt`, a 2 MiB file byte-identical. From the window, a
folder chosen through the real GTK folder chooser arrived as `Trip/b.txt` and
`Trip/day1/a.txt`. Dragging onto the window was not exercised: nothing here
can synthesise a drag from another application.

## Taking a send back

**2026-09-25.** A send nobody has collected can be cancelled, from the window's
Transfers and Send screens or with `qurb cancel`. It becomes a tombstone in the
recipient's vault, which a recipient never takes as a delivery, so it does not
arrive however long the device was away. Once the device has reported holding
it, cancelling is refused: the file is theirs, as decision 0030 already said.

The window's button is pressed twice — the first press asks, in place, and the
second within five seconds cancels. In the page rather than a dialog, because
the list redraws every second and a half and would otherwise redraw under the
question.

Verified in the window with two devices on the laptop: two files queued, one
cancelled with the two presses, the other device started an hour later — only
the other file arrived, and *Finished* read "taken back before desk collected
it".

Not built: stopping a transfer already moving, in either direction, and pause.
A failed file needs no retry button — the next sync retries it — which the
Transfers screen does not yet say.

## Finding what was sent to you

**2026-09-25.** A received file on the Transfers screen has *Show in folder*,
and Settings has where files sent here go, with *Open that folder*. The folder
opened is looked up from the history entry on the Rust side, never taken from
the page, and only a folder inside the downloads directory is opened — checked
after resolving links, so a link inside Downloads pointing elsewhere does not
count. A changed location is refused in Settings if it overlaps the synced
folder, and a running daemon picks it up on its next pass.

**Verified:** the path check by three unit tests, including a `../` path and a
link out of Downloads. The live pickup with two devices on the laptop: the
location changed with `qurb config` on a running device, and the next file sent
landed in the new place with no restart, the daemon logging "files sent here
now go to". An overlapping location was refused while it ran.

**Not verified:** the Settings screen itself and the two buttons were not
pressed in the window. Screenshots of the window came back blank for this run,
though the page was drawing — the accessibility tree read the setting off the
screen — and filling a text field needs keystrokes sent into the desktop, which
was stopped rather than risk them reaching another window. `xdg-open` was not
run, since it opens a file manager on the screen.

**Measured on the way:** with no rendezvous service, a receiving device already
running took **19 seconds** to collect from a sender that started after it —
two daemons on one laptop, loopback. The sender reached the receiver at once;
the receiver found the sender only on its own next look. Recorded rather than
changed here.

## Directly, or through the relay

**2026-09-25.** The Devices screen says, for each device, whether it is
connected now and how: *connected directly*, or *connected through an
encrypted relay* — named that way because "relay" alone sounds like somebody
else holding your files. The address, the path and the transport are under
*Details*.

A connection records whether it went through the relay when it is made, since
afterwards the relay looks like any other address. The daemon publishes the
connections it actually holds, and drops ones that have ended every five
seconds, rather than only at a sync pass.

That second part was found by watching: the first version published the list
only at sync passes, which can be minutes apart, and a device that had been
switched off was still shown *connected directly* two and a half minutes later.
A device that goes away without a word is now noticed by the connection's
thirty-second idle timeout.

**Verified:** in the window, a second device on the same laptop showed as
*connected directly*. The route is asserted in the end-to-end tests for both
the direct and the relayed case. Clearing it is covered by a daemon test
against a real local peer, which fails with the fix removed. The live
check of clearing it in the window did not complete: screen captures of the
window stopped working partway through the session, and it was not repeated
by simulated clicks. *Details* unfolding was fixed after it was seen to fold
itself again on the screen's five-second redraw, and was not seen again
afterwards.

The relay path in the window is untested: on one machine every direct attempt
succeeds, so nothing here falls back to the relay by itself.

## How much space, asked first

**2026-09-27.** Setting up in the window now asks how much of the disk qurb may
use, between choosing the folder and making the key — the brief's order and
[decision 0038](../decisions/0038-the-storage-question-during-setup.md), which
records what was built, the unit it counts in, and how it was checked: unit
tests and the fixture page in a headless browser, not the real window, which
synthetic input cannot reach under Wayland.

**That was not enough, and it cost five days** (found 2026-09-28): *Show a
code* on the Devices screen never worked in the application. The command was
synchronous, so Tauri ran it on the main thread, outside the Tokio runtime,
and opening the pairing host's QUIC endpoint failed with "no async runtime
found". The fixture page answers commands itself and showed a working screen
throughout; the project owner found it by using the window. It is `async` now,
a failure to show a code is shown as an error beside the buttons rather than
as an empty white square, and pairing by the window's code was checked end to
end against a second device.

The window can be driven after all: Tauri lets WebKit's own WebDriver control
its web view when `TAURI_WEBVIEW_AUTOMATION=true`, and GTK's Broadway backend
gives it a display that is on nobody's screen. `scripts/desktop-smoke.sh` does
that: sets a device up through the window, opens every tab, pairs by code, and
sends a file, failing on any command that errs. Its first run found a second,
smaller fault — the page polled the home screen's three commands every 1.5 s
during setting up, each refused because there was no device yet — and both are
fixed. The Broadway display reports a nonsense geometry (a device pixel ratio
of −0.01), so it checks behaviour, not layout; layout is still the fixture's.

The last setting-up screen also stopped saying devices are introduced "on the
command line for now". The window's Devices screen has done that since pairing
by code was built.

## Through a server of your own

**2026-09-27.** The project owner means to run the rendezvous service and the
relay on a server of his own, so that his phone and laptop sync whenever both
are on, wherever they are, without Tailscale. Getting ready for that found:

- **The phone never used a relay** — the app passed none — so on mobile data,
  behind carrier address translation that a direct connection often cannot
  cross, it reached nothing. Settings now has a Relay row, checked as it is
  saved by the engine's own rule.
- **A relay could only be given as an address**, while the server guide's
  example used a name. Names now work, looked up at each start, trying each
  address until one answers.
- **A relay that was down stopped a device syncing at all**, even with the
  other device on the same network. It is best effort now.
- **The guide said the relay was UDP**; it is TCP. And its examples, and the
  CLI's own help, named `~/Downloads/qurb` as the folder, which is now where
  received files go. Both corrected, with the firewall rules spelled out.
- **The rendezvous service held about 150 KiB per connected device**, in the
  WebSocket library's default buffers. Now about 22 KiB, 30 KiB with its own
  TLS. Measured with a load generator in
  [experiments/service-capacity](../../experiments/service-capacity/README.md),
  which also measured the relay: 241 MiB/s for one transfer, 726 MiB/s for
  eight, at 4–5 CPU-seconds per GiB — over loopback on the laptop.

**And the laptop's own idle cost**, measured the next day: an idle daemon with
its phone offline used 0.02 CPU-seconds in three minutes but woke 2.7 times a
second, most of them for a check of the index every five seconds for new
pairings and sends. The window now tells its daemon when it pairs or sends, the
check is a thirty-second backstop, and the connected-devices display refreshes
only while something is connected: 1.1 wakeups a second, 0.01 CPU-seconds.

**No server at all, for now** (2026-09-28): with no card to hand for a cloud
account, the rendezvous service runs on the laptop — only while the laptop is
on, which is the only time the phone could sync with it anyway — as a user
unit (`packaging/qurb-rendezvous.service`), published by Tailscale Funnel at
the address the phone already used, so the phone no longer needs Tailscale.
There is no relay in this arrangement; `qurb netcheck` found the laptop's home
network gives the same public address to whoever asks, so a direct path from
the phone on mobile data should be possible. A Cloudflare quick tunnel carried
an introduction between two devices in 73 ms before being set aside for
Funnel's fixed address.

**Verified the same day, on hardware:** the Galaxy S23 with Wi-Fi off, on its
mobile network, and the laptop on home Wi-Fi, with no relay configured
anywhere. *Sync now* on the phone went through Funnel to the laptop's
rendezvous service, and the laptop recorded reaching the phone at 13:27:51,
13:27:53 and 13:28:39 — directly, since there was nothing else it could have
gone through. What the phone's carrier does to connections is therefore no
obstacle on this pair of networks; another carrier, or another home router,
could differ, and that is what the relay on a server is for.

**Push, the same way** (2026-09-28): `packaging/install-rendezvous.sh` builds
qurb with the push feature, installs it as `qurb-rendezvous` so a plain build
cannot replace it with one that refuses `--push`, and turns push on in the unit
when the Firebase service account is present. The laptop's service started with
it ("waking sleeping devices through Firebase project qurb-f05de").

**Verified on hardware, 2026-09-28:** the Galaxy S23 on mobile data, screen
off, after one *Sync now* to give the restarted service its wake token. A file
written into the laptop's `~/qurb` at 13:40:05; the phone announced itself at
the laptop at 13:40:09, woken by the push, and the laptop connected to it
directly at 13:40:10 (the phone's mobile address, a different port from
earlier); the file was then held on both. About five seconds from a change
on the laptop to a sleeping phone syncing it, with nothing running on the
phone in between.

**Kept across a restart, and a rarer scheduled pass** (2026-09-28, later).
The service now keeps wake tokens in a small file in its state directory,
written when one changes; the server guide explains why that reverses what it
used to say. And with pushes arriving, the phone's scheduled pass drops from
every fifteen minutes to every hour — adaptively: only after a push has
actually woken it within the last seven days. Not measured: the battery this
saves. The claim is a quarter of the scheduled wake-ups, not a number of hours.

One of the phone's passes that afternoon did not connect: at 13:35:47 the
laptop saw the phone arrive and tried to reach it, and no connection followed
in either direction. The daemon logged that failure only at debug level, so why
is not known. The passes before and after it, on the same networks, connected
directly. Since the same day, a device the rendezvous service has just
announced and that then cannot be reached is logged at the default level —
"the device is there and could not be reached" — with how each of its
addresses failed ("106.206.76.231:7773 timed out, …"), so the next one says
why. A mobile network's address mapping is not guaranteed to allow a
direct path every time; the relay on a server is the answer to that, and there
is none in this arrangement yet.

**A server to try it on for free**: Oracle Cloud's Always Free tier, with the
caveats in [the server guide](../../packaging/server/README.md#a-free-server-to-try-it-on).
`packaging/server/deploy.sh user@host` sets one up from the laptop and checks it
from outside — written and not yet run against a real server.

The answer to "is it scalable": for one person's devices a small server is
nowhere near any limit; by memory, it would hold tens of thousands of devices,
and the relay's limit is the server's bandwidth and its bill. **Not yet done**:
any of it on a real server across the internet, a phone on mobile data actually
syncing through the relay, and the churn of many phones connecting and leaving.

## Removing a device

**2026-09-28.** The Devices screen on both platforms, and
`qurb remove-device`, remove a paired device after saying what that does:
trust ends on this device; nothing on the removed device is touched; sends it
never collected are cancelled; what this device keeps for it stays unless the
box is ticked; files already freed because it had a copy are named, with an
offer to fetch them first. Only on this device — the others go on trusting it
until it is removed there too. [Decision 0041](../decisions/0041-removing-a-device.md)
has the reasoning.

Building it found two faults that would have made removal a statement rather
than an action. The daemon updated its list of peers only when a device was
*added*, so a removed one went on being dialled and synced with; and a
connection the removed device already held open was still served, because trust
was checked only at the handshake and an authenticated device missing from the
trust store was deliberately given the shared area. The daemon now drops and
*closes* a removed device's connections, and the listener asks the live trust
list before every request.

How it was checked: storage tests for each consequence
(`crates/storage/tests/removal.rs`, seven), a `qurb-peer` test that an open
connection stops being served, a daemon test that the connection is closed, a
phone FFI test, `qurb remove-device` run against two paired devices, and the
desktop smoke test, which now removes the device it paired through the window —
question, cancelled send and all. The Android screen is built and compiles; it
has **not yet been run on the phone**.

## Recently deleted, and conflicts settled

**2026-09-28.** Two pieces of the brief that turned out to be one problem:
where the bytes of something deleted go.

Under single-copy storage the file in the folder is the only copy of its bytes
on a device, and a deletion arriving from another device unlinked it. So one
deletion removed every copy everywhere, and neither "recover a deleted file"
nor "nothing was lost" when settling a conflict had anything to stand on.
Now a file taken out of the folder by a deletion goes to `trash/` in the store
directory for thirty days — [decision 0042](../decisions/0042-recently-deleted.md) —
and restoring it writes it back as a new version, which reaches every device.
Conflicts are found by name on every device and settled by keeping one version,
the other, or both, with the version not kept going to Recently deleted —
[decision 0043](../decisions/0043-settling-a-conflict.md).

Building the restore found an older fault: a device that had deleted a file
could never take the same bytes back, under any name. Its own tombstone
answered "that content is here" with an empty chunk list, which assembled to
the wrong bytes, and the file failed as corrupt on every sync. Fixed in
`any_file_with_content`; the regression test fails without the fix.

How it was checked: engine tests with two devices for each case
(`crates/engine/tests/recently_deleted.rs`, seven, and `conflicts.rs`, six —
among them a device name `../../evil` given as a label), the fixture page for
the window's new panels in WebKitGTK, and the desktop smoke test for
regressions. Not yet: either on the phone, and a real conflict between the
laptop and the phone.

## A folder shared with chosen devices

**2026-09-28.** Brief §23, with the semantics the project owner chose:
a folder can be on some devices instead of all; the chosen ones sync it both
ways; the others are never sent it; a device left out keeps what it had.
[Decision 0044](../decisions/0044-sharing-with-chosen-devices.md) has the table
of answers the brief asks for.

The rules are small files in `.qurb-sharing/` that sync like anything else, so
no protocol changed; each device derives index tables from them and the three
queries that decide what a peer may see — the tree, a chunk, a content hash —
filter by them. Receiving is filtered too, by the receiver's own rules, and a
change to a rule is taken only from a device the folder is shared with.

Building it found that a device's *own* side of every plan was built with the
same query a peer is served with: once that query left out restricted folders,
a device could not see its own copy and fetched it again on every sync. The
two are separate queries now.

How it was checked: three devices in `crates/engine/tests/sharing.rs`, each
serving the others only what its rules allow — a folder not reaching a device;
the left-out device rewriting the rule file to include itself, refused
everywhere; keeping an old copy and receiving nothing new; changes made in that
old copy going nowhere; sharing with everyone again; chunk and content requests
refused by hash; and the refusals for nesting, nobody, the root, the rules
directory and `../`. Not yet: between real devices.

## A folder kept only remotely

**2026-09-28.** Brief §29 and product plan §4.8. A device can keep a folder
or only list it: files stay listed, a version arriving from another device is
recorded rather than downloaded, and local copies are freed only where another
device has them — the rest stay, named, because freeing the only copy would be
deleting it. [Decision 0045](../decisions/0045-a-folder-kept-remotely.md) says
why this is at the point a version is taken rather than the replica's
`PinSet`, which would have made the folder look deleted.

Checked by two-device engine tests (`crates/engine/tests/availability.rs`,
four): only the copy another device has is freed; a new file is listed and not
downloaded, and downloads when asked for; a file already here stays current;
keeping the folder here again brings everything back and new files arrive
again.

## The passphrase in the window, and a Security section

**2026-09-28.** A key wrapped with a passphrase used to need a terminal to
start: launched from the menu or at login, qurb could not ask, so it did not
sync, and said so on a window nobody had opened. Now the window opens on "qurb
is locked" and asks; at login it shows itself for that. Settings has a
Security section — identity, how the key is kept in plain words, changing it,
pairings and removals — [decision 0046](../decisions/0046-the-window-asks-for-the-passphrase.md),
amending 0033.

Checked in the real window by the desktop smoke test, which now protects the
key with a passphrase from Settings, quits, starts again, has a wrong
passphrase refused and the right one unlock it. The smoke test also runs off
the session bus now, so nothing it does can reach a real keyring.

## Installing, versions, upgrading

**2026-09-28.** Brief §69 and §70, [decision 0047](../decisions/0047-versions-and-upgrades.md):

- `packaging/arch/PKGBUILD` builds a pacman package from this checkout —
  `qurb` (with push), the window, the tray, the menu entry and icon — for
  every user, upgraded by building again and removed with `pacman -R`. Setting
  a device up in the window now arranges to start at login for that person,
  which a system package cannot.
- A release APK is signed with a key generated for the project and kept
  outside the repository.
- Every build says what it is — `qurb 0.1.0 · protocol qurb/2 · index schema
  15` — on the command line and in both apps' Settings.
- The index is copied before a migration runs (`VACUUM INTO`), the way back
  from an upgrade; an index from a newer build is refused rather than guessed
  at. Tests: `crates/storage/tests/upgrading.rs`.
- Each migration and the schema number it sets are now one transaction.
  Apart, a crash between them left a migration applied but unrecorded, and the
  next open re-ran it — which an `ADD COLUMN` cannot survive, so the store
  would never open again. Found when the crash-injection test failed once
  while a package build loaded the machine and the kill landed during the
  first migrations; it passed on the eight runs after, and the fix removes the
  state rather than the timing.

The package was built with `makepkg` on the development laptop (20 MB,
`qurb-0.1.0.r117.3b570dd`) and its `qurb` run from the archive; it has **not
been installed** — the laptop still runs the `install.sh` copies in
`~/.local/bin`, and installing a package over them is the owner's call.

No automatic updater: the decision says what one would need.

## The design, in the window

**2026-09-29.** The window rebuilt to the owner's design direction
([design/direction.md](../design/direction.md), 57 sections, recorded word for
word) and its brief ([design/brief.md](../design/brief.md),
[decision 0048](../decisions/0048-the-design-direction.md)). A plan to design
it in Figma first was dropped by the owner the same day; it was built in the
window itself.

What changed, in short: eight tabs became a translucent sidebar — Home, Files,
Devices, Storage, Private Vault set apart, Settings — over one frosted stage;
Send became Home's primary action and a sheet reachable from anywhere,
including by dropping files on the window; Transfers became a panel that
appears only while something moves or waits; Activity is reached from Home.
The colours are the direction's (Qurb green `#2F6B57` on warm off-white), the
type is Inter and the icons Lucide, both bundled, since the window's CSP loads
nothing from outside. Files became a file browser from the index, with each
file's state in the direction's words — *On this device*, *Available
elsewhere*, *Only copy here* — and a details panel that names the devices
holding it. Storage leads with what can be freed without losing anything.

New in the engine and the window for it: `Db::holders_of_content` and
`Db::freeable` (tests in `crates/storage/tests/storage_cap.rs`); commands to
browse by folder, show a file's details, free a file's local space, delete to
Recently deleted, open a file or its folder; a `notifications` setting the
notifier obeys; and the daemon now picks up *Keep new files private* while it
runs, as it already did the limit and the downloads location.

How it was checked:

- **Every place rendered** against the fixture data in WebKitGTK — the
  window's own engine — offscreen, in each of Home's states, with sheets,
  panels and menus open. That renderer runs without compositing, so it draws
  no backdrop blur; sheets and panels were made nearly opaque because of it,
  since some Linux setups run the real window the same way.
- **The real window driven end to end** by `scripts/desktop-smoke.sh`, updated
  for the new structure: set up, every place, a device added by the code the
  window shows, a file sent and seen under Transfers, the device removed after
  the question, the key protected with a passphrase and unlocked on the next
  start. No command failed.
- `cargo test` and clippy; the wiring test now reads every script the page
  loads, and caught the switches in Settings passing their command name
  through a variable, where it could not see them.

Not done: the owner has not looked at it yet; nothing was checked with
compositing on, so the blur and the running animations have not been seen;
dark mode; moving a file into or out of Private Vault; the Android app. On
2026-09-29 the laptop was on a network that does not carry multicast, and the
seven tests that find devices on the local network failed there — four in
`crates/peer/tests/beacons.rs`, three in `end_to_end.rs` — code unchanged
since they passed the day before at home. The other 746 passed.

## A folder typed without a slash

**Found 2026-10-03**, on the designed Android app's first run on the S23: the
phone could not reach the laptop, and the laptop had last reached the phone
four days before.

On 2026-09-29 a folder was set up through the window as `home/project/qurb`,
with no leading `/` or `~`. The window expanded only a leading `~/`, so the
relative path went to setup unchanged: a new device, with a new key, was made
at `~/home/project/qurb` — relative to the window's working directory, the
home folder — and the folder list recorded `home/project/qurb` as typed, at
the top. From the next login the desktop opened that empty, unpaired device
instead of `~/qurb`, the folder paired with the phone, and nothing synced from
2026-09-30 to 2026-10-03. `qurb status` read the same relative line from a
terminal in the repository, found nothing there, and fell back to `~/qurb`, so
the command line and the window named different folders and the command line
looked healthy. The window said nothing a person would read as wrong: the new
device's Home said *Add your first device*, and the window was hidden.

Fixed in three places:

- **The window reads a typed folder from the home folder** — `qurb`,
  `Documents/qurb`, `~/qurb` — and never leaves it relative; empty text is no
  folder rather than the home folder. Under the field it now shows the full
  path whenever that differs from what was typed. Test:
  `a_typed_folder_is_found_from_the_home_folder`.
- **The folder list stores only absolute paths**, and reads an older relative
  entry from the home folder, where the desktop has been opening it, so the
  window and the command line name the same folder.
- **On the laptop**, with the owner's agreement: the empty device at
  `~/home/project/qurb` deleted (no files, a key that had never been paired),
  the list reduced to `~/qurb`, and the desktop restarted on it.

Found with it: `qurb config` with no arguments panicked, and `config`,
`protect` and `join` took their first word as the folder, so each worked only
with a path although `qurb` says the folder may be left out. All six commands
that take an optional folder now read it one way — the first word is the
folder only if it has qurb in it. `crates/qurb/tests/arguments.rs` runs the
real binary in a home of its own; both tests failed before the change, one at
the panic.

**Not all of them, it turned out (2026-10-05).** Seven more commands read the
folder through a second function, `split_path`: `deleted`, `restore`, `free`,
`activity`, `ls`, `find`, and the new `forget`. It also took a lone word for
the folder, so `qurb find holiday`, `qurb ls docs` and `qurb restore #1` each
said the word was "not set up yet", and worked only with a path in front. It
was found by writing a test for `qurb forget`, and seen with the installed
binary. `split_path` now uses the same rule. Test:
`a_lone_word_is_not_taken_for_the_folder`.

What this does not fix: a second device made by mistake is still a second
device, and nothing tells a person their paired folder has stopped running.
A desktop whose Home says *Add your first device* while another folder on the
same computer is paired with a phone is a state worth noticing, and nothing
notices it yet.

## A folder deleted while qurb ran

**Found 2026-10-05.** At 23:25 on 2026-10-03, `~/qurb` was moved to the
desktop's Trash — by the file manager; qurb never uses that Trash, its own
Recently deleted is inside `.qurb/trash` — while the desktop app was running.
Nothing said so. A restarted desktop found no folder set up, which is how it
came to light.

What the running app did in the meantime:

- **The daemon carried on**, on the store inside the Trash, through the files
  it already had open: it synced with the phone, began pulling the 800 MB video
  ([phase 5](phase-5-mobile.md#an-800-mb-video-and-what-stopped-it)), and
  wrote its index there.
- **The window's commands made a second device.** Removing the phone and
  showing a pairing code open the folder by its path; finding none, they made
  one — `~/qurb/.qurb` with a new identity and an empty index — and the phone
  paired with that new identity at 23:28, while the daemon, under the old one,
  went on syncing with it.

Restored on 2026-10-05 with the owner's agreement: the half-made folder moved
aside to `~/qurb-made-after-deletion`, kept; the original moved back from the
Trash. Identity `410cac55`, the phone paired, the files and history intact;
the desktop restarted on it. The phone listed a second "saqib", the
half-made identity `043ebd1d`. It was removed from the phone's Devices with
*Remove this device* at 07:23 UTC on 2026-10-05. The phone's history records
the removal, and its trust list now holds only `410cac55`.

Two faults, fixed the same day:

- **The daemon did not notice its folder had gone.** It now notes its store's
  device and inode when it starts — a moved folder keeps its inode and loses
  its name — and checks them before applying any change the watcher reports
  and on its half-minute and two-minute timers. A folder that is not at its
  name any more, moved or replaced, stops the daemon with *"… was moved or
  deleted while qurb was syncing it, so qurb has stopped. Put the folder back,
  or set qurb up again."*, which the window shows on Home. Checked before
  changes are applied so that a folder moving can never be taken for its files
  being deleted. Test: `a_folder_moved_away_or_replaced_is_not_the_same_folder`;
  and run for real — a throwaway device's folder moved while `qurb run` synced
  it: the daemon stopped within a second, saying so, and nothing was made
  where the folder had been.
- **Opening a store created one.** The window's pairing and its Security
  section opened the folder by path and called `Identity::load_or_create`.
  Pairing now refuses a folder that is not set up, and both read the identity
  with `Identity::load`, which never makes one. Making an identity is setup's
  job alone.

## Notifications stopped at the first failure

**Found 2026-10-05**, while measuring large transfers from the phone
([phase 5](phase-5-mobile.md#an-800-mb-video-and-what-stopped-it)). Each time
a fetch failed, the desktop's log showed, within a second:

> thread 'tokio-rt-worker' panicked at … tokio-1.53.1/src/runtime/scheduler/multi_thread/mod.rs:91:9:
> Cannot start a runtime from within a runtime.

Five times since the log began on 2026-09-28: once in each run of the app
that had something to announce, on 2026-09-28, on 2026-10-03, and three times
on 2026-10-05. So the owner has probably never seen a desktop notification
from qurb. The panic was in the
notification watcher (`crates/desktop/src/notify.rs`), not the transfer. A
failure is one of the three things worth a notification. Raising one is a
blocking D-Bus call, made from the watcher, which is a Tokio task. `qurb-tray`
depends on zbus with its `tokio` feature, and Cargo merges features across
everything built in one command. So the desktop, built alongside the tray as
the README and the Arch package both do, got a zbus whose every blocking call
starts a Tokio runtime and blocks on it, which panics on a Tokio thread. The
panic killed the watcher, and the desktop raised no notification of any kind
until it was restarted. Syncing was unaffected. The tray has had that
dependency since 2026-09-18 and the desktop its notifications since
2026-09-24, so any desktop built alongside the tray since then could have done
this. Nobody noticed, because a notification that does not appear looks the
same as one that was never due.

Two fixes, either enough:

- **The watcher raises each notification on a blocking thread**
  (`spawn_blocking`), where starting a runtime is allowed. This holds whatever
  features zbus is built with. Test:
  `a_notification_is_raised_where_blocking_is_allowed`, which does what zbus
  does from inside a Tokio task. With the call made inline, as before, it fails
  with the same message at the same line of Tokio.
- **The tray uses zbus's own runtime** (`async-io`), so no crate in the
  workspace turns zbus's `tokio` feature on (`cargo tree --workspace -e
  features -i zbus`).

Not checked: whether a notification now appears on the owner's desktop when a
transfer fails. Run since the fix, the desktop has not had one to raise.

## Setting a computer up without the 24 words

**2026-10-05**, by [decision 0052](../decisions/0052-the-key-travels-with-the-code.md).
The window's setup no longer shows the words or asks for three back: *Set up
Qurb here* makes the key and starts. *I already use Qurb* takes the code a
phone shows (*Show a code on this phone*), and `join_new_device` fetches the
key with it and pairs; the 24 words are the fallback. Pairing from Devices
gives this computer's key to a device with none, once per code. On the command
line, `qurb init` no longer prints the words, and `qurb join [dir] <code>` sets
up a folder that is not set up yet. Test:
`a_new_folder_joins_with_a_code_and_takes_the_key`, the real binary on both
sides.

That test found that `qurb join` exiting straight after pairing left `qurb
pair` waiting 30 seconds for a close that had not been sent. The joining side
now waits up to two seconds for its close to leave. A refusal had the same
fault in reverse: the device showing the code dropped the connection under its
"no", and the asking device saw *connection lost*.

## Pairing approved by number

**2026-10-07**, [decision 0053](../decisions/0053-approval-same-key-and-safe-copies.md).
The window's *Show a code* sheet now shows a device asking to join, with the
six digits it should be showing and *Approve* / *Decline*. *Enter a code*, and
setup's *I already use Qurb*, show this computer's digits while the other
device decides. `qurb pair` asks in the terminal and treats anything but `y` as
no, so an unattended one lets nobody in. Settings says where the key is safe,
and warns when no other device holds it. The daemon's trust list re-reads the
store when an unknown device connects, so a device paired from a terminal is
accepted on its first try.

Looked at the same day against the fixtures, which now ask to join after four
seconds (`experiments/desktop-fixtures`), driven in headless Chromium over the
DevTools protocol. *Show a code*; *Pixel 8 wants to pair*, the number in
large type, *Decline* and *Approve*. *Decline* went back to the countdown
with the question gone, and *Approve* to *Device added*. The key row read
"On this computer and on Galaxy S23 …". A real pairing through the window,
with a second device, is not done.

**A file on no device** ([decision 0055](../decisions/0055-a-file-on-no-device-says-so.md)),
in the same fixtures: its row reads *On no device*, its menu offers *Details*
and *Delete* only, and its details panel says why, with "On: No device".
That showed a fault older than it: a badge in the *attention* tone (*Only
copy here*, and now *On no device*) took the attention card's grid layout,
whose class name it shares, and stretched across the panel with its words
pushed to the middle. `.badge.attention` now restates a badge's own shape.

**And in the real window**, the next day (2026-10-08), with
`scripts/desktop-smoke.sh`. The smoke test still set a device up with 24 words
and paired without a question, so it had stopped matching the window after
decisions 0052 and 0053. It now sets up with nothing to write down. A second
device, the command line in a folder not set up, joins with the window's code
and takes its key. The window asks, and is approved once the number it shows
matches the one `qurb join` printed. The run passed whole: setup, every place,
pairing, a send, removing the device, and the passphrase. It passed again with
40 and with 90 seconds of waiting before *Approve*, and `qurb pair` against
`qurb join` took 90 seconds the same way.

Its first attempt, which never approved, found something worth fixing. When
the code ran out, the device asking was told only "connection lost: closed by
peer". The device showing the code stops waiting at expiry and closes without
an answer. A device whose wait ends that way after the code's time is now
told *the code expired before the other device approved this one*
(`Error::NotApprovedInTime`; test
`a_device_not_approved_before_the_code_expires_is_told_so`).

The other way round as well, with `SMOKE_MODE=join`: the window set up from
nothing by *I already use Qurb* and a code that `qurb pair` showed. The
window showed the number to approve at, `qurb pair` asked about the same
number (199 134), a yes gave the window its key, and every place opened. The
full run passed again after the change.

## Still to do

- **Running the *daemon* as a service** — a user unit, a launch agent, a
  Windows service. The two *server* services have units. On the laptop, the
  window now keeps syncing when closed and starts hidden at login
  ([decision 0040](../decisions/0040-the-menu-opens-the-window.md#closing-is-not-quitting)),
  which covers what a service was wanted for; a daemon with no window at all is
  still `qurb run`.
- **An automatic updater**, deliberately (decision 0047). Installing is a
  pacman package on Arch, `packaging/install.sh` elsewhere; no `.deb`,
  Flatpak or AppImage yet.
- **A replica that can free space.** It keeps every payload, because with no
  folder there is nowhere else for the bytes to live — and eviction works by
  deleting a file from a folder, so a cap on a replica reports the overrun
  rather than acting on it. Dropping chunk payloads is a different operation
  and is not written.
- **The owner's look at the window**, then dark mode and moving files into
  and out of Private Vault. The Android app got the same design on 2026-09-29
  ([phase 5](phase-5-mobile.md#the-designed-app)); after the review, the relay
  on a server of the owner's own, then a formal release. See
  [product-plan.md](../product-plan.md).
- ~~**The mark in the applications menu.**~~ Done 2026-10-03:
  `packaging/qurb.svg`, `crates/desktop/icons/icon.png` and the tray icon are
  the mark, the tray's drawn from the same coordinates
  ([0048](../decisions/0048-the-design-direction.md#progress)).
