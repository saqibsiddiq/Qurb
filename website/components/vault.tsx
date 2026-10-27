'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView } from './play-on-view'
import { TALL_PAIR } from './tall'
import { press, show, tap, type Keys } from './timeline'

// Scene 5 of the website brief (docs/design/website-brief.md): the private
// vault. The laptop asks to open this phone's folder; the phone asks *Open
// your folder on skib laptop?*, the thumb confirms, and the folder opens on
// the laptop, to lock itself again after ten minutes unused. On the owner's
// own computer that is QurbSpace's rule rather than encryption — the computer
// holds the same key (docs/features.md) — so the words say it opens only when
// you say yes, not that nothing else could read it.

const DURATION = 9500

const TIMELINE: Record<string, Keys> = {
  'l-cursor': { o: show(0.03, 0.06, 0.12, 0.15), x: [[0.03, 90], [0.09, 0]], y: [[0.03, 70], [0.09, 0]] },
  'l-open': { s: press(0.1) },
  'l-asking': { o: show(0.12, 0.14, 0.58, 0.6) },
  'p-sheet': { o: show(0.15, 0.17, 0.32, 0.34) },
  'p-sheet-panel': { y: [[0.15, 260], [0.18, 0]] },
  'p-open': { s: press(0.28) },
  'tap-open': tap(0.28),
  'p-finger': { o: show(0.33, 0.35, 0.53, 0.55) },
  'p-finger-panel': { y: [[0.33, 200], [0.36, 0]] },
  'tap-finger': { o: [[0.41, 0], [0.42, 0.45], [0.47, 0]], s: [[0.41, 0.4], [0.47, 1.8]] },
  'p-finger-on': { o: [[0.42, 0], [0.49, 1]] },
  'p-finger-ok': { o: [[0.49, 0], [0.51, 1]], s: [[0.49, 0.5], [0.52, 1]] },
  'p-opened': { o: show(0.55, 0.58, 0.9, 0.93), y: [[0.55, 10], [0.58, 0]] },
  'l-shackle': { y: [[0.58, 0], [0.64, -9]], r: [[0.62, 0], [0.66, -18]] },
  'l-locked': { o: [[0, 1], [0.62, 1], [0.66, 0]] },
  'l-files': { o: [[0.64, 0], [0.7, 1]], y: [[0.64, 10], [0.7, 0]] },
  'l-timer': { o: show(0.74, 0.78), y: [[0.74, 8], [0.78, 0]] },
  'l-ring': { dash: [[0.78, 0], [1, 0.18]] }
}

const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const FILE = 'M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM160,51.31,188.69,80H160ZM200,216H56V40h88V88a8,8,0,0,0,8,8h48V216Z'
const FOLDER =
  'M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z'
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

// A lock whose shackle can lift away from its body.
function Lock({ size = 56, k }: { size?: number; k?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" className="vt-lock">
      <path data-k={k} className="vt-shackle" d="M20 30 V20 a12 12 0 0 1 24 0 V30" fill="none" stroke="currentColor" strokeWidth="5" strokeLinecap="round" />
      <rect x="12" y="28" width="40" height="30" rx="8" fill="currentColor" />
      <circle cx="32" cy="42" r="4" fill="#fff" />
    </svg>
  )
}

// Concentric arcs, drawn the way a fingerprint prompt draws one.
function Fingerprint({ k }: { k?: string }) {
  const arcs = ['M20 44 C 18 28 26 18 32 18 C 40 18 46 26 45 38', 'M26 48 C 23 36 26 25 32 25 C 38 25 40 32 39 40', 'M32 50 C 30 42 30 34 32 32 C 35 33 34 40 36 46', 'M14 36 C 14 20 24 11 32 11 C 42 11 51 20 51 32', 'M40 50 C 42 46 45 44 46 40']
  return (
    <svg data-k={k} width="64" height="64" viewBox="0 0 64 64" aria-hidden="true">
      {arcs.map((d) => (
        <path key={d} d={d} fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
      ))}
    </svg>
  )
}

