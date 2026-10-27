'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView } from './play-on-view'
import { TALL_WALK } from './tall'
import { press, show, tap, type Keys } from './timeline'

// The walkthrough under the hero (docs/design/website-brief.md): a laptop and
// a phone, in three chapters —
// pairing once with the QR code, opening the computer's folders from the
// phone, and changing them from the phone with the computer following. The
// screens are the app's, simplified: its green, its circle, its sheets and
// round action buttons, with less detail.
//
// Every moving part has keyframes in one number, p, across all three
// chapters (TIMELINE); each chapter is a scene of its own that plays its part
// of p once, by the clock, when it comes into view. It was pinned and driven
// by the scroll until the owner found that too unlike the rest of the page.

const TIMELINE: Record<string, Keys> = {
  // Chapter 1 — connect once
  'ch-1': { o: [[0, 1], [0.28, 1], [0.31, 0]], y: [[0.28, 0], [0.31, -16]] },
  phone: {
    x: [[0.065, 0], [0.13, -150], [0.25, -150], [0.31, 0]],
    y: [[0.065, 0], [0.13, 24], [0.25, 24], [0.31, 0]],
    r: [[0.065, 0], [0.13, -7], [0.25, -7], [0.31, 0]],
    tall: { x: [[0, 0]], y: [[0.065, 0], [0.13, -130], [0.25, -130], [0.31, 0]], r: [[0.065, 0], [0.13, -4], [0.25, -4], [0.31, 0]] }
  },
  'p-join': { o: [[0, 1], [0.055, 1], [0.072, 0]] },
  'p-scan-btn': { s: press(0.04) },
  'tap-scan': tap(0.04),
  'p-cam': { o: show(0.058, 0.075, 0.145, 0.158) },
  'p-scan': { y: [[0.085, 0], [0.125, 150]] },
  'p-found': { o: [[0.125, 0], [0.135, 1]] },
  'p-found-name': { o: [[0.125, 0], [0.135, 1]] },
  'p-home': { o: [[0.27, 0], [0.3, 1], [0.43, 1], [0.45, 0]] },
  'p-number': { o: show(0.145, 0.155, 0.215, 0.225) },
  'p-ok': { o: show(0.215, 0.225, 0.27, 0.29) },
  'p-ok-check': { s: [[0.215, 0.5], [0.235, 1]] },
  'l-pair': { o: [[0, 1], [0.3, 1], [0.32, 0]] },
  'l-qr': { o: [[0, 1], [0.14, 1], [0.15, 0]] },
  'l-number': { o: show(0.14, 0.15, 0.21, 0.22) },
  'l-approve': { s: press(0.188) },
  'l-cursor': { o: show(0.16, 0.17, 0.205, 0.215), x: [[0.16, 110], [0.185, 0]], y: [[0.16, 70], [0.185, 0]] },
  'l-ok': { o: show(0.215, 0.225) },
  'l-ok-check': { s: [[0.215, 0.5], [0.235, 1]] },

  // Chapter 2 — your computer, in your pocket
  'ch-2': { o: show(0.31, 0.34, 0.61, 0.64), y: [[0.31, 16], [0.34, 0], [0.61, 0], [0.64, -16]] },
  'l-files': { o: show(0.31, 0.34) },
  'tap-circle': tap(0.36),
  'p-sheet': { o: show(0.37, 0.385, 0.43, 0.445) },
  'p-sheet-panel': { y: [[0.37, 240], [0.392, 0]] },
  'p-private': { s: press(0.415) },
  'tap-private': tap(0.415),
  'p-files': { o: show(0.43, 0.45), x: [[0.43, 50], [0.45, 0]] },
  beam: { o: show(0.46, 0.5, 0.6, 0.63) },
  // …and opened there as if it were here: a video playing as it arrives,
  // a photo, a PDF.
  'tap-video': tap(0.47),
  'p-video': { o: show(0.478, 0.49, 0.535, 0.545), s: [[0.478, 0.94], [0.495, 1]] },
  'p-vbar': { w: [[0.49, 0], [0.535, 38]] },
  'p-vplay': { o: [[0.49, 1], [0.5, 0]], s: [[0.49, 1], [0.5, 1.6]] },
  'l-stream': { o: show(0.485, 0.495, 0.535, 0.545) },
  'tap-photo': tap(0.551),
  'p-photo': { o: show(0.557, 0.568, 0.585, 0.595), s: [[0.557, 0.94], [0.573, 1]] },
  'tap-pdf': tap(0.598),
  'p-pdf': { o: show(0.603, 0.612, 0.623, 0.63), s: [[0.603, 0.94], [0.616, 1]] },

  // Chapter 3 — change it from your phone
  'ch-3': { o: show(0.63, 0.66), y: [[0.63, 16], [0.66, 0]] },
  'tap-notes': { o: [[0.67, 0], [0.675, 0.5], [0.7, 0]], s: [[0.67, 0.3], [0.7, 1.7]] },
  'p-act-1': { o: show(0.685, 0.7, 0.728, 0.738) },
  'p-act-1-panel': { y: [[0.685, 220], [0.705, 0]] },
  'p-rename': { s: press(0.716) },
  'tap-rename': tap(0.716),
  'p-notes-old': { o: [[0.738, 1], [0.745, 0]] },
  'p-notes-new': { o: [[0.744, 0], [0.745, 1]], w: [[0.745, 0], [0.79, 100]] },
  'l-notes-old': { o: [[0.8, 1], [0.81, 0]] },
  'l-notes-new': { o: [[0.8, 0], [0.81, 1]] },
  'l-notes-pulse': { o: [[0.8, 0], [0.81, 1], [0.85, 0]] },
  'tap-draft': { o: [[0.85, 0], [0.855, 0.5], [0.88, 0]], s: [[0.85, 0.3], [0.88, 1.7]] },
  'p-act-2': { o: show(0.865, 0.878, 0.9, 0.91) },
  'p-act-2-panel': { y: [[0.865, 220], [0.882, 0]] },
  'p-delete': { s: press(0.89) },
  'tap-delete': tap(0.89),
  'p-draft': { h: [[0.91, 52], [0.93, 0]], o: [[0.91, 1], [0.925, 0]] },
  'l-draft-pulse': { o: [[0.93, 0], [0.937, 1], [0.95, 0.6]] },
  'l-draft': { h: [[0.945, 38], [0.97, 0]], o: [[0.945, 1], [0.965, 0]] },

  'dot-1': { o: [[0, 1], [0.3, 1], [0.32, 0.25]] },
  'dot-2': { o: [[0.3, 0.25], [0.32, 1], [0.62, 1], [0.64, 0.25]] },
  'dot-3': { o: [[0.62, 0.25], [0.64, 1]] }
}


