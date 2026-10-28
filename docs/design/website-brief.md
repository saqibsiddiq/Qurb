# The website brief

What the website at qurb.cloud is for and how it should feel, agreed with the
owner in conversation on 2026-10-27, before any of it is drawn. It replaces
the landing page on `main` (2026-10-03) and the one waiting on the
`website-redesign` branch (2026-10-11) as the reference for the next design,
which is made in Figma first — the file *QurbSpace — website*,
<https://www.figma.com/design/9DQiFyvLWy0BlVtllA4pIk> — and built only once
the owner has approved it there.

**Status: the home page of qurb.cloud since 2026-10-27,** built scene by
scene with the owner on the branch `website-qurbspace` (at `/preview` while
it was designed) and merged to `main` at his word. Three hero mockups
were drawn in Figma the same day, before this conversation, and the owner
turned all three down; they stay on the file's first page as what not to do.
Figma's free plan then ran out of tool calls after Scene 1's still, and the
owner chose to design the rest as a live page in the browser instead, where
the motion can be judged as motion.

## Who it is for

**Ordinary people, not technical people.** The page says what QurbSpace does
for someone, never how: no *encrypted*, *peer-to-peer*, *sync*, *relay*,
*chunks* or *index* in the main story. What is technical lives one click
away (Docs, GitHub, Security), not on the landing page.

## The name

**QurbSpace**, as the app now calls itself; the address stays qurb.cloud.
*Qurb* (قرب) means **nearness** — confirmed by the owner — and that is the
page's idea. In his words: the files you have stored on your desktop are
"symbolically near you, all the time".

## The hero: remote access

**Opening your computer's files from your phone, wherever you are**, is the
heart of the page — chosen by the owner over sending. It is nearness made
literal: the computer stays at home, its files come with you. Sending, the
Private vault and having nobody in the middle follow it, as smaller moments.

## How it should feel

- **Easy, never overwhelming.** One thing to look at per screen. The
  rejected mockups had too much competing at once — badges, chips, cards,
  stats — and that is the failure to avoid.
- **Visual, not verbal.** As in the app, animation and pictures carry the
  meaning and the words are few: a short line per idea, not paragraphs.
- **Fun moments inside a calm, luxurious frame.** The frame — space, type,
  pace — is quiet and premium; inside it, motion is allowed to delight and
  surprise. Where the two pull apart, the frame wins and the fun stays in its
  moment.
- **Light, with dark rooms.** One light look, no dark mode; since
  2026-10-27, at the owner's word, a few scenes are deep green "night
  rooms" inside it (see *Richer*, below).
- **References:** Google's Antigravity site — one idea to a screen, very
  large confident moments, motion that shows — and luxury brand pages, with
  some fun designs alongside.

## What the page ends in

Three actions:

1. **Download, for the visitor's own device.** Android and Linux download
   once 0.1.0 is published, which the owner has agreed to; Windows is built
   but has never been run on Windows, and says so; macOS and iPhone have no
   app and get the waitlist instead.
2. **Join the waitlist** — an email address, kept by a small service added
   for it. The site asks for and stores nothing today, so this changes what
   the privacy page says, and the page must change with it.
3. **GitHub** — the code.

## The six scenes

Six scenes, as proposed in the conversation and kept by the owner:

1. *Your files. Always near.* — the phrase and one living animation: a
   laptop at home, you somewhere else, the distance closing.
2. Away from home, the phone opens the computer's folders as if you were at
   it — the hero.
3. Send in one tap — a photo leaves the phone and lands on the laptop.
4. Only yours — straight from one device to the other, the cloud in the
   middle simply absent.
5. The Private vault — a lock, a fingerprint.
6. Get it — the device-aware button, the waitlist, GitHub.

## Anywhere, labelled as coming

From another network, the phone reaches the computer through the rendezvous
service, and through the relay when no direct path exists. Both have worked
for the owner's own devices ([features.md](../features.md): syncing from
mobile data ✅; the relay 🧪, running on the owner's laptop since
2026-10-15). But someone who downloads QurbSpace has no rendezvous or relay
of their own, so for them remote access works on the same Wi-Fi only, until
both run on a server anyone can reach — roadmap item 2.

Asked how the hero should handle this, the owner chose to **promise
"anywhere" and label it as coming** until that server is up — over promising
only the home, or putting the server before the site. The label comes off
the day a stranger's phone can reach their own computer from another
network, and not before.

## As built

