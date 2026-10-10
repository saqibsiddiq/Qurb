import type { Metadata } from 'next'
import { DocPage, LinkRow } from '@/components/doc-page'
import { Icon } from '@/components/icon'
import { repoDoc, repoUrl } from '../site-config'

export const metadata: Metadata = {
  title: 'Get Qurb',
  description: 'How to install Qurb today: built from source on Linux, and the Android app from the same repository.'
}

export default function DownloadPage() {
  return (
    <DocPage
      title="Get Qurb"
      intro="There’s no release to download yet. Qurb builds from its source on Linux, and the Android app builds from the same repository — this is how."
    >
      <p className="callout">
        <Icon name="circle-check" size={20} />
        <span>
          <b>Linux and Android, first.</b> Both work today, and a formal release for both is planned. macOS, Windows
          and iOS come after them; nothing is built for those yet.
        </span>
      </p>

      <h2>On Linux</h2>
      <p>
        You need the source and a Rust toolchain. On Arch and its derivatives, a package of the checkout installs the
        app, the tray icon and the command line for everyone on the computer:
      </p>
      <pre>
        {`git clone ${repoUrl}.git && cd Qurb
cd packaging/arch && makepkg -si`}
      </pre>
      <p>On any other distribution, for your user only:</p>
      <pre>
        {`cargo build --release -p qurb-cli -p qurb-tray -p qurb-desktop
./packaging/install.sh`}
      </pre>
      <p>
        Then open <strong>Qurb</strong> from your applications menu. It asks where your files should live and how much
        of the disk it may use, and starts. There’s nothing to write down: your other devices get the key from the code
        you add them with.
      </p>

      <h2>On Android</h2>
      <p>
        The app builds from the same repository. It needs the Android NDK, because the engine inside it is the same Rust
        the desktop runs, and JDK 17. With your phone connected over ADB:
      </p>
      <pre>{`./scripts/android-app.sh install`}</pre>
      <p>
        On first launch it sets up a new key, or joins your computer by scanning the code it shows. The key is kept in the
        Android Keystore, and backed up through Google’s Block Store — end-to-end encrypted when the phone has a screen
        lock — so a reinstalled app finds it. The 24 words are a fallback, if you have them.
      </p>

      <h2>Adding your phone</h2>
      <ol>
        <li>
          On the computer: <strong>Devices</strong> → <strong>Add a device</strong> → <strong>Show a code</strong>.
        </li>
        <li>
          On the phone: <strong>Devices</strong> → <strong>Add</strong>, and scan it. The code works once, for five
          minutes.
        </li>
        <li>
          On the same Wi‑Fi they find each other with no server at all. To sync from anywhere else, they need a small
          service both can reach, which you run yourself.
        </li>
      </ol>

      <h2>Step by step</h2>
      <div className="rows">
        <LinkRow
          href={repoDoc('docs/trying-it.md')}
          icon="file-text"
          title="Trying it on your own hardware"
          text="From one machine to a phone, every step run and checked"
        />
        <LinkRow
          href={repoDoc('docs/anywhere.md')}
          icon="arrow-up-down"
          title="Syncing from outside the house"
          text="What has to be reachable, and a free way to get there"
        />
        <LinkRow
          href={repoDoc('android/README.md')}
          icon="smartphone"
          title="The Android app"
          text="Building it, signing a release, and what it does in the background"
        />
      </div>
    </DocPage>
  )
}