// A QR code's look — finder squares and dots — not a code anything reads.
function Qr() {
  const n = 25
  const cells: [number, number][] = []
  let seed = 7
  const finder = (x: number, y: number) => [[0, 0], [n - 7, 0], [0, n - 7]].some(([fx, fy]) => x >= fx - 1 && x <= fx + 7 && y >= fy - 1 && y <= fy + 7)
  for (let y = 0; y < n; y++)
    for (let x = 0; x < n; x++) {
      seed = (seed * 1103515245 + 12345) % 2147483648
      if (!finder(x, y) && seed % 100 < 46) cells.push([x, y])
    }
  return (
    <svg viewBox={`0 0 ${n} ${n}`} className="wt-qr" aria-hidden="true">
      {[[0, 0], [n - 7, 0], [0, n - 7]].map(([x, y]) => (
        <g key={`${x}-${y}`}>
          <rect x={x + 0.5} y={y + 0.5} width="6" height="6" rx="1.6" fill="none" stroke="currentColor" strokeWidth="1" />
          <rect x={x + 2} y={y + 2} width="3" height="3" rx="0.8" fill="currentColor" />
        </g>
      ))}
      {cells.map(([x, y]) => (
        <circle key={`${x}-${y}`} cx={x + 0.5} cy={y + 0.5} r="0.42" fill="currentColor" />
      ))}
    </svg>
  )
}

const FOLDER =
  'M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z'
const FILE = 'M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM160,51.31,188.69,80H160ZM200,216H56V40h88V88a8,8,0,0,0,8,8h48V216Z'
const LOCK =
  'M208,80H176V56a48,48,0,0,0-96,0V80H48A16,16,0,0,0,32,96V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V96A16,16,0,0,0,208,80ZM96,56a32,32,0,0,1,64,0V80H96ZM208,208H48V96H208V208Z'
const LAPTOP =
  'M232,168h-8V72a24,24,0,0,0-24-24H56A24,24,0,0,0,32,72v96H24a8,8,0,0,0-8,8v16a24,24,0,0,0,24,24H216a24,24,0,0,0,24-24V176A8,8,0,0,0,232,168ZM48,72a8,8,0,0,1,8-8H200a8,8,0,0,1,8,8v96H48ZM224,192a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8v-8H224ZM152,88a8,8,0,0,1-8,8H112a8,8,0,0,1,0-16h32A8,8,0,0,1,152,88Z'
