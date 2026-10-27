import type { ReactNode } from 'react'
import Link from 'next/link'
import { Cmd, Note, Steps } from '../../components/doc-parts'
import { repoDoc } from '../site-config'

// The docs: how-to pages for the people the landing page is for, and
// technical pages for the privacy-minded and curious, each linking to its
// design record. What each page says is what docs/features.md says the apps
// do, at the level it is checked there; where something is not out yet, or
// only tested on an emulator, the page says so.

export type Doc = { slug: string; title: string; summary: string; body: ReactNode }
export type Section = { name: string; docs: Doc[] }

export const SECTIONS: Section[] = [
  {
    name: 'Getting started',
    docs: [
      {
        slug: 'install-on-your-computer',
        title: 'Install on your computer',
        summary: 'QurbSpace for Linux today, Windows early, and Mac later.',
        body: (
          <>
            <Note tone="soon">
              QurbSpace 0.1.0 isn’t published yet. The day it is, the downloads appear on the <Link href="/#get">Get QurbSpace</Link> part of the home page.
            </Note>
            <h2>Pick the one for your computer</h2>
            <ul className="doc-list">
              <li>
                <b>Ubuntu, Debian, Mint, Pop!_OS:</b> the <em>.deb</em> package. Open it and choose Install, or in a terminal: <Cmd>sudo apt install ./qurb_0.1.0_amd64.deb</Cmd>
              </li>
              <li>
                <b>Fedora, openSUSE:</b> the <em>.rpm</em> package. <Cmd>sudo dnf install ./qurb-0.1.0-1.x86_64.rpm</Cmd>
              </li>
              <li>
                <b>Arch, CachyOS, EndeavourOS, Manjaro:</b> the Arch package. <Cmd>sudo pacman -U qurb-0.1.0-1-x86_64.pkg.tar.zst</Cmd>
              </li>
              <li>
                <b>Windows 10 and 11:</b> the zip. It is built, but nobody has run it on Windows yet, so treat it as early.
              </li>
              <li>
                <b>Mac:</b> not yet. Join the waitlist and we’ll tell you when it is.
              </li>
            </ul>
            <h2>Then</h2>
            <Steps>
              <li>Open QurbSpace from your apps.</li>
              <li>It makes a folder for your files and sits quietly in the background.</li>
              <li>Next, put it on your phone.</li>
            </Steps>
          </>
        )
      },
      {
        slug: 'install-on-your-phone',
        title: 'Install on your phone',
        summary: 'Any Android phone from Android 8 on. iPhone is on the waitlist.',
        body: (
          <>
            <Note tone="soon">The Android app is published with 0.1.0, which isn’t out yet.</Note>
            <Steps>
              <li>On your phone, download the QurbSpace app (the file ending in <em>.apk</em>).</li>
              <li>Open it. Android asks whether your browser may install apps: allow it, just this once.</li>
              <li>Open QurbSpace and choose <b>Start here</b>.</li>
            </Steps>
            <p>There is no account to make, and no password. Your phone makes its own key, and keeps it safe for you. See <Link href="/docs/your-key">Your key, and a new phone</Link>.</p>
          </>
        )
      },
      {
        slug: 'connect',
        title: 'Connect your phone and computer',
        summary: 'One scan, once. They recognise each other from then on.',
        body: (
          <>
            <Steps>
              <li>On your computer, open QurbSpace and choose <b>Add a device</b>. It shows a QR code and six digits.</li>
              <li>On your phone, choose <b>Scan a code</b> and point it at the screen. On the same WiFi you can type the six digits instead.</li>
              <li>Both screens show the same number. Check it matches, then choose <b>Approve</b> on the computer.</li>
            </Steps>
            <p>That’s all. You never do it again for these two devices.</p>
          </>
        )
      }
    ]
  },
  {
    name: 'Everyday use',
    docs: [
      {
        slug: 'open-your-computer',
        title: 'Open your computer from your phone',
        summary: 'Its folders on your phone: watch its videos, see its photos, read its documents.',
        body: (
          <>
            <Steps>
              <li>On your phone, go to <b>Files</b>, then <b>Remote access</b>, and choose your computer.</li>
              <li>
                Choose <b>Shared</b> for the computer’s shared folder, or <b>Private</b> for your phone’s own folder kept on it.
              </li>
              <li>Tap any file to open it. A long video starts playing while the rest of it is still on its way.</li>
            </Steps>
            <Note tone="soon">
              Today this works when your phone can reach the computer: at home, on the same WiFi. From anywhere else is coming soon.
            </Note>
          </>
        )
      },
      {
        slug: 'change-files',
        title: 'Change your computer’s files from your phone',
        summary: 'Rename, copy, move or delete. The computer follows.',
        body: (
          <>
            <Steps>
              <li>
                In <b>Private</b>, hold a file or a folder. Choose <b>Rename</b>, <b>Copy</b>, <b>Move</b> or <b>Delete</b>.
              </li>
              <li>After a copy, every folder has a <b>Paste</b> at the top. Tap the cross to forget it.</li>
              <li>
                In <b>Shared</b>, you can rename and delete.
              </li>
            </Steps>
            <p>Nothing is lost by mistake: what you delete waits in <b>Recently deleted</b> for 30 days, and you can put it back.</p>
          </>
        )
      },
      {
        slug: 'send',
        title: 'Send a file',
        summary: 'From your phone to your computer, and back.',
        body: (
          <>
            <h2>From your phone</h2>
            <Steps>
              <li>
                On Home, tap <b>Send</b>, choose the photo or file, then choose your computer.
              </li>
              <li>
                Or from any app: <b>Share</b>, then <b>QurbSpace</b>.
              </li>
              <li>It lands in your computer’s Downloads, and the computer tells you it arrived.</li>
            </Steps>
            <h2>From your computer</h2>
            <Steps>
              <li>
                Choose a file, then <b>Send to device</b>, and pick your phone.
              </li>
            </Steps>
            <p>If the other device is switched off, it collects the file the next time it’s on. Nothing keeps a copy on the way.</p>
          </>
        )
      },
      {
        slug: 'backups',
        title: 'Back up WhatsApp, photos and more',
        summary: 'Choose folders once; your phone keeps them backed up on your computer.',
        body: (
          <>
            <Steps>
              <li>
                On your phone, open <b>Folders to back up</b> from Home or Settings.
              </li>
              <li>Tick what to keep safe: WhatsApp, your Camera, Documents, any folder at all.</li>
              <li>That’s it. New photos and messages follow on their own. WhatsApp backs up on WiFi.</li>
            </Steps>
            <h2>Getting them back</h2>
            <Steps>
              <li>
                Go to <b>Files</b>, then <b>Your folder on</b> your computer.
              </li>
              <li>
                Open, <b>Download</b> or <b>Restore</b> any file. Restore puts it back where it was.
              </li>
            </Steps>
            <p>You can also choose the folders from the computer. Whichever you chose last is the one that counts.</p>
          </>
        )
      },
      {
        slug: 'private-vault',
        title: 'Your private vault',
        summary: 'Files that belong to your phone. Your computer keeps them, and opens them only when you say yes.',
        body: (
          <>
            <p>
              Your phone’s own files, its <b>Vault</b>, are kept on your computer too, in a folder that is yours.
            </p>
            <Steps>
              <li>To open it at the computer, choose <b>Open</b> there.</li>
              <li>Your phone asks you, and you answer with your fingerprint, face or screen lock.</li>
              <li>It locks itself again after ten minutes unused.</li>
            </Steps>
            <Note>
              Good to know: on your own computer, this is a rule QurbSpace keeps, because the computer holds your key too. Someone else’s folder on your computer, a guest’s, is sealed: the computer can’t read it at all.
            </Note>
          </>
        )
      },
      {
        slug: 'family',
        title: 'Share a computer with your family',
        summary: 'A private folder for each person, and a Shared folder for all of you.',
        body: (
          <>
            <Note tone="soon">Several people on one computer is being tested.</Note>
            <Steps>
              <li>Each person puts QurbSpace on their own phone.</li>
              <li>Each connects their phone to the computer, the same way you did, with one scan.</li>
              <li>Everyone gets a private folder there, which opens only from their own phone. A guest’s is sealed, so even the computer can’t read it.</li>
              <li>
                The <b>Shared</b> folder is for all of you: anyone can add, open and change what’s in it.
              </li>
            </Steps>
          </>
        )
      },
      {
        slug: 'several-computers',
        title: 'Use more than one computer',
        summary: 'Home and work: your phone keeps every computer you use.',
        body: (
          <>
            <Steps>
              <li>Connect each computer once, the same way, with one scan.</li>
              <li>
                On Home, <b>hold the circle</b> at the top right to choose which computer you’re working with.
              </li>
              <li>
                <b>Files</b>, <b>Remote access</b> lists every computer, with a blue dot when it’s on and a red one when it’s off.
              </li>
            </Steps>
          </>
        )
      },
      {
        slug: 'your-key',
        title: 'Your key, and a new phone',
        summary: 'Nothing to write down, a new phone, and 24 words just in case.',
        body: (
          <>
            <p>
              Your phone makes a key that locks your files. It’s kept safely in your phone’s Google backup when your phone has a screen lock, so there’s nothing to write down.
            </p>
            <h2>A new phone</h2>
            <Steps>
              <li>Install QurbSpace and choose <b>I already use Qurb</b>.</li>
              <li>
                If your key is in your Google backup, choose <b>Use my key</b>.
              </li>
            </Steps>
            <h2>Just in case: the 24 words</h2>
            <p>
              In <b>Settings</b>, <b>Show the 24 words</b>, then <b>Save as text file</b>, and keep it somewhere safe. They bring your files back if every device is lost.
            </p>
            <Note>Anyone with those 24 words can read your files. Keep them to yourself.</Note>
          </>
        )
      }
    ]
  },
  {
    name: 'How it works',
    docs: [
      {
        slug: 'how-it-works',
        title: 'The big picture',
        summary: 'Your devices, one shared space, and no server holding your files.',
        body: (
          <>
            <p>
              Your devices form one group that shares a single key. Each keeps the files in its QurbSpace folder as ordinary files, and they bring each other up to date directly, device to device.
            </p>
            <ul className="doc-list">
              <li>
                <b>The data goes between your devices.</b> Files, their names and their contents travel only between devices you have connected, encrypted on the way.
              </li>
              <li>
                <b>Servers only help them meet.</b> On one WiFi they need none. Across networks, a rendezvous service introduces them and, if no direct path exists, a relay forwards traffic it can’t read.
              </li>
              <li>
                <b>Nothing is central.</b> There are no accounts and no registry of devices: each device keeps its own list of the devices it trusts, filled by pairing and emptied when you remove one.
              </li>
              <li>
                <b>Every file knows where its bytes are:</b> on this device, on another, or only here. QurbSpace never frees the last copy of anything. A phone’s copy doesn’t count as a safe copy, since one tap can erase it.
              </li>
            </ul>
            <p className="doc-more">
              Design record: <a href={repoDoc('docs/decisions/0001-hybrid-p2p-topology.md')}>devices first, servers only to coordinate</a>
            </p>
          </>
        )
      },
      {
        slug: 'how-files-sync',
        title: 'How files sync',
        summary: 'Files cut into pieces by their content; only the pieces that changed move.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>Chunks.</b> Each file is cut into pieces where its content says to (FastCDC), between 128 KiB and 2 MiB, about 512 KiB on average. Change part of a large file and only the pieces around the change are sent again.
              </li>
              <li>
                <b>Named by content.</b> Each piece is named by its BLAKE3 hash, and checked against that name when it arrives. Anything that doesn’t match is refused, as is any path that tries to leave the folder.
              </li>
              <li>
                <b>Versions, not overwrites.</b> If two devices change one file at the same moment, both versions are kept, and you choose which to keep. An edit is never silently lost.
              </li>
              <li>
                <b>Deletions are recorded,</b> so another device can tell a deleted file from one it hasn’t received yet. A deleted file is kept whole in Recently deleted for thirty days.
              </li>
            </ul>
            <p className="doc-more">
              Design records: <a href={repoDoc('docs/decisions/0004-chunk-parameters.md')}>chunk sizes, measured</a> · <a href={repoDoc('docs/decisions/0005-conflict-resolution.md')}>two edits at once</a> · <a href={repoDoc('docs/decisions/0042-recently-deleted.md')}>Recently deleted</a>
            </p>
          </>
        )
      }
    ]
  },
  {
    name: 'Privacy & encryption',
    docs: [
      {
        slug: 'keys',
        title: 'Your key',
        summary: 'One root key, which is your 24 words. Every other key comes from it.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>One root key:</b> 256 random bits from the operating system, which <em>are</em> your 24-word recovery phrase. Every other key is derived from it, one for each purpose.
              </li>
              <li>
                <b>On your phone</b> it is kept in the Android Keystore, and in Google’s Block Store, which backs it up, encrypted with your screen lock, so a new phone restored from your backup gets it back. It does not survive <em>Clear data</em>.
              </li>
              <li>
                <b>On a computer</b> you choose: a file only you can read, the system keystore, or behind a passphrase.
              </li>
              <li>
                <b>Joining a device</b> sends it the key over the new, encrypted connection, once it has proved it saw your code. It never goes through a server.
              </li>
            </ul>
            <Note>All your devices hold the key; nobody else ever does. If you lose the 24 words and every device, no one can recover your files.</Note>
            <p className="doc-more">
              Design records: <a href={repoDoc('docs/decisions/0012-key-hierarchy-and-recovery.md')}>keys and recovery</a> · <a href={repoDoc('docs/decisions/0052-the-key-travels-with-the-code.md')}>the key travels with the code</a>
            </p>
          </>
        )
      },
      {
        slug: 'encryption',
        title: 'What’s encrypted, and when',
        summary: 'Always on the way. On a device, as well as the rest of your disk is, and more where QurbSpace keeps copies.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>Between devices</b>, everything travels over QUIC with TLS 1.3. Each device has its own certificate, and pairing carries its full fingerprint across, so a device only ever talks to devices you added.
              </li>
              <li>
                <b>In your QurbSpace folder</b>, files are ordinary files, protected the way the rest of your disk is: by your disk encryption and your login.
              </li>
              <li>
                <b>In QurbSpace’s own store,</b> which holds copies for your other devices and the parts of files not in the folder, everything is compressed and encrypted with XChaCha20-Poly1305.
              </li>
            </ul>
            <p className="doc-more">
              Design record: <a href={repoDoc('docs/decisions/0011-peer-identity-pinning.md')}>each device’s identity, pinned</a>
            </p>
          </>
        )
      },
      {
        slug: 'private-folders',
        title: 'Private folders: yours and a guest’s',
        summary: 'The same promise to open only from a phone, kept two different ways.',
        body: (
          <>
            <h2>Your own, on your own computer</h2>
            <p>
              Your computer holds your key, so your private folder there is private by QurbSpace’s rule: it opens only when your phone says yes, behind your fingerprint, face or screen lock, and closes after ten minutes unused. It is not encrypted against your own computer.
            </p>
            <h2>A guest’s, on someone else’s computer</h2>
            <ul className="doc-list">
              <li>
                <b>Encrypted with the guest’s own key</b>, derived for that one computer. The computer stores it under names it cannot read.
              </li>
              <li>
                <b>Opened only from the guest’s phone:</b> asked, the phone sends the folder’s key behind its fingerprint, face or screen lock. The computer forgets it when the folder is locked, or after ten minutes unused.
              </li>
              <li>
                <b>No shared pieces between people,</b> so the computer’s owner can’t even tell that a guest holds the same file they do. The owner sees only that the guest exists, and how much space they use.
              </li>
            </ul>
            <Note tone="soon">Guests are being tested.</Note>
            <p className="doc-more">
              Design record: <a href={repoDoc('docs/decisions/0060-a-computer-keeps-private-folders-for-several-people.md')}>a computer keeps private folders for several people</a>
            </p>
          </>
        )
      },
      {
        slug: 'who-learns-what',
        title: 'Who can learn what',
        summary: 'Everyone in the picture, and exactly what each one can see.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>Your devices</b> see your files. Nobody else does.
              </li>
              <li>
                <b>The rendezvous service</b> learns your devices’ network addresses and that they want to meet, under identifiers it can’t link to a person. Never a file, a file’s name or a key.
              </li>
              <li>
                <b>A relay,</b> when one is used, forwards encrypted traffic it cannot read.
              </li>
              <li>
                <b>Google,</b> if push is set up to wake your phone, learns that it was woken, and when. Nothing about why.
              </li>
              <li>
                <b>The apps</b> send no analytics and no crash reports, and there is no account.
              </li>
            </ul>
            <p className="doc-more">
              Design record: <a href={repoDoc('docs/decisions/0016-what-signalling-learns.md')}>what the introduction service learns</a>
            </p>
          </>
        )
      },
      {
        slug: 'this-website',
        title: 'What this website keeps',
        summary: 'Only the email address you give the waitlist.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                If you join the waitlist, it keeps your email address, and nothing else about you, to tell you once when QurbSpace is ready. It’s kept with Upstash, a database service.
              </li>
              <li>It sets no cookies and runs no analytics.</li>
              <li>It loads nothing from other sites: even its font is served from here.</li>
              <li>It’s hosted by Vercel, which keeps the ordinary logs any web host keeps, such as addresses and pages asked for.</li>
            </ul>
            <p>The apps send no analytics and no crash reports, and need no account.</p>
          </>
        )
      },
      {
        slug: 'limits',
        title: 'What it doesn’t protect against yet',
        summary: 'The limits, said as plainly as the promises.',
        body: (
          <ul className="doc-list">
            <li>
              <b>Your devices share one key.</b> A file in one device’s private vault is private from devices that don’t hold its bytes, not cryptographically private from one that does.
            </li>
            <li>
              <b>Removing a device doesn’t take its key away.</b> It stops being trusted at once, and keeps what it already has. Changing the key is not built yet.
            </li>
            <li>
              <b>Nobody outside the project has reviewed it.</b> It is tested, including against hostile devices and crashes, but it has not been audited.
            </li>
            <li>
              Found something? <Link href="/responsible-disclosure">Report it</Link>.
            </li>
          </ul>
        )
      }
    ]
  },
  {
    name: 'Networking',
    docs: [
      {
        slug: 'local-discovery',
        title: 'Finding each other on one network',
        summary: 'On the same WiFi, no server at all.',
        body: (
          <>
            <p>
              Every device sends a small encrypted beacon across the local network saying who it is and where it can be reached, and listens for the others’. That is the whole protocol. Nothing outside your network is involved.
            </p>
            <p>Some networks, in offices, hotels and cafés, block these beacons. There, connect with the QR code.</p>
            <p className="doc-more">
              Design record: <a href={repoDoc('docs/decisions/0034-finding-each-other-with-no-server.md')}>finding each other with no server</a>
            </p>
          </>
        )
      },
      {
        slug: 'rendezvous',
        title: 'The rendezvous service',
        summary: 'Introduces your devices across networks. Never sees a file.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>What it does:</b> tells devices on different networks where to find each other, keeps a short message for a device that is away, and can wake a sleeping phone through push.
              </li>
              <li>
                <b>What it sees:</b> your devices announce themselves under identifiers derived from your key, the same group for all your devices, so it sees opaque bytes. It holds no key, learns no file name, and never sees a piece of a file.
              </li>
              <li>
                <b>Who runs it:</b> today, you do. <Cmd>qurb signal</Cmd> runs one, on any Linux server or computer. One that anyone can use is coming; until then, QurbSpace works away from home only for those who run their own.
              </li>
            </ul>
            <p className="doc-more">
              Design records: <a href={repoDoc('docs/decisions/0016-what-signalling-learns.md')}>what it learns</a> · <a href={repoDoc('docs/decisions/0022-the-service-announces-arrivals.md')}>how it tells a device there is work</a>
            </p>
          </>
        )
      },
      {
        slug: 'relay',
        title: 'The relay',
        summary: 'When there is no direct path, it forwards traffic it cannot read.',
        body: (
          <>
            <p>
              Some networks stop two devices reaching each other directly. Then they connect through a relay, which forwards opaque packets: the same encrypted QUIC session, with the same pinned certificates, runs inside it.
            </p>
            <ul className="doc-list">
              <li>It is reached over TCP on port 443, so it gets through networks that allow only the web.</li>
              <li>The rendezvous service carries one at <em>/relay</em>, so a device needs nothing set to use it.</li>
            </ul>
            <p className="doc-more">
              Design records: <a href={repoDoc('docs/decisions/0017-relay.md')}>the relay</a> · <a href={repoDoc('docs/decisions/0068-the-rendezvous-service-relays.md')}>the rendezvous service relays</a>
            </p>
          </>
        )
      },
      {
        slug: 'pairing',
        title: 'How pairing works',
        summary: 'What the QR code holds, and why both screens show the same number.',
        body: (
          <>
            <ul className="doc-list">
              <li>
                <b>The code</b> carries the showing device’s certificate fingerprint, where to reach it, and a token that works once. No name, and no key.
              </li>
              <li>
                <b>The number</b> both screens show comes from both devices’ fingerprints and the token, with BLAKE3. Someone who copied your code from your screen holds a different certificate, so their number is different. That’s why you check it before <b>Approve</b>.
              </li>
              <li>
                <b>Same key, checked one way:</b> the devices compare a value derived from the key, never the key itself.
              </li>
            </ul>
            <p className="doc-more">
              Design records: <a href={repoDoc('docs/decisions/0053-approval-same-key-and-safe-copies.md')}>approval by number</a> · <a href={repoDoc('docs/decisions/0066-six-digits-to-pair.md')}>six digits</a>
            </p>
          </>
        )
      }
    ]
  },
  {
    name: 'When something’s wrong',
    docs: [
      {
        slug: 'cant-find-computer',
        title: 'My phone can’t find my computer',
        summary: 'A few things to check.',
        body: (
          <Steps>
            <li>Is the computer on, awake, and is QurbSpace running on it? On your phone, its dot is blue when it’s on and red when it’s off.</li>
            <li>Are they on the same WiFi? Away from home is coming soon.</li>
            <li>Some WiFi, in offices, hotels and cafés, stops devices finding each other. Use the QR code to connect there.</li>
          </Steps>
        )
      },
      {
        slug: 'computer-was-off',
        title: 'The computer was asleep or off',
        summary: 'Nothing is lost. Everything waits.',
        body: (
          <ul className="doc-list">
            <li>Your phone says what’s waiting for the computer, and sends it when the computer is back.</li>
            <li>A file you sent is collected the next time the computer is on.</li>
            <li>Backups carry on from where they stopped.</li>
          </ul>
        )
      },
      {
        slug: 'two-versions',
        title: 'A file has two versions',
        summary: 'When two devices change one file at once, both versions are kept.',
        body: (
          <>
            <p>QurbSpace never quietly throws an edit away. If the same file was changed on two devices at the same moment, you’ll see that it needs attention.</p>
            <Steps>
              <li>Open it: you’ll see both versions, who made each, and when.</li>
              <li>
                Keep <b>this one</b>, <b>the other</b>, or <b>both</b>.
              </li>
            </Steps>
          </>
        )
      }
    ]
  }
]

export const ALL = SECTIONS.flatMap((s) => s.docs.map((d) => ({ ...d, section: s.name })))
