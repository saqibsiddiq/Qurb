'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView } from './play-on-view'
import { TALL_PAIR } from './tall'
import { show, type Keys } from './timeline'

// Scene 4 of the website brief (docs/design/website-brief.md): nobody in the
// middle. A cloud, and the way files usually go — up into it and down again —
// is drawn, then fades away; the file goes straight from the phone to the
// laptop instead, locked on the way and unlocked on arrival. True as
// docs/features.md has it: files travel encrypted, device to device, and the
// rendezvous service that introduces the devices never sees a file or a name.

const DURATION = 9000

const TIMELINE: Record<string, Keys> = {
  cloud: { o: [[0, 0], [0.1, 1], [0.34, 1], [0.46, 0]], s: [[0, 0.94], [0.1, 1], [0.34, 1], [0.46, 0.9]], y: [[0, 10], [0.1, 0], [0.34, 0], [0.46, -12]] },
  'cloud-up': { dash: [[0.1, 1], [0.2, 0]], o: [[0.1, 1], [0.32, 1], [0.42, 0]] },
  'cloud-down': { dash: [[0.18, 1], [0.28, 0]], o: [[0.18, 1], [0.32, 1], [0.42, 0]] },
  'cloud-label': { o: show(0.08, 0.14, 0.32, 0.4) },
  direct: { dash: [[0.44, 1], [0.6, 0]], o: [[0.44, 1], [1, 1]] },
  file: {
    o: [[0.5, 0], [0.54, 1], [0.86, 1], [0.9, 0]],
    path: { t0: 0.5, t1: 0.86, facing: 0, bank: 0.25, points: [[200, 300], [310, 372], [450, 400], [580, 372], [660, 262], [672, 150]] }
  },
  'file-lock': { o: [[0.5, 1], [0.8, 1], [0.84, 0]] },
  'file-open': { o: [[0.8, 0], [0.84, 1]] },
  'l-new': { h: [[0.86, 0], [0.9, 44]], o: [[0.86, 0], [0.9, 1]] },
  'l-new-pulse': { o: [[0.88, 0], [0.9, 1], [0.97, 0]] }
}

const LOCK =
  'M208,80H176V56a48,48,0,0,0-96,0V80H48A16,16,0,0,0,32,96V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V96A16,16,0,0,0,208,80ZM96,56a32,32,0,0,1,64,0V80H96ZM208,208H48V96H208V208Z'
const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const FILE = 'M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM160,51.31,188.69,80H160ZM200,216H56V40h88V88a8,8,0,0,0,8,8h48V216Z'
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

export function NoMiddle() {
  const root = useRef<HTMLElement>(null)
  usePlayOnView(root, TIMELINE, DURATION, 'nm', [0, 1], TALL_PAIR)

  return (
    <section ref={root} className="scene nm room-dark" aria-label="Nobody in the middle">
      <Rise lines={['Nobody in the middle.']} />
      <p>Your files go straight from your phone to your computer, locked on the way. No company keeps a copy.</p>

      <div className="scene-room nm-room">
        <div className="scene-stage" data-w="1240" aria-hidden="true">
          <svg className="nm-lines" viewBox="0 0 1240 520">
            <g data-k="cloud" className="nm-cloud">
              <path
                d="M 378 128 C 342 128 330 92 358 78 C 354 46 396 34 416 56 C 430 22 490 24 498 62 C 530 50 560 78 542 104 C 566 116 554 128 530 128 Z"
                fill="#fff"
                stroke="#b9bcb4"
                strokeWidth="2"
                strokeDasharray="6 7"
                strokeLinejoin="round"
              />
            </g>
            <path data-k="cloud-up" d="M 296 230 C 316 160 330 112 350 100" pathLength="1" strokeDasharray="1" fill="none" stroke="#b9bcb4" strokeWidth="2" strokeLinecap="round" />
            <path data-k="cloud-down" d="M 542 104 C 590 104 618 124 640 160" pathLength="1" strokeDasharray="1" fill="none" stroke="#b9bcb4" strokeWidth="2" strokeLinecap="round" />
            <path data-k="direct" d="M 300 330 C 420 410 560 420 650 330" pathLength="1" strokeDasharray="1" fill="none" stroke="#2f6b57" strokeOpacity="0.45" strokeWidth="2.5" strokeLinecap="round" />
          </svg>
          <span className="nm-cloud-label" data-k="cloud-label">A company’s servers</span>

          <div className="wt-phone sp-phone">
            <div className="wt-pscreen">
              <div className="wt-layer wt-home" style={{ opacity: 1 }}>
                <small className="wt-time">12:30</small>
                <b className="wt-brand">QurbSpace</b>
                <span className="wt-circle">L<i /></span>
                <span className="wt-mark"><Icon d={CHECK} size={28} /></span>
                <b className="wt-synced">Everything is synced.</b>
                <span className="wt-send">Send</span>
              </div>
            </div>
          </div>

          <div className="sp-laptop">
            <div className="wt-screen">
              <div className="wt-layer sp-window" style={{ opacity: 1 }}>
                <b className="sp-title">Downloads</b>
                <div className="sp-row sp-fold" data-k="l-new">
                  <span className="wt-pulse" data-k="l-new-pulse" />
                  <Icon d={FILE} />
                  Passport scan.pdf
                </div>
                <div className="sp-row"><Icon d={FILE} />Tickets.pdf</div>
                <div className="sp-row"><Icon d={FILE} />Thesis draft.pdf</div>
              </div>
            </div>
            <div className="wt-base" />
          </div>

          <span className="nm-file" data-free data-k="file">
            <span className="nm-file-icon">
              <span data-k="file-lock"><Icon d={LOCK} size={14} /></span>
              <span data-k="file-open" className="nm-open"><Icon d={CHECK} size={14} /></span>
            </span>
            Passport scan.pdf
          </span>
        </div>
      </div>
    </section>
  )
}