const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const ACTION_ICONS = {
  Rename: 'M227.31,73.37,182.63,28.68a16,16,0,0,0-22.63,0L36.69,152A15.86,15.86,0,0,0,32,163.31V208a16,16,0,0,0,16,16H92.69A15.86,15.86,0,0,0,104,219.31L227.31,96a16,16,0,0,0,0-22.63ZM92.69,208H48V163.31l88-88L180.69,120ZM192,108.68,147.31,64l24-24L216,84.68Z',
  Copy: 'M216,32H88a8,8,0,0,0-8,8V80H40a8,8,0,0,0-8,8V216a8,8,0,0,0,8,8H168a8,8,0,0,0,8-8V176h40a8,8,0,0,0,8-8V40A8,8,0,0,0,216,32ZM160,208H48V96H160Zm48-48H176V88a8,8,0,0,0-8-8H96V48H208Z',
  Move: 'M128,136v64a8,8,0,0,1-16,0V155.32L45.66,221.66a8,8,0,0,1-11.32-11.32L100.68,144H56a8,8,0,0,1,0-16h64A8,8,0,0,1,128,136ZM208,32H80A16,16,0,0,0,64,48V96a8,8,0,0,0,16,0V48H208V176H160a8,8,0,0,0,0,16h48a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Z',
  Delete: 'M216,48H176V40a24,24,0,0,0-24-24H104A24,24,0,0,0,80,40v8H40a8,8,0,0,0,0,16h8V208a16,16,0,0,0,16,16H192a16,16,0,0,0,16-16V64h8a8,8,0,0,0,0-16ZM96,40a8,8,0,0,1,8-8h48a8,8,0,0,1,8,8v8H96Zm96,168H64V64H192ZM112,104v64a8,8,0,0,1-16,0V104a8,8,0,0,1,16,0Zm48,0v64a8,8,0,0,1-16,0V104a8,8,0,0,1,16,0Z'
}
const Play = ({ size = 16 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    <path d="M7 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5Z" fill="currentColor" />
  </svg>
)
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

// A file's sheet, held: its name and the app's round action buttons. Each
// of the two sheets presses one button, Rename or Delete.
function Actions({ k, name, target }: { k: string; name: string; target: 'Rename' | 'Delete' }) {
  const pressKey = target === 'Rename' ? 'p-rename' : 'p-delete'
  const tapKey = target === 'Rename' ? 'tap-rename' : 'tap-delete'
  return (
    <div className="wt-layer wt-dim" data-k={k}>
      <div className="wt-panel wt-act" data-k={`${k}-panel`}>
        <span className="wt-grab" />
        <div className="wt-act-head">
          <span className="wt-tile">
            <Icon d={FILE} />
          </span>
          <b>{name}</b>
        </div>
        <div className="wt-round">
          {(['Rename', 'Copy', 'Move', 'Delete'] as const).map((label) => (
            <span key={label} className={label === 'Delete' ? 'wt-red' : ''}>
              <i data-k={label === target ? pressKey : undefined}>
                <Icon d={ACTION_ICONS[label]} size={18} />
              </i>
              {label}
            </span>
          ))}
        </div>
        <span className="wt-tap" data-k={tapKey} style={{ left: target === 'Rename' ? 20 : 167, top: 80 }} />
      </div>
    </div>
  )
}

// Each chapter is its own scene: the part of TIMELINE it plays, how long it
// takes, and its words.
const CHAPTERS = {
  connect: {
    range: [0.03, 0.3] as [number, number],
    duration: 6500,
    title: 'Scan once. That’s it.',
    line: 'Your phone and your computer recognise each other from then on.',
    soon: false
  },
  remote: {
    range: [0.33, 0.63] as [number, number],
    duration: 9500,
    title: 'Your computer, in your pocket.',
    line: 'Open its folders from your phone. Watch its videos, look through its photos, read its documents, as if you were sitting at it.',
    soon: true
  },
  change: {
    range: [0.655, 0.975] as [number, number],
    duration: 6500,
    title: 'Change it from your phone.',
    line: 'Rename, move, copy or delete. Your computer follows.',
    soon: false
  }
}

export function Walkthrough({ chapter }: { chapter: keyof typeof CHAPTERS }) {
  const root = useRef<HTMLElement>(null)
  const c = CHAPTERS[chapter]
  usePlayOnView(root, TIMELINE, c.duration, chapter, c.range, TALL_WALK)

  return (
    <section ref={root} className="scene wt-scene" id={chapter === 'connect' ? 'how' : undefined} aria-label={c.title}>
      <Rise lines={[c.title]} />
      <p>{c.line}</p>
      {c.soon && <span className="fm-test wt-soon">From anywhere · coming soon</span>}

      <div className="scene-room">
          <div className="scene-stage" data-w="1000" aria-hidden="true">
            {/* The laptop. */}
            <div className="wt-laptop">
              <div className="wt-screen">
                <div className="wt-layer wt-pairing" data-k="l-pair">
                  <div className="wt-card">
                    <div className="wt-layer wt-center" data-k="l-qr">
                      <b>Add a device</b>
                      <Qr />
                      <small>Scan with QurbSpace on your phone</small>
                    </div>
                    <div className="wt-layer wt-center" data-k="l-number">
                      <b>skib phone wants to join</b>
                      <span className="wt-number">482 913</span>
                      <small>Same number on your phone?</small>
                      <span className="wt-btn" data-k="l-approve">
                        Approve
                        <svg className="wt-cursor" data-k="l-cursor" width="22" height="22" viewBox="0 0 24 24">
                        <path d="M5 3l14 8-6 1.5L10 19z" fill="#171816" stroke="#fff" strokeWidth="1.5" strokeLinejoin="round" />
                        </svg>
                      </span>
                    </div>
                    <div className="wt-layer wt-center" data-k="l-ok">
                      <span className="wt-ok" data-k="l-ok-check">
                        <Icon d={CHECK} size={30} />
                      </span>
                      <b>skib phone is connected</b>
                    </div>
                  </div>
                </div>

                <div className="wt-layer wt-window" data-k="l-files">
                  <aside>
                    <b>QurbSpace</b>
                    <span>Home</span>
                    <span className="on">Files</span>
                    <span>Settings</span>
                  </aside>
                  <main>
                    <b className="wt-title">Private · skib phone</b>
                    <div className="wt-folders">
                      <span><Icon d={FOLDER} size={22} />Projects</span>
                      <span><Icon d={FOLDER} size={22} />Photos</span>
                    </div>
                    <div className="wt-row">
                      <span className="wt-pulse" data-k="l-notes-pulse" />
                      <Icon d={FILE} />
                      <span className="wt-swap">
                        <span data-k="l-notes-old">notes.txt</span>
                        <span data-k="l-notes-new" className="wt-new">ideas.txt</span>
                      </span>
                    </div>
                    <div className="wt-row wt-fold" data-k="l-draft">
                      <span className="wt-pulse wt-pulse-red" data-k="l-draft-pulse" />
                      <Icon d={FILE} />
                      old-draft.txt
                    </div>
                    <div className="wt-row">
                      <Icon d={FILE} />
                      receipt.pdf
                    </div>
                    <div className="wt-row">
                      <Play size={14} />
                      Holiday.mp4
                      <span className="wt-stream" data-k="l-stream">Playing on skib phone</span>
                    </div>
                    <div className="wt-row">
                      <span className="wt-mini-photo" />
                      Sunset.jpg
                    </div>
                  </main>
                </div>
              </div>
              <div className="wt-base" />
            </div>

            <svg className="wt-beam" data-k="beam" viewBox="0 0 1000 560">
              <path d="M 560 250 C 640 170 700 170 770 230" fill="none" stroke="#2F6B57" strokeOpacity="0.35" strokeWidth="2" strokeDasharray="1 9" strokeLinecap="round" />
            </svg>

            {/* The phone. */}
            <div className="wt-phone" data-k="phone">
              <div className="wt-pscreen">
                <div className="wt-layer wt-join" data-k="p-join">
                  <span className="wt-mark wt-join-mark"><Icon d={LAPTOP} size={30} /></span>
                  <b>Connect your computer</b>
                  <small>Open QurbSpace on your computer to show its code.</small>
                  <span className="wt-send wt-join-scan" data-k="p-scan-btn">Scan a code</span>
                  <span className="wt-join-type">Type 6 digits</span>
                  <span className="wt-tap" data-k="tap-scan" style={{ left: 94, top: 338 }} />
                </div>

                <div className="wt-layer wt-home" data-k="p-home">
                  <small className="wt-time">12:30</small>
                  <b className="wt-brand">QurbSpace</b>
                  <span className="wt-circle">L<i /></span>
                  <span className="wt-mark"><Icon d={CHECK} size={28} /></span>
                  <b className="wt-synced">Everything is synced.</b>
                  <span className="wt-send">Send</span>
                  <span className="wt-tap" data-k="tap-circle" style={{ left: 176, top: 36 }} />
                </div>

                <div className="wt-layer wt-cam" data-k="p-cam">
                  <div className="wt-finder">
                    <Qr />
                    <span className="wt-scan" data-k="p-scan" />
                    <span className="wt-found" data-k="p-found" />
                  </div>
                  <small className="wt-found-name" data-k="p-found-name">skib laptop</small>
                  <small>Point at the code</small>
                </div>

                <div className="wt-layer wt-center wt-pnum" data-k="p-number">
                  <small>Joining skib laptop</small>
                  <span className="wt-number">482 913</span>
                  <small>Approve on skib laptop</small>
                </div>

                <div className="wt-layer wt-center wt-pnum" data-k="p-ok">
                  <span className="wt-ok" data-k="p-ok-check"><Icon d={CHECK} size={26} /></span>
                  <b>Connected to skib laptop</b>
                </div>

                <div className="wt-layer wt-dim" data-k="p-sheet">
                  <div className="wt-panel wt-choose" data-k="p-sheet-panel">
                    <span className="wt-grab" />
                    <b>skib laptop</b>
                    <div className="wt-two">
                      <span><Icon d={FOLDER} size={22} />Shared</span>
                      <span data-k="p-private" className="wt-pick"><Icon d={LOCK} size={22} />Private</span>
                    </div>
                    <span className="wt-tap" data-k="tap-private" style={{ left: 145, top: 86 }} />
                  </div>
                </div>

                <div className="wt-layer wt-pfiles" data-k="p-files">
                  <b className="wt-back">‹ Private</b>
                  <div className="wt-pfolders">
                    <span><Icon d={FOLDER} size={20} />Projects</span>
                    <span><Icon d={FOLDER} size={20} />Photos</span>
                  </div>
                  <div className="wt-prow">
                    <span className="wt-tile"><Icon d={FILE} /></span>
                    <span className="wt-swap">
                      <span data-k="p-notes-old">notes.txt</span>
                      <span className="wt-type" data-k="p-notes-new">ideas.txt</span>
                    </span>
                    <span className="wt-tap" data-k="tap-notes" style={{ left: 80, top: 6 }} />
                  </div>
                  <div className="wt-prow wt-fold" data-k="p-draft">
                    <span className="wt-tile"><Icon d={FILE} /></span>
                    old-draft.txt
                    <span className="wt-tap" data-k="tap-draft" style={{ left: 80, top: 6 }} />
                  </div>
                  <div className="wt-prow">
                    <span className="wt-tile"><Icon d={FILE} /></span>
                    receipt.pdf
                    <span className="wt-tap" data-k="tap-pdf" style={{ left: 80, top: 6 }} />
                  </div>
                  <div className="wt-prow">
                    <span className="wt-tile wt-tile-video"><Play size={12} /></span>
                    Holiday.mp4
                    <small className="wt-size">1.6 GB</small>
                    <span className="wt-tap" data-k="tap-video" style={{ left: 80, top: 6 }} />
                  </div>
                  <div className="wt-prow">
                    <span className="wt-tile wt-tile-photo" />
                    Sunset.jpg
                    <span className="wt-tap" data-k="tap-photo" style={{ left: 80, top: 6 }} />
                  </div>
                </div>

                <div className="wt-layer wt-viewer wt-video" data-k="p-video">
                  <b className="wt-back">‹ Holiday.mp4</b>
                  <div className="wt-frame">
                    {/* Ocean surface waves, by Mostafameraji, CC0, from Wikimedia
                        Commons; made small and silent in public/media. */}
                    <video autoPlay muted loop playsInline preload="auto" poster="/media/waves.jpg">
                      <source src="/media/waves.webm" type="video/webm" />
                      <source src="/media/waves.mp4" type="video/mp4" />
                    </video>
                    <span className="wt-vplay" data-k="p-vplay"><Play size={22} /></span>
                  </div>
                  <div className="wt-vbar"><i data-k="p-vbar" /></div>
                  <small>Playing from skib laptop</small>
                </div>
                <div className="wt-layer wt-viewer" data-k="p-photo">
                  <b className="wt-back">‹ Sunset.jpg</b>
                  <div className="wt-photo" />
                  <small>6.1 MB · from skib laptop</small>
                </div>
                <div className="wt-layer wt-viewer" data-k="p-pdf">
                  <b className="wt-back">‹ receipt.pdf</b>
                  <div className="wt-page">
                    <b>Receipt</b>
                    <i /><i /><i /><i className="wt-short" />
                    <span>Total <b>£42.00</b></span>
                  </div>
                  <small>Page 1 of 2</small>
                </div>

                <Actions k="p-act-1" name="notes.txt" target="Rename" />
                <Actions k="p-act-2" name="old-draft.txt" target="Delete" />
              </div>
            </div>
          </div>
      </div>
    </section>
  )
}
