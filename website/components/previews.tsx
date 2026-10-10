/**
 * The apps, drawn on the page from their own parts: the window’s sidebar,
 * stage, rows and status mark; the phone’s Home and floating tab bar. The
 * words are the apps' own (crates/desktop/ui/home.js, android/.../Words.kt),
 * and the files and devices are made up. Pictures, so each is one image to a
 * screen reader, described in words.
 */
import type { IconName } from './icon-paths'
import { Icon } from './icon'
import { Mark } from './mark'

export type Row = { icon: IconName; name: string; sub: string[]; state?: { cls: string; icon: IconName; words: string }; arriving?: boolean }

export function Rows({ rows, lit }: { rows: Row[]; lit?: number }) {
  return (
    <ul className="rows">
      {rows.map((row, i) => (
        <li key={row.name} className={i === lit ? 'row lit' : 'row'}>
          <span className={row.arriving ? 'tile arriving' : 'tile'}>
            <Icon name={row.icon} />
          </span>
          <span>
            <span className="name">{row.name}</span>
            <span className="sub">
              {row.state && (
                <span className={`state ${row.state.cls}`}>
                  <Icon name={row.state.icon} size={14} />
                  {row.state.words}
                </span>
              )}
              {row.sub.map((part) => (
                <span key={part}>{part}</span>
              ))}
            </span>
          </span>
        </li>
      ))}
    </ul>
  )
}

const sidebar: [IconName, string][] = [
  ['house', 'Home'],
  ['folder', 'Files'],
  ['monitor-smartphone', 'Devices'],
  ['hard-drive', 'Storage']
]

/** The desktop window on Home, everything synced (direction §5, §24). */
export function WindowPreview() {
  return (
    <div
      className="window glass-frosted"
      role="img"
      aria-label="The Qurb window on Linux: a sidebar with Home, Files, Devices, Storage and Private Vault; Home says Everything is synced, with a Send to device button and the files that arrived recently."
    >
      <div className="titlebar">
        <span className="title">Qurb</span>
        <span className="controls">
          <span />
          <span />
          <span />
        </span>
      </div>
      <div className="app">
        <div className="sidebar glass-clear">
          <span className="brand">
            <Mark size={26} />
            Qurb
          </span>
          {sidebar.map(([icon, label], i) => (
            <span key={label} className={i === 0 ? 'nav on' : 'nav'}>
              <Icon name={icon} />
              {label}
            </span>
          ))}
          <span className="label">Private</span>
          <span className="nav">
            <Icon name="lock-keyhole" />
            Private Vault
          </span>
          <span className="spacer" />
          <span className="nav">
            <Icon name="settings" />
            Settings
          </span>
        </div>
        <div className="stage glass-frosted">
          <div className="home-hero">
            <span className="status-mark">
              <Icon name="check" size={24} />
            </span>
            <span>
              <h3>Everything is synced.</h3>
              <p>Your files are safe. Nothing needs your attention.</p>
            </span>
          </div>
          <div className="home-actions">
            <span className="btn primary inert">
              <Icon name="send" />
              Send to device
            </span>
            <span className="facts">48.2 GB used · 2 devices connected</span>
          </div>
          <div className="home-body">
            <h4>
              Recent <span>See all</span>
            </h4>
            <Rows
              lit={0}
              rows={[
                { icon: 'file-image', name: 'IMG_2041.jpg', sub: ['Arrived from Phone', '2 minutes ago', '4.7 MB'] },
                { icon: 'file-text', name: 'Thesis draft.pdf', sub: ['Saved on this computer', '18 minutes ago', '1.2 MB'] },
                { icon: 'circle-check', name: 'Tickets.pdf', sub: ['Collected by Phone', '1 hour ago'] },
                { icon: 'cloud-off', name: 'Holiday.mp4', sub: ['Local space freed', 'Yesterday', '2.3 GB'] }
              ]}
            />
          </div>
        </div>
      </div>
    </div>
  )
}

const tabs: [IconName, string][] = [
  ['house', 'Home'],
  ['folder', 'Files'],
  ['monitor-smartphone', 'Devices'],
  ['settings', 'Settings']
]

/** The Android app on Home, under its floating tab bar (§25). */
export function PhonePreview() {
  return (
    <div
      className="phone"
      role="img"
      aria-label="The Qurb app on Android: Home says Everything is synced, with Send to device, and the files that arrived recently; four tabs below — Home, Files, Devices, Settings."
    >
      <div className="screen">
        <div className="bar">
          <span>12:30</span>
          <span>5G</span>
        </div>
        <div className="head">
          <Mark size={22} />
          Qurb
        </div>
        <div className="home">
          <span className="status-mark small">
            <Icon name="check" size={21} />
          </span>
          <h3>Everything is synced.</h3>
          <p>Your files are safe. Nothing needs your attention.</p>
          <span className="btn primary inert">
            <Icon name="send" size={17} />
            Send to device
          </span>
          <span className="facts">48.2 GB of files · 2 devices · synced 2 minutes ago</span>
        </div>
        <div className="recent">
          <h4>
            Recent <span>See all</span>
          </h4>
          <Rows
            rows={[
              // One line of words, as the phone’s own row draws them (Words.happened).
              { icon: 'file-image', name: 'IMG_2041.jpg', sub: ['Saved on this phone · 2 minutes ago'] },
              { icon: 'file-text', name: 'Thesis draft.pdf', sub: ['Arrived from Laptop · 18 minutes ago'] }
            ]}
          />
        </div>
        <div className="tabs glass-clear">
          {tabs.map(([icon, label], i) => (
            <span key={label} className={i === 0 ? 'on' : undefined}>
              <Icon name={icon} size={20} />
              {label}
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}

/** Files, with each file’s state in the apps' words (§11, §18). */
export function FilesPreview() {
  return (
    <div
      className="panel glass-frosted"
      role="img"
      aria-label="A list of files, each saying where it is: On this device, Available elsewhere, Only copy here, Downloading."
    >
      <div className="panel-head">
        <strong>Files</strong>
        <span className="crumbs">
          Qurb <Icon name="chevron-right" size={14} /> <b>Photos</b>
        </span>
      </div>
      <Rows
        lit={1}
        rows={[
          { icon: 'file-image', name: 'Lake at dawn.jpg', sub: ['6.1 MB'], state: { cls: 'here', icon: 'hard-drive', words: 'On this device' } },
          { icon: 'file-video', name: 'Holiday.mp4', sub: ['2.3 GB'], state: { cls: 'elsewhere', icon: 'cloud', words: 'Available elsewhere' } },
          { icon: 'file-text', name: 'Notes from Tuesday.md', sub: ['12 KB'], state: { cls: 'only', icon: 'triangle-alert', words: 'Only copy here' } },
          { icon: 'file-image', name: 'IMG_2041.jpg', sub: ['4.7 MB'], state: { cls: 'moving', icon: 'download', words: 'Downloading' }, arriving: true }
        ]}
      />
    </div>
  )
}
