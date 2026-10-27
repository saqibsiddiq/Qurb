'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView, type Tall } from './play-on-view'
import { press, show, tap, type Keys } from './timeline'

// The owner's addition to the brief, in two halves. One computer for several
// people: each keeps a private folder there, locked to their own phone, and
// a Shared folder is open to all of them (decisions 0060, 0067, 0073). One
// phone with several computers: hold the circle on Home, choose another.
// The words keep two things apart, as docs/features.md does: everyone's
// folder opens only from their own phone, but only a guest's is sealed —
// the owner's computer holds the owner's key. Guests are checked on the
// emulator, not yet on a real phone, so they are marked as being tested.

const DURATION = 12000

// On a phone: the people in a row over the computer, then the computers over
// the phone (preview.css). Nothing here flies free between two devices, so
// the drops have tall routes of their own instead of bending.
const TALL_FAMILY: Tall = { w: 640, h: 890, a: { x: 0, d: [0, 0] }, b: { x: 1, d: [0, 0] } }

const TIMELINE: Record<string, Keys> = {
  // One computer, several people.
  'cap-1': { o: show(0, 0.03, 0.5, 0.54) },
  family: { o: [[0, 1], [0.5, 1], [0.55, 0]], s: [[0.5, 1], [0.55, 0.96]] },
  'pp-1': { o: [[0.03, 0], [0.07, 1]], s: [[0.03, 0.6], [0.08, 1]] },
  'pp-2': { o: [[0.07, 0], [0.11, 1]], s: [[0.07, 0.6], [0.12, 1]] },
  'pp-3': { o: [[0.11, 0], [0.15, 1]], s: [[0.11, 0.6], [0.16, 1]] },
  'ln-1': { dash: [[0.07, 1], [0.13, 0]] },
  'ln-2': { dash: [[0.11, 1], [0.17, 0]] },
  'ln-3': { dash: [[0.15, 1], [0.21, 0]] },
  'f-1': { o: [[0.12, 0], [0.15, 1]] },
  'f-2': { o: [[0.16, 0], [0.19, 1]] },
  'f-3': { o: [[0.2, 0], [0.23, 1]] },
  'f-shared': { o: [[0.24, 0], [0.27, 1]], s: [[0.24, 0.9], [0.27, 1]] },
  'drop-1': {
    o: [[0.29, 0], [0.3, 1], [0.35, 1], [0.36, 0]],
    path: { t0: 0.29, t1: 0.355, facing: 0, bank: 0, points: [[200, 150], [340, 120], [520, 260], [716, 316]] },
    tall: { path: { t0: 0.29, t1: 0.355, facing: 0, bank: 0, points: [[92, 40], [200, 100], [380, 300], [440, 398]] } }
  },
  'drop-2': {
    o: [[0.33, 0], [0.34, 1], [0.39, 1], [0.4, 0]],
    path: { t0: 0.33, t1: 0.395, facing: 0, bank: 0, points: [[1030, 250], [900, 230], [880, 300], [770, 316]] },
    tall: { path: { t0: 0.33, t1: 0.395, facing: 0, bank: 0, points: [[320, 40], [360, 130], [430, 300], [452, 398]] } }
  },
  'drop-3': {
    o: [[0.37, 0], [0.38, 1], [0.43, 1], [0.44, 0]],
    path: { t0: 0.37, t1: 0.435, facing: 0, bank: 0, points: [[200, 380], [330, 420], [560, 380], [740, 330]] },
    tall: { path: { t0: 0.37, t1: 0.435, facing: 0, bank: 0, points: [[548, 40], [560, 150], [500, 320], [446, 400]] } }
  },
  'shared-pulse': { o: [[0.355, 0], [0.365, 1], [0.39, 0.3], [0.4, 1], [0.43, 0.3], [0.44, 1], [0.48, 0]] },
  'shared-n': { o: [[0, 1], [0.355, 1], [0.36, 0]] },
  'shared-n2': { o: [[0.355, 0], [0.36, 1], [0.44, 1], [0.445, 0]] },
  'shared-n3': { o: [[0.44, 0], [0.445, 1]] },

  // One phone, several computers.
  'cap-2': { o: show(0.54, 0.58) },
  phone: { o: [[0.54, 0], [0.58, 1]], x: [[0.54, -30], [0.58, 0]] },
  'pc-1': { o: [[0.56, 0], [0.6, 1]] },
  'pc-2': { o: [[0.58, 0], [0.62, 1]] },
  'hold-circle': { o: [[0.64, 0], [0.645, 0.5], [0.69, 0.5], [0.7, 0]], s: [[0.64, 0.4], [0.69, 1.6]] },
  'p-list': { o: show(0.68, 0.7, 0.82, 0.84) },
  'p-list-panel': { y: [[0.68, 240], [0.71, 0]] },
  'p-office': { s: press(0.78) },
  'tap-office': tap(0.78),
  'c-home': { o: [[0, 1], [0.82, 1], [0.83, 0]] },
  'c-office': { o: [[0.82, 0], [0.83, 1]] },
  'link-home': { o: [[0.6, 0], [0.63, 1], [0.82, 1], [0.86, 0]] },
  'link-office': { o: [[0.83, 0], [0.87, 1]], dash: [[0.83, 1], [0.9, 0]] },
  'pc-1-on': { o: [[0.6, 0], [0.63, 1], [0.82, 1], [0.85, 0]] },
  'pc-2-on': { o: [[0.85, 0], [0.88, 1]] }
}

