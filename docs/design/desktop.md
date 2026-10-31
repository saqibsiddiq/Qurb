# The desktop window, redesigned

The desktop window's redesign, agreed with the owner one component at a time
from 2026-10-31, before each is built. The direction he chose on 2026-10-28:
**the Android app's structure with the website's polish**, a sidebar, dark
mode kept. A first full Home (a greeting, six sections in cards, a starfield,
a glow) was built straight after that and turned down: the owner wants the
minimal language of the phone and the website, and "we need to discuss
every component before designing".

The language, as the phone and the website have it: one thing at a time, a
great deal of room, as few words as possible and no "·" separators, the deep
green the only colour against warm off-white (`#FAF9F6`) or its dark twin;
and the same few parts everywhere — the status mark, the device circle,
folders two to a row, plain file rows, a sheet of round buttons.

It was built in `crates/desktop/ui-next/` beside the window it replaced, on
the same engine commands (`src/commands.rs`), and became `ui/` on
2026-10-31 at the owner's word. It is looked at with made-up answers in
`experiments/desktop-fixtures/index.html` (`?state=synced`,
`syncing`, `away`, `alone`, `attention`). When every place is agreed and
built, it replaces `ui/`.

## Agreed

1. **The window.** A sidebar of icons with their words — **Home, Files,
   Settings**, three places as on the phone — under *QurbSpace*, the name's
   only place. Devices and the people who use this computer are reached
   through Home's circle, as on the phone. No cards and no panels: the
   chosen place in green. Light and dark, following the system.
2. **Home's status mark.** The phone's mark, large (180 px) in Home's middle,
   with its states — synced, syncing (a third of the ring travelling),
   receiving or sending (the ring as far as it has gone), a device off,
   devices away, something needing attention, *Add a device* — and one line
   of words under it. Clicking it syncs; there is no Sync button. Home holds
   the mark, the circle and Send, and Waiting only while something is queued.
3. **The device circle.** The chosen device at Home's top right: its picture
   or first letter, a blue dot online, a red one off. One click drops a
   panel from it: one's own devices as rows, the chosen one ticked (a click
   chooses), each with an arrow to its page; *People who use this computer*;
   *Add a device*. A device's page replaces Home, with a way back: its circle
   and name, whether it is on, what it backs up here, and *Remove* in red.
   The chosen device is where Send sends.
4. **Send.** One wide green button under the mark, saying *Send*: the
   system's chooser, for files (folders come by dragging). Files dragged
   anywhere over the window, in any place, go to the chosen device; while
   they are held over it the window says *Drop to send to …*. Afterwards the
   mark's line says *Sending …* a moment, and Waiting shows the send until it
   is collected. A file sent there before is asked about first, as today.
5. **Waiting.** Only sends not yet collected and transfers in progress —
   files kept only on this computer belong with storage, in Settings. Under
   Send, only while something is queued: *Waiting* with a count, up to three
   rows (a tile, the name, a thin bar or *Ready for …*, × to stop a send,
   which stops it without asking), then *N more*.

## Built ahead of agreement, to review

On 2026-10-31 the owner asked for every other place to be built at once —
"complete the entire app, keeping all the functionalities in mind" — and
to suggest changes afterwards. So 6 to 11 below were not agreed one by one
like 1 to 5: each is a proposal, built, in the same language, waiting for
his review.

6. **Files.** A search across the shared folder and the Vault at the top;
   then *This computer* — Shared, Vault, Received, Recently deleted — as
   cards two to a row, and *People*, a card for each person's folder,
   locked or open. Inside: folders as cards, files as plain rows, an icon at
   a row's end only when its bytes are not simply here (on other devices,
   downloading, only here, on no device). A folder mostly of pictures is a
   gallery. A top folder's options are a right click away.
7. **A file's sheet.** As the phone's: its tile, its name, chips for where it
   is and its size, round buttons four to a row — Preview, Open, Show, Keep
   here, Send, Free up, To Vault or To Shared, Delete in red — and the
   devices that have it. Send sends to the chosen device, as Home's does.
8. **People.** Guests in the circle's panel, each with a page; their folder
   is a card in Files, sealed until they say yes on their phone.
9. **Add a device.** Three round choices — Show a code, Enter a code, Add a
   person; the code as a QR with six digits and a countdown; a device asking
   shows the number to compare, Decline and Approve; the new device arrives
   as its circle.
10. **Settings.** The phone's groups — This computer, Files, App, Connection
    — of rows with a value, a switch or an arrow; this computer's circle and
    name at the top; Storage, Key, Activity and Advanced as pages; Quit in
    red at the end.
11. **The new features.** The gallery; quick preview (Space, arrows,
    Escape) of pictures, text, video, songs and PDFs; search over the shared
    folder, the Vault and every open person's folder; a **speed limit** in
    Settings, Connection — chips for each way, None to 25 MB/s; and **Back
    up folders** in Settings, This computer — the folders, *Add a folder*,
    and *Kept on*, a switch per other computer
    ([0076](../decisions/0076-a-computer-backs-up-its-folders-and-holds-back-its-speed.md)).
    Storage gained *Only on this computer*, where component 5 put it.
12. **Two versions on Home.** When a file has two versions the mark says so
    in amber, and clicking it opens them instead of syncing; *Something needs
    attention* opens Activity. This extends component 2.

Still to agree: 13, the tray and notifications, which are unchanged.

## Built so far

Every place, on 2026-10-31, looked at with the fixtures in light and dark;
then swapped in as `ui/` at the owner's word, before his review of 6 to 12.
`scripts/desktop-smoke.sh`, rewritten for it, drives the real window
through all of it — set up, every place, pairing by six digits, a send,
removing a device, a passphrase and unlocking; joining by code; following
the desktop's dark preference — with no command failing (2026-10-31, on
this laptop, Broadway display). The four features left then were built the
same day and added to that run: a speed limit set and read back, a folder
backed up and kept by the paired device, people's folders searched, a video
played in quick preview from its local address. **A PDF in quick preview
has not been seen** in the real window: WebKitGTK 2.54 carries a PDF
viewer, and whether it shows one inside the window's frame is untested.