Each scene is a component in `website/components/`, drawn from the app's
screens, simplified: its green, its circle, its sheets and round buttons. The
six scenes above grew to eight as the owner added the walkthrough, backups,
and several people and computers. He asked that they run "in a proper
sequence so that the user doesn't get confused" and that "after scrolling,
the user must have a pretty solid understanding of the product": they go
from what it is, to starting, to daily use, to keeping things safe, to who it
is for, to why it can be trusted. Each scene had a numbered label for a day,
until he asked for the numbering to go. Each plays once when it comes into
view, and plays again when clicked once it has finished (a replay button was
tried and taken out at his asking):

- **Hero** (`always-near.tsx`): *Your files. Always near.* — the laptop at
  home, the phone leaving, the folders following like ducklings. Plays once
  on load, about four seconds.
- **The walkthrough, three scenes** (`walkthrough.tsx`, one component with
  a `chapter`): *Scan once. That's it.* (Join, the scanner finding the code,
  the same number on both, Approve), *Your computer, in your pocket.* (the
  circle, Shared or Private, the computer's folders on the phone; *From
  anywhere · coming soon*), *Change it from your phone.* (rename and delete
  from the phone, the laptop following). It was first one section pinned
  while the visitor scrolled; the owner found that "too odd compared to the
  rest of the website", so each chapter now plays by itself like the others.
- **Send** (`send-plane.tsx`): *From your hand to your desk. And back
  again.* A photo folds into a dart and flies to the laptop; a PDF folds into
  a different plane, a glider, and loops back to the phone. The title the
  owner chose over "Send in one tap", which was three taps.
- **Backups** (`backups.tsx`): *Pick what to keep safe.* WhatsApp,
  Camera and Documents ticked in the folder tree, carried to the computer,
  new photos following on their own, and WhatsApp restored to a phone from
  the computer.
- **Private vault** (`vault.tsx`): *A folder only your phone can open*
  — the laptop asks, the phone confirms behind the fingerprint, the folder
  opens and locks itself after ten minutes unused. On the owner's own
  computer this is QurbSpace's rule, not encryption, so the words say it
  opens only when you say yes.
- **Family & computers** (`family.tsx`): one computer with a private
  folder for each person, locked to their own phone, and a Shared folder for
  all; then one phone holding the circle to switch between a home laptop and
  an office desktop. *Being tested* on the half with several people, since
  guests are checked only on the emulator; and "locked to their own phone"
  rather than "encrypted", which only a guest's folder is.
- **Nobody in the middle** (`no-middle.tsx`): the usual way, up into a
  company's cloud and down, drawn and faded away; the file goes straight
  across, locked on the way.
- **Get it** (`get-it.tsx`): *Get QurbSpace.*, the device-aware button, the
  waitlist and GitHub
  ([decision 0075](../decisions/0075-the-website-keeps-a-waitlist.md)) —
  plain, without the hero's drawing or words, which the owner found
  repeated.

*Your computer, in your pocket.* also plays a video from the laptop on the
phone while it is still arriving, then opens a photo and a PDF, at the
owner's asking — true as features.md has it (a 1.6 GB episode playing six
seconds after *Open* on the S23). The video is real, as he asked: *Ocean
surface waves 06* by Mostafameraji, CC0, from Wikimedia Commons, cut to
480 pixels and no sound in `website/public/media/` (67 KB as MP4, 125 KB as
WebM), served from the site like everything else on it.

## The docs

At the owner's asking, *Privacy* in the top bar became **Docs**: separate
pages with a sidebar and a search (his choice over one long page), in
`website/app/docs/`, written in `content.tsx`, with no animations. Two kinds
of page, for two readers he named:

- **How-to**, for the landing page's ordinary people: *Getting started*
  (install on the computer, on the phone, connect), *Everyday use* (open the
  computer from the phone, change its files, send, backups, the private
  vault, family, several computers, your key and a new phone), *When
  something's wrong*.
- **Technical**, for the privacy-minded and curious, not repeating the
  landing page: *How it works* (the big picture, how files sync), *Privacy &
  encryption* (keys, what is encrypted and when, a private folder of one's
  own and a guest's, who can learn what, what this website keeps, what it
  doesn't protect against yet), *Networking* (finding each other on one
  network, the rendezvous service, the relay, how pairing works). Each ends
  in the design records it rests on, and says what the security page and
  those records say, no more.

The owner asked for no privacy link in the footer; what the site keeps — the
waitlist's addresses and nothing else — is said on *What this website keeps*.