const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const FOLDER =
  'M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z'
const LOCK =
  'M208,80H176V56a48,48,0,0,0-96,0V80H48A16,16,0,0,0,32,96V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V96A16,16,0,0,0,208,80ZM96,56a32,32,0,0,1,64,0V80H96ZM208,208H48V96H208V208Z'
const LAPTOP =
  'M232,168h-8V72a24,24,0,0,0-24-24H56A24,24,0,0,0,32,72v96H24a8,8,0,0,0-8,8v16a24,24,0,0,0,24,24H216a24,24,0,0,0,24-24V176A8,8,0,0,0,232,168ZM48,72a8,8,0,0,1,8-8H200a8,8,0,0,1,8,8v96H48ZM224,192a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8v-8H224ZM152,88a8,8,0,0,1-8,8H112a8,8,0,0,1,0-16h32A8,8,0,0,1,152,88Z'
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

const PEOPLE = [
  { k: 'pp-1', initial: 'S', name: 'You', at: { left: 120, top: 110 } },
  { k: 'pp-2', initial: 'A', name: 'Ammi', at: { left: 1040, top: 210 } },
  { k: 'pp-3', initial: 'Z', name: 'Zara', at: { left: 120, top: 340 } }
]

export function Family() {
  const root = useRef<HTMLElement>(null)
  usePlayOnView(root, TIMELINE, DURATION, 'fm', [0, 1], TALL_FAMILY)

  return (
    <section ref={root} className="scene fm scene-split" aria-label="One computer for the family, one phone for all your computers">
      <Rise lines={['One computer for everyone.', 'One phone for every computer.']} />
      <div className="fm-caps">
        <p data-k="cap-1">
          Everyone who uses your computer gets a private folder, locked to their own phone, and a Shared folder for all of you.
          <span className="fm-test">Several people on one computer · being tested</span>
        </p>
        <p data-k="cap-2">Home and work: your phone keeps every computer you use. Hold the circle to switch.</p>
      </div>

      <div className="scene-room">
        <div className="scene-stage" data-w="1240" aria-hidden="true">
          {/* One computer, several people */}
          <div className="fm-family" data-k="family">
            <svg className="fm-lines" viewBox="0 0 1240 540">
              <path data-k="ln-1" d="M 196 148 C 300 140 360 160 420 190" pathLength="1" strokeDasharray="1" />
              <path data-k="ln-2" d="M 1044 248 C 980 240 900 200 820 190" pathLength="1" strokeDasharray="1" />
              <path data-k="ln-3" d="M 196 378 C 300 360 340 300 420 280" pathLength="1" strokeDasharray="1" />
            </svg>
            <div className="sp-laptop fm-laptop">
              <div className="wt-screen">
                <div className="wt-layer sp-window" style={{ opacity: 1 }}>
                  <b className="sp-title">skib laptop</b>
                  <div className="fm-grid">
                    {[
                      ['f-1', 'You'],
                      ['f-2', 'Ammi'],
                      ['f-3', 'Zara']
                    ].map(([k, who]) => (
                      <span key={k} data-k={k} className="fm-own">
                        <span className="fm-lock"><Icon d={LOCK} size={14} /></span>
                        <Icon d={FOLDER} size={24} />
                        <b>{who}</b>
                        <small>Private</small>
                      </span>
                    ))}
                    <span data-k="f-shared" className="fm-shared">
                      <span className="wt-pulse" data-k="shared-pulse" />
                      <Icon d={FOLDER} size={24} />
                      <b>Shared</b>
                      <small className="wt-swap">
                        <span data-k="shared-n">12 files</span>
                        <span data-k="shared-n2" className="bk-n">13 files</span>
                        <span data-k="shared-n3" className="bk-n">15 files</span>
                      </small>
                    </span>
                  </div>
                </div>
              </div>
              <div className="wt-base" />
            </div>
            {PEOPLE.map((p) => (
              <span key={p.k} className="fm-person" data-k={p.k} style={p.at}>
                <span className="wt-circle fm-circle">{p.initial}<i /></span>
                <small>{p.name}</small>
              </span>
            ))}
            {[1, 2, 3].map((n) => (
              <span key={n} className="bk-dot fm-drop" data-k={`drop-${n}`} />
            ))}
          </div>

          {/* One phone, several computers */}
          <div className="wt-phone sp-phone fm-phone" data-k="phone">
            <div className="wt-pscreen">
              <div className="wt-layer wt-home" style={{ opacity: 1 }}>
                <small className="wt-time">12:30</small>
                <b className="wt-brand">QurbSpace</b>
                <span className="wt-circle">
                  <span className="fm-letter" data-k="c-home">L</span>
                  <span className="fm-letter" data-k="c-office">O</span>
                  <i />
                </span>
                <span className="wt-mark"><Icon d={CHECK} size={28} /></span>
                <b className="wt-synced">Everything is synced.</b>
                <span className="wt-send">Send</span>
                <span className="wt-tap" data-k="hold-circle" style={{ left: 175, top: 33 }} />
              </div>
              <div className="wt-layer wt-dim" data-k="p-list">
                <div className="wt-panel fm-list" data-k="p-list-panel">
                  <span className="wt-grab" />
                  <b>Your computers</b>
                  <span className="fm-row on"><span className="wt-circle fm-circle">L<i /></span>skib laptop<small>Home</small></span>
                  <span className="fm-row" data-k="p-office"><span className="wt-circle fm-circle">O<i /></span>Office desktop<small>Work</small></span>
                  <span className="fm-row fm-add">+ Add a computer</span>
                  <span className="wt-tap" data-k="tap-office" style={{ left: 90, top: 98 }} />
                </div>
              </div>
            </div>
          </div>
          <svg className="fm-lines fm-links" viewBox="0 0 1240 540">
            <path data-k="link-home" d="M 300 240 C 460 170 600 150 700 160" />
            <path data-k="link-office" d="M 300 280 C 460 330 600 380 700 380" pathLength="1" strokeDasharray="1" />
          </svg>
          {[
            ['pc-1', 'skib laptop', 'Home', 110],
            ['pc-2', 'Office desktop', 'Work', 330]
          ].map(([k, name, where, top]) => (
            <span key={k} className="fm-pc" data-k={k} style={{ top: Number(top) }}>
              <span className="fm-pc-on" data-k={`${k}-on`} />
              <Icon d={LAPTOP} size={40} />
              <span>
                <b>{name}</b>
                <small>{where}</small>
              </span>
            </span>
          ))}
        </div>
      </div>
    </section>
  )
}
