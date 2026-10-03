import Link from 'next/link'
import type { IconName } from '@/components/icon-paths'
import { Icon } from '@/components/icon'
import { Mark } from '@/components/mark'
import { FilesPreview, Journey, PhonePreview, WindowPreview } from '@/components/previews'
import { SiteFooter } from '@/components/site-footer'
import { SiteHeader } from '@/components/site-header'
import { repoDoc, repoUrl } from './site-config'

// Every claim on this page is one docs/features.md makes, at the level it is
// checked there. Change the product, change this page.

const steps = [
  [
    'Put a file in Qurb',
    'A folder on your computer, the app on your phone. Everything in them is part of your Qurb space.'
  ],
  [
    'It goes straight to your other devices',
    'Encrypted before it leaves, over your Wi‑Fi or across the internet. Edit a large file and only the part that changed moves.'
  ],
  [
    'Nothing in the middle keeps it',
    'When your devices are on different networks, a small service introduces them — one you run yourself. It never sees a file, a filename, or who you are.'
  ]
]

const fileTruths: [IconName, string, string][] = [
  [
    'cloud-off',
    'Free local space, keep the file',
    'A file you free stays in Qurb and in the list. Its bytes are on another of your devices, and come back when you ask for them.'
  ],
  [
    'shield-check',
    'Never the only copy',
    "Qurb won’t free the last copy of anything. If no other device has a file, it says so — Only copy here — and keeps it."
  ],
  [
    'hard-drive',
    'As much disk as you allow',
    'Tell Qurb how much space it may use on a computer, and it frees what your other devices already hold to stay inside it.'
  ]
]

const everyday: [IconName, string, string][] = [
  [
    'send',
    'Send to one device',
    "Straight to one device and nobody else’s. If it’s switched off, it collects the file the next time it’s on."
  ],
  [
    'lock-keyhole',
    'Private Vault',
    'Files that belong to one device. Another device you choose can keep a backup it never shows.'
  ],
  [
    'git-compare',
    'Two versions, never a lost edit',
    'If two devices change a file at the same moment, both versions are kept, and you choose which stays.'
  ],
  [
    'rotate-ccw',
    'Recently deleted',
    'Thirty days to change your mind. Restore a file and it comes back on every device.'
  ],
  [
    'folder',
    'Choose who has each folder',
    'Share a folder with some of your devices, or keep one only elsewhere and fetch each file when you open it.'
  ],
  [
    'monitor-smartphone',
    'Finds your devices by itself',
    'On the same Wi‑Fi, with no server at all. Add a device once, with a code you scan or type.'
  ],
  [
    'bell',
    'Wakes your phone',
    'A change on your computer reaches a sleeping phone within seconds, when push is set up.'
  ],
  [
    'upload',
    'From any app on your phone',
    "Share a file into Qurb from anywhere on the phone — even with no network. It goes when there’s a way."
  ]
]

const privacy: [IconName, string, string][] = [
  [
    'hard-drive',
    'Your files stay on your devices',
    "They live where you put them and move between your devices. There’s no Qurb cloud holding a copy — and today, no Qurb servers at all."
  ],
  [
    'lock-keyhole',
    'Encrypted whenever it travels',
    "Everything that leaves a device is encrypted, and goes only to devices you’ve added — each one recognised by its own key. Copies kept for another device are stored encrypted too."
  ],
  [
    'monitor',
    'A service that only introduces',
    'Devices on different networks find each other through a small service you run. It learns that some devices want to meet — never what they share.'
  ]
]

