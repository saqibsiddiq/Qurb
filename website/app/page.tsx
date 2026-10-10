import Link from 'next/link'
import type { IconName } from '@/components/icon-paths'
import { Icon } from '@/components/icon'
import { Mark } from '@/components/mark'
import { Motion } from '@/components/motion'
import { FilesPreview, PhonePreview, WindowPreview } from '@/components/previews'
import { GuestScene, SendScene } from '@/components/scenes'
import { SiteFooter } from '@/components/site-footer'
import { SiteHeader } from '@/components/site-header'
import { repoDoc, repoUrl } from './site-config'
import './landing.css'

// Every claim on this page is one docs/features.md makes, at the level it is
// checked there; what is only in tests says so. Change the product, change
// this page.

// Words, each its own span, so the statement can light them in turn.
function Words({ text }: { text: string }) {
  return (
    <>
      {text.split(' ').map((word, i) => (
        <span key={i} className="w">
          {word}{' '}
        </span>
      ))}
    </>
  )
}

const steps: [string, string, string][] = [
  [
    '01',
    'Add a device',
    'Scan the code one device shows with the other. Both screens show the same six digits; say yes, and they trust each other. No account, no password, nothing to write down.'
  ],
  [
    '02',
    'Put a file in',
    'Save it into Qurb on your computer, or add it in the app on your phone, and it’s on your other devices. Change a large file and only the part that changed moves.'
  ],
  [
    '03',
    'Nothing in the middle',
    'Your devices reach each other directly, encrypted. On different networks, a small service you run introduces them — it never sees a file, a filename, or who you are.'
  ]
]

const fileTruths: [IconName, string, string][] = [
  [
    'cloud-off',
    'Free space, keep the file',
    'A file you free stays in Qurb and in the list. Its bytes are on another of your devices, and come back when you open it.'
  ],
  [
    'shield-check',
    'Never the only copy',
    'Qurb won’t free the last copy of anything. If no other device has a file, it says so — Only copy here — and keeps it.'
  ],
  [
    'hard-drive',
    'As much disk as you allow',
    'Tell Qurb how much space it may use on a computer, and it frees what your other devices already hold to stay inside it.'
  ]
]

const possibilities: [IconName, string, string][] = [
  ['smartphone', 'Your phone’s photos, on your computer', 'Without a cable, and without uploading them anywhere first.'],
  ['cloud-off', 'A full phone, emptied safely', 'Free its space; every file stays in the list, and comes back when you open it.'],
  ['users', 'The family computer', 'A private folder for everyone who uses it, none of which the others — or the computer — can open.'],
  ['send', 'Tickets to the laptop, nobody else', 'Send to one device. It’s the only one that gets it, and Qurb keeps no copy.'],
  ['lock-keyhole', 'A backup only you can read', 'Private Vault files belong to one device; another you choose keeps an encrypted backup it never shows.'],
  ['git-compare', 'Two edits, both kept', 'Changed on two devices at once? Both versions stay, and you choose which one wins.'],
  ['rotate-ccw', 'Thirty days to change your mind', 'Delete on any device, restore on every device.'],
  ['upload', 'Share from any app', 'Send a file into Qurb from anywhere on your phone — even offline. It goes when there’s a way.']
]

const today: { title: string; icon: IconName; cls: string; items: [string, string][] }[] = [
  {
    title: 'Works today',
    icon: 'circle-check',
    cls: 'works',
    items: [
      ['The desktop app', 'on Linux, with an Arch package'],
      ['The Android app', 'synced with a laptop both ways, on a real phone'],
      ['Across networks', 'Wi‑Fi or mobile data, through a service you run'],
      ['Push', 'a sleeping phone woken within seconds'],
      ['Dark mode', 'on both, as your system is set'],
      ['A command line', 'for everything the apps do']
    ]
  },
  {
    title: 'Being tested',
    icon: 'history',
    cls: 'testing',
    items: [
      ['Guests and their sealed folders', 'in tests and on a laptop; not yet between two people’s phones'],
      ['Opening your folder with your fingerprint', 'built; not yet tried on a phone'],
      ['Sends that keep no copy', 'in tests and on a laptop']
    ]
  },
  {
    title: 'Not yet',
    icon: 'clock',
    cls: 'later',
    items: [
      ['A release', 'for now, Qurb is built from source'],
      ['macOS, Windows and iOS', 'Linux and Android come first'],
      ['A relay anyone can use', 'for networks that block a direct connection'],
      ['Recovery on a computer', 'a phone’s key is backed up; a computer’s isn’t yet']
    ]
  }
]