Laid out after the documentation sites he pointed to, looked at on
2026-10-27 (Stripe's, Vercel's, Tailwind's): their own top bar, with a
search that opens with Ctrl K or ⌘K and finds a page as one types; a
sidebar that stays put, the open page marked by a green edge; a breadcrumb,
a large title and a one-line lead; steps as numbered badges on a thread;
callouts with a coloured edge; *On this page* at the right on wide screens,
marking the heading being read; Previous and Next as cards. On a phone the
sidebar slides in from a menu button. **No dashes in the words**, at his
asking: the pages say it with a colon, a comma or a full stop instead.

## Richer

After looking at Antigravity's site again the owner found ours "fails to
look and feel as rich". What made theirs rich, looked at the same day: the
page itself alive (a particle field that answers the pointer), depth and
rhythm (light and dark sections, things moving at different depths, smooth
scrolling), real media, things answering a hover, a grand close. He chose
five changes, each made from QurbSpace's own idea, nearness:

1. **A living field** (`near-field.tsx`): tiny green dots drifting behind
   the whole page on one canvas, gathering near the pointer or the finger.
2. **Smooth, deep scrolling** (`depth.tsx`, `rise.tsx`): the wheel glides
   to a stop, by Lenis (MIT), which tells a trackpad from a wheel — a first
   version of our own smoothed both alike, and the owner could not scroll
   smoothly with it; checked by sending real wheel input to a headless
   Chromium (three notches: 461 to 900 pixels, easing out over a second); headlines rise word by word; the words under them and each
   scene ease in; a scene's devices drift against the scroll, a little
   deeper than its words.
3. **Rooms**: *A folder only your phone can open* and *Nobody in the
   middle* are deep green night rooms, with a faint drifting starfield;
   *Backups* and *Family* put their headline beside their words. Variety,
   not one shape repeated.
4. **Richer devices**: bezels with light on their edge, glass across the
   screens, deeper shadows, a slight lean toward the pointer, and the ocean
   clip's own frame wherever a photo is shown (*Sunset.jpg*, which was a
   gradient named *Lake at dawn*).
5. **A grand ending**: *QurbSpace* across the foot of the page, which the
   field's dots fly in to and settle into as it arrives.

None of it moves with reduced motion asked for: the dots hold still, the
name is simply there, nothing glides. The touch screen, the keyboard and
the scrollbar scroll as they always do; only the wheel is smoothed.

## On a phone, and every screen

The owner found the phone site "ugly and clunky" (2026-10-28), and asked
for four things: no dots chasing a finger, a menu icon at the top right, the
animations redone for the phone, and a site that looks good at every size.

- **Phone first.** Up to 900 pixels wide every scene is drawn tall, the
  phone at full size and the computer small above it (`Tall` in
  `play-on-view.ts`, `tall.ts`), so what the phone shows can be read on the
  phone it is shown on; what flies between them bends and shrinks with them.
  On a tablet the tall drawing grows to fill the width, up to 1.6 times. The
  hero has a tall drawing of its own: the laptop up at the left, the phone
  down at the right, the folders trailing down to it.
- **The dots** answer only a mouse, and there are fewer of them on a phone.
- **A menu** behind an icon at the top right, opening the page's links full
  screen (`site-menu.tsx`).
- **Every size**: headlines balanced; the closing name sized to the width;
  small phones (320) given smaller headlines and gutters; on screens from
  1800 pixels the page grows with the screen (CSS `zoom`), so a 2560 monitor
  is filled as a laptop is. Checked in still screenshots at 320, 390, 768,
  1024, 1440, 1920 and 2560.

## Still open

- **Phones.** Every scene scales down to fit a phone's width, which leaves
  the devices too small to read in every scene with a phone and a laptop; each
  needs a layout of its own for narrow screens.
- **The other October pages** — security, reporting a vulnerability,
  licences, cookies, terms, status, notes, contact, download — are still in
  the October design and still say *Qurb*; the privacy page now sends its
  visitors to `/docs/this-website`.
- **The image a shared link shows** (`app/opengraph-image.png`) is still the
  October one, *Qurb*.
- **The docs' words** follow the app's: *I already use Qurb* is quoted as
  the Android welcome still says it, though Home says *QurbSpace*.
- **The downloads say *Coming soon*** until 0.1.0 is published on GitHub.
- **The other pages** (privacy, security, docs, licences…) are still in the
  October design and still say *Qurb*.
- **Every claim against [features.md](../features.md)**, as the site's
  README requires, read through once more before it goes to `main`.
- **Richer, felt rather than seen.** The field, the glide, the tilt and the
  settling were checked in still screenshots and by reading the canvas, not
  by eye in motion; how they feel, and whether the field costs a slow
  machine too much, is not yet known.
- **Checked only in still screenshots** in Chromium, held at chosen moments
  (`?connect=`, `?remote=`, `?change=`, `?sp=`, `?bk=`, `?vt=`, `?fm=`, `?nm=`,
  `?gi=` in development);
  how the motion feels is the owner's to judge, and Firefox and Safari are
  not looked at.