export default function HomePage() {
  return (
    <div className="page">
      <SiteHeader />

      <main>
        <section className="hero">
          <div className="wrap">
            <span className="status-pill glass-frosted">
              <span className="dot" />
              Working today on Linux and Android
            </span>
            <h1 className="display">
              <span>Your files.</span>
              <span>Your devices.</span>
              <span>Your space.</span>
            </h1>
            <p className="lead">
              Qurb keeps your files on the devices you own and moves them directly between them, encrypted. There’s no
              cloud drive in the middle holding a copy.
            </p>
            <div className="hero-buttons">
              <Link className="btn primary large" href="/download">
                Get Qurb
                <Icon name="arrow-right" />
              </Link>
              <a className="btn large" href="#how">
                How it works
              </a>
            </div>

            <div className="showcase">
              <WindowPreview />
              <PhonePreview />
            </div>
          </div>
        </section>

        <section className="section" id="how">
          <div className="wrap">
            <div className="section-head center">
              <div className="eyebrow">How it works</div>
              <h2>
                Not a cloud drive.
                <br />
                Your devices, together.
              </h2>
              <p>
                Qurb gives your computer and your phone one shared space. A file you add on one is on the others, and
                none of them needs a company’s server to keep it.
              </p>
            </div>
            <Journey />
            <ol className="steps">
              {steps.map(([title, text], i) => (
                <li key={title}>
                  <span className="n">{i + 1}</span>
                  <h3>{title}</h3>
                  <p>{text}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        <section className="section" id="files">
          <div className="wrap">
            <div className="split">
              <FilesPreview />
              <div>
                <div className="section-head">
                  <div className="eyebrow">Where every file is</div>
                  <h2>Qurb always knows where your files are.</h2>
                  <p>Every file says where its bytes are, because that decides what you can safely do with it.</p>
                </div>
                <ul className="points" style={{ marginTop: 32 }}>
                  {fileTruths.map(([icon, title, text]) => (
                    <li key={title}>
                      <span className="tile">
                        <Icon name={icon} size={20} />
                      </span>
                      <span>
                        <h3>{title}</h3>
                        <p>{text}</p>
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </section>

        <section className="section" id="features">
          <div className="wrap">
            <div className="section-head center">
              <div className="eyebrow">Everyday things</div>
              <h2>The things you’d expect, done carefully.</h2>
            </div>
            <ul className="features glass-frosted">
              {everyday.map(([icon, title, text]) => (
                <li key={title}>
                  <span className="tile">
                    <Icon name={icon} size={20} />
                  </span>
                  <span>
                    <h3>{title}</h3>
                    <p>{text}</p>
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </section>

        <section className="section" id="privacy">
          <div className="wrap">
            <div className="split reverse">
              <div className="panel glass-frosted" role="img" aria-label="The 24-word recovery phrase, numbered, three to a row, with the words hidden.">
                <div className="panel-head">
                  <strong>Your recovery phrase</strong>
                  <Icon name="key-round" />
                </div>
                <div className="phrase">
                  {Array.from({ length: 24 }, (_, i) => (
                    <span key={i}>
                      {i + 1}
                      <i />
                    </span>
                  ))}
                </div>
                <p className="phrase-note">
                  <Icon name="triangle-alert" />
                  Anyone with these 24 words can read every file you keep in Qurb.
                </p>
              </div>
              <div>
                <div className="section-head">
                  <div className="eyebrow">Privacy</div>
                  <h2>Private because of how it’s built.</h2>
                  <p>
                    Your key is 24 words you write on paper. Lose them and every device, and nobody can recover your files —
                    not us, not anyone. That isn’t a gap in the promise. It is the promise.
                  </p>
                </div>
                <ul className="points" style={{ marginTop: 32 }}>
                  {privacy.map(([icon, title, text]) => (
                    <li key={title}>
                      <span className="tile">
                        <Icon name={icon} size={20} />
                      </span>
                      <span>
                        <h3>{title}</h3>
                        <p>{text}</p>
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </section>

        <section className="section" id="today">
          <div className="wrap">
            <div className="section-head center">
              <div className="eyebrow">Today</div>
              <h2>Where Qurb is, plainly.</h2>
              <p>
                Qurb is built in the open, and says what works and what doesn’t yet.{' '}
                <a className="text-link" href={repoDoc('docs/features.md')}>
                  Everything it does, and how far each part is checked
                  <Icon name="arrow-right" size={16} />
                </a>
              </p>
            </div>
            <div className="today">
              <div className="panel glass-frosted works">
                <h3>
                  <Icon name="circle-check" size={20} />
                  Works today
                </h3>
                <ul>
                  {[
                    ['The desktop app', 'on Linux, with an Arch package'],
                    ['The Android app', 'synced with a laptop both ways, on a real phone'],
                    ['Syncing from anywhere', 'over Wi‑Fi or mobile data, through a service you run'],
                    ['Push', 'a sleeping phone woken within seconds'],
                    ['A command line', 'for everything the apps do']
                  ].map(([what, how]) => (
                    <li key={what}>
                      <Icon name="check" size={18} />
                      <span>
                        <b>{what}</b> — {how}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
              <div className="panel glass-frosted later">
                <h3>
                  <Icon name="clock" size={20} />
                  Not yet
                </h3>
                <ul>
                  {[
                    ['A release', 'for now, Qurb is built from source'],
                    ['macOS, Windows and iOS', 'Linux and Android come first'],
                    ['A relay anyone can use', 'for networks that block a direct connection; it works, and nothing hosts one yet'],
                    ['Dark mode', 'after the light design is settled'],
                    ['Recovery without the 24 words', 'deliberately, for now']
                  ].map(([what, how]) => (
                    <li key={what}>
                      <Icon name="clock" size={18} />
                      <span>
                        <b>{what}</b> — {how}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </section>

        <div className="wrap">
          <section className="cta glass-frosted" aria-labelledby="try-it">
            <Mark size={56} />
            <h2 id="try-it">Try it on your own devices.</h2>
            <p>There’s no release yet. Qurb builds from source on Linux, and the Android app builds from the same repository.</p>
            <div className="hero-buttons">
              <Link className="btn primary large" href="/download">
                Get Qurb
                <Icon name="arrow-right" />
              </Link>
              <a className="btn large" href={repoUrl}>
                The code, on GitHub
                <Icon name="external-link" />
              </a>
            </div>
          </section>
        </div>
      </main>

      <SiteFooter />
    </div>
  )
}