export default function HomePage() {
  return (
    <div className="page landing">
      <Motion />
      <SiteHeader />

      <main>
        {/* ---------------------------------------------------------- hero */}
        <section className="l-hero">
          <div className="wrap">
            <span className="status-pill glass-frosted rise" style={{ '--i': 0 } as React.CSSProperties}>
              <span className="dot" />
              For Linux and Android · built in the open
            </span>
            <h1 className="l-display">
              <span className="rise" style={{ '--i': 1 } as React.CSSProperties}>
                Your files,
              </span>
              <span className="rise green" style={{ '--i': 2 } as React.CSSProperties}>
                on your devices.
              </span>
            </h1>
            <p className="l-lead rise" style={{ '--i': 3 } as React.CSSProperties}>
              Qurb moves your files straight between your phone and your computer, encrypted. No cloud drive in the
              middle keeps a copy — not even of what you send.
            </p>
            <div className="hero-buttons rise" style={{ '--i': 4 } as React.CSSProperties}>
              <Link className="btn primary large" href="/download">
                Get Qurb
                <Icon name="arrow-right" />
              </Link>
              <a className="btn large" href="#how">
                How it works
              </a>
            </div>
            <div className="showcase rise slow" style={{ '--i': 5 } as React.CSSProperties}>
              <WindowPreview />
              <PhonePreview />
            </div>
          </div>
        </section>

        {/* ------------------------------------------------ the one idea */}
        <section className="l-statement" aria-label="What Qurb is">
          <div className="wrap">
            <p className="statement">
              <Words text="Qurb isn’t a place to keep your files. It’s how they cross — from the phone in your hand to the computer on your desk, and to the people you choose." />
            </p>
          </div>
        </section>

        {/* ------------------------------------------------- how it works */}
        <section className="l-section" id="how">
          <div className="wrap">
            <div className="l-head reveal">
              <div className="l-eyebrow">How it works</div>
              <h2 className="l-h2">
                Three steps.
                <br />
                <span className="soft">No account.</span>
              </h2>
            </div>
            <ol className="chapters">
              {steps.map(([n, title, text]) => (
                <li key={n} className="reveal">
                  <span className="n">{n}</span>
                  <h3>{title}</h3>
                  <p>{text}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        {/* ------------------------------------------------------ sending */}
        <section className="l-section" id="send">
          <div className="wrap">
            <div className="l-split">
              <div className="l-head reveal">
                <div className="l-eyebrow">Sending</div>
                <h2 className="l-h2">A send is a journey, not a copy.</h2>
                <p className="l-body">
                  Send a file to one device and it goes there, and nowhere else. Qurb reads it from where it is at the
                  moment that device collects it, and keeps nothing of its own. Change or delete the file before then,
                  and the send says so, rather than sending something else.
                </p>
                <ul className="l-points">
                  <li>
                    <Icon name="refresh-cw" size={18} />
                    <span>
                      <b>Sending it again?</b> Qurb asks, rather than quietly skipping a file it has sent before.
                    </span>
                  </li>
                  <li>
                    <Icon name="clock" size={18} />
                    <span>
                      <b>Switched off?</b> The device collects it the next time it’s on.
                    </span>
                  </li>
                </ul>
              </div>
              <div className="reveal">
                <SendScene />
              </div>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------- people */}
        <section className="l-night" id="people">
          <div className="wrap">
            <div className="l-head center reveal">
              <span className="pill-new">New · being tested</span>
              <div className="l-eyebrow">One computer, many people</div>
              <h2 className="l-h2">
                Your own folder on a shared computer.
                <br />
                <span className="soft">Only you can open it.</span>
              </h2>
              <p className="l-body">
                Add the people who use your computer as guests. Each keeps their own key, and can keep their own folder
                on it. The computer holds those folders sealed: it can’t read a filename, let alone a file.
              </p>
            </div>
            <div className="reveal">
              <GuestScene />
            </div>
            <div className="night-points">
              <div className="reveal">
                <span className="tile">
                  <Icon name="fingerprint" size={20} />
                </span>
                <h3>Opened with your fingerprint</h3>
                <p>
                  To open your folder at that computer, approve on your phone — with your fingerprint, your face or your
                  screen lock. It stays open until it’s locked there, or ten minutes go by unused.
                </p>
              </div>
              <div className="reveal">
                <span className="tile">
                  <Icon name="folder" size={20} />
                </span>
                <h3>Your folder, or their Downloads</h3>
                <p>
                  Sharing from your phone, choose: into your folder on that computer, or straight into its Downloads,
                  as an ordinary file.
                </p>
              </div>
              <div className="reveal">
                <span className="tile">
                  <Icon name="user-plus" size={20} />
                </span>
                <h3>Nothing shared by accident</h3>
                <p>
                  A guest sees only what is sent to them. Your own files, and the computer’s, stay where they are.
                </p>
              </div>
            </div>
          </div>
        </section>

        {/* --------------------------------------------- where each file is */}
        <section className="l-section" id="files">
          <div className="wrap">
            <div className="l-split reverse">
              <div className="reveal">
                <FilesPreview />
              </div>
              <div className="l-head reveal">
                <div className="l-eyebrow">Where every file is</div>
                <h2 className="l-h2">It always knows where your files are.</h2>
                <p className="l-body">
                  Every file says where its bytes are, because that decides what you can safely do with it.
                </p>
                <ul className="truths">
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

        {/* ------------------------------------------------ possibilities */}
        <section className="l-section" id="possible">
          <div className="wrap">
            <div className="l-head center reveal">
              <div className="l-eyebrow">What it makes possible</div>
              <h2 className="l-h2">Small things, done properly.</h2>
            </div>
            <ul className="possible">
              {possibilities.map(([icon, title, text], i) => (
                <li key={title} className="glass-frosted reveal" style={{ '--i': i % 4 } as React.CSSProperties}>
                  <span className="tile">
                    <Icon name={icon} size={20} />
                  </span>
                  <h3>{title}</h3>
                  <p>{text}</p>
                </li>
              ))}
            </ul>
          </div>
        </section>

        {/* ------------------------------------------------------ privacy */}
        <section className="l-section" id="privacy">
          <div className="wrap">
            <div className="l-head reveal">
              <div className="l-eyebrow">Privacy</div>
              <h2 className="l-h2">
                Private because of how it’s built.
                <br />
                <span className="soft">Not because we promise.</span>
              </h2>
            </div>
            <div className="figures">
              <div className="reveal">
                <span className="fig">0</span>
                <h3>Qurb servers holding your files</h3>
                <p>Your files live on your devices and move between them. There is no Qurb cloud keeping a copy.</p>
              </div>
              <div className="reveal" style={{ '--i': 1 } as React.CSSProperties}>
                <span className="fig">1</span>
                <h3>Key, and it’s yours</h3>
                <p>
                  Everything that travels is encrypted, and goes only to devices you’ve added. Lose every device and
                  nobody can recover your files — not us, not anyone.
                </p>
              </div>
              <div className="reveal" style={{ '--i': 2 } as React.CSSProperties}>
                <span className="fig">0</span>
                <h3>Words to write down</h3>
                <p>
                  Your key travels in the pairing code, and on Android in Google’s end‑to‑end encrypted backup when the
                  phone has a screen lock. The 24 words are there if you want them.
                </p>
              </div>
            </div>
          </div>
        </section>

        {/* -------------------------------------------------------- today */}
        <section className="l-section" id="today">
          <div className="wrap">
            <div className="l-head center reveal">
              <div className="l-eyebrow">Today</div>
              <h2 className="l-h2">Where Qurb is, plainly.</h2>
              <p className="l-body">
                Built in the open, and honest about what works and what doesn’t yet.{' '}
                <a className="text-link" href={repoDoc('docs/features.md')}>
                  Everything it does, and how far each part is checked
                  <Icon name="arrow-right" size={16} />
                </a>
              </p>
            </div>
            <div className="today3">
              {today.map((col) => (
                <div key={col.title} className={`panel glass-frosted reveal ${col.cls}`}>
                  <h3>
                    <Icon name={col.icon} size={20} />
                    {col.title}
                  </h3>
                  <ul>
                    {col.items.map(([what, how]) => (
                      <li key={what}>
                        <b>{what}</b>
                        <span>{how}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* ---------------------------------------------------------- end */}
        <section className="l-end">
          <div className="wrap">
            <div className="reveal">
              <Mark size={64} />
              <h2 className="l-h2">Try it on your own devices.</h2>
              <p className="l-body">
                There’s no release yet. Qurb builds from source on Linux, and the Android app builds from the same
                repository.
              </p>
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
            </div>
          </div>
        </section>
      </main>

      <SiteFooter />
    </div>
  )
}