export function Vault() {
  const root = useRef<HTMLElement>(null)
  usePlayOnView(root, TIMELINE, DURATION, 'vt', [0, 1], TALL_PAIR)

  return (
    <section ref={root} className="scene vt room-dark" aria-label="The private vault">
      <Rise lines={['A folder only your phone can open.']} />
      <p>Your computer keeps it, but it opens only when you say yes on your phone.</p>

      <div className="scene-room">
        <div className="scene-stage" data-w="1240" aria-hidden="true">
          <div className="wt-phone sp-phone">
            <div className="wt-pscreen">
              <div className="wt-layer wt-home" style={{ opacity: 1 }}>
                <small className="wt-time">12:30</small>
                <b className="wt-brand">QurbSpace</b>
                <span className="wt-circle">L<i /></span>
                <span className="wt-mark"><Icon d={CHECK} size={28} /></span>
                <b className="wt-synced">Everything is synced.</b>
                <span className="wt-send">Send</span>
                <span className="sp-sent vt-opened" data-k="p-opened"><Icon d={CHECK} size={12} />Opened on skib laptop</span>
              </div>

              <div className="wt-layer wt-dim" data-k="p-sheet">
                <div className="wt-panel vt-ask" data-k="p-sheet-panel">
                  <span className="wt-grab" />
                  <span className="vt-ask-mark"><Lock size={30} /></span>
                  <b>Open your folder on skib laptop?</b>
                  <small>It locks itself after 10 minutes unused.</small>
                  <span className="vt-chip">Only if you asked at skib laptop</span>
                  <div className="vt-two">
                    <span>Not now</span>
                    <span className="vt-yes" data-k="p-open">Open</span>
                  </div>
                  <span className="wt-tap" data-k="tap-open" style={{ left: 142, top: 214 }} />
                </div>
              </div>

              <div className="wt-layer wt-dim" data-k="p-finger">
                <div className="wt-panel vt-finger" data-k="p-finger-panel">
                  <b>Confirm it’s you</b>
                  <small>Touch the sensor</small>
                  <span className="vt-print">
                    <Fingerprint />
                    <span className="vt-print-on" data-k="p-finger-on"><Fingerprint /></span>
                    <span className="vt-print-ok" data-k="p-finger-ok"><Icon d={CHECK} size={26} /></span>
                  </span>
                  <span className="wt-tap vt-thumb" data-k="tap-finger" />
                </div>
              </div>
            </div>
          </div>

          <div className="sp-laptop">
            <div className="wt-screen">
              <div className="wt-layer sp-window vt-window" style={{ opacity: 1 }}>
                <b className="sp-title">Your folder from skib phone</b>
                <div className="vt-locked" data-k="l-locked">
                  <span className="vt-lock-wrap"><Lock k="l-shackle" /></span>
                  <small>Locked</small>
                  <span className="wt-btn vt-open-btn" data-k="l-open">
                    Open
                    <svg className="wt-cursor" data-k="l-cursor" width="22" height="22" viewBox="0 0 24 24">
                      <path d="M5 3l14 8-6 1.5L10 19z" fill="#171816" stroke="#fff" strokeWidth="1.5" strokeLinejoin="round" />
                    </svg>
                  </span>
                  <small className="vt-asking" data-k="l-asking">Asking skib phone…</small>
                </div>
                <div className="vt-files" data-k="l-files">
                  <div className="wt-folders">
                    <span><Icon d={FOLDER} size={22} />Photos</span>
                    <span><Icon d={FOLDER} size={22} />Documents</span>
                  </div>
                  <div className="sp-row"><Icon d={FILE} />Passport scan.pdf</div>
                  <div className="sp-row"><Icon d={FILE} />Bank letter.pdf</div>
                </div>
                <div className="vt-timer" data-k="l-timer">
                  <svg width="18" height="18" viewBox="0 0 20 20">
                    <circle cx="10" cy="10" r="8" fill="none" stroke="#e2e3dd" strokeWidth="2.5" />
                    <circle data-k="l-ring" cx="10" cy="10" r="8" fill="none" stroke="#2f6b57" strokeWidth="2.5" pathLength="1" strokeDasharray="1" strokeLinecap="round" transform="rotate(-90 10 10)" />
                  </svg>
                  Locks itself after 10 minutes unused
                </div>
              </div>
            </div>
            <div className="wt-base" />
          </div>
        </div>
      </div>
    </section>
  )
}
