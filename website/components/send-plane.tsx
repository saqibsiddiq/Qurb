'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView } from './play-on-view'
import { TALL_PAIR } from './tall'
import { press, show, tap, type Keys } from './timeline'

// Scene 3 of the website brief (docs/design/website-brief.md): sending,
// which works both ways. A photo leaves the phone, folds into a paper dart
// and lands on the laptop; then a PDF leaves the laptop, folds into a
// different plane — a broader glider, with a loop on the way — and lands on
// the phone. It plays once, by the clock, when it comes into view, rests on
// the last frame, and plays again when clicked; with reduced motion it shows that frame still.

const DURATION = 11500

const TIMELINE: Record<string, Keys> = {
  // There: the phone sends a photo.
  'tap-send': tap(0.05),
  's-send': { s: press(0.05) },
  's-pick': { o: show(0.07, 0.09, 0.2, 0.22) },
  'tap-thumb': tap(0.125),
  's-check': { o: [[0.13, 0], [0.14, 1]], s: [[0.13, 0.4], [0.15, 1]] },
  's-to': { o: show(0.16, 0.175, 0.225, 0.24) },
  's-to-panel': { y: [[0.16, 200], [0.18, 0]] },
  'tap-device': tap(0.205),
  'photo-1': {
    o: [[0.235, 0], [0.245, 1], [0.285, 1], [0.3, 0]],
    x: [[0.235, 100], [0.265, 100]],
    y: [[0.235, 230], [0.265, 190]],
    s: [[0.235, 0.4], [0.265, 1], [0.285, 1], [0.3, 0.3]],
    r: [[0.265, 0], [0.3, -25]]
  },

  'plane-1': {
    o: [[0.285, 0], [0.3, 1], [0.44, 1], [0.46, 0]],
    path: { t0: 0.285, t1: 0.455, facing: -39, points: [[112, 190], [200, 100], [360, 36], [540, 16], [690, 48], [760, 118]] },
    s: [[0.285, 0.3], [0.31, 1], [0.43, 1], [0.46, 0.4]]
  },

  's-sent': { o: show(0.42, 0.44, 0.54, 0.56), y: [[0.42, 12], [0.44, 0]] },
  'l-new': { h: [[0.44, 0], [0.47, 44]], o: [[0.44, 0], [0.47, 1]] },
  'l-new-pulse': { o: [[0.46, 0], [0.47, 1], [0.53, 0]] },
  'l-toast': { o: show(0.45, 0.47, 0.56, 0.58), x: [[0.45, 30], [0.47, 0]] },

  // And back: the laptop sends a PDF.
  'l-cursor': { o: show(0.57, 0.59, 0.67, 0.69), x: [[0.57, -120], [0.61, 0], [0.64, 0], [0.655, -80]], y: [[0.57, 90], [0.61, 0], [0.64, 0], [0.655, 56]] },
  'l-send-btn': { o: [[0.585, 0], [0.6, 1], [0.69, 1], [0.7, 0]], s: press(0.615) },
  'l-menu': { o: show(0.625, 0.635, 0.67, 0.68), y: [[0.625, -6], [0.635, 0]] },
  'l-menu-pick': { s: press(0.66) },
  'doc-1': {
    o: [[0.68, 0], [0.69, 1], [0.715, 1], [0.73, 0]],
    x: [[0.68, 1040], [0.705, 1030]],
    y: [[0.68, 186], [0.705, 150]],
    s: [[0.68, 0.4], [0.705, 1], [0.715, 1], [0.73, 0.3]],
    r: [[0.705, 0], [0.73, 20]]
  },

  'plane-2': {
    o: [[0.715, 0], [0.73, 1], [0.9, 1], [0.915, 0]],
    path: {
      t0: 0.715,
      t1: 0.912,
      facing: 187,
      points: [[1020, 150], [1050, 270], [990, 400], [880, 466], [780, 478], [700, 456], [662, 400], [690, 348], [748, 352], [768, 410], [720, 468], [620, 482], [480, 444], [330, 362], [200, 272], [110, 220]]
    },
    s: [[0.715, 0.3], [0.74, 1], [0.895, 1], [0.915, 0.4]]
  },

  's-banner': { o: show(0.905, 0.925), y: [[0.905, -70], [0.935, 0]] }
}

// The photo that travels: a frame of the ocean clip (public/media).
const PHOTO = 'center / cover url(/media/waves.jpg)'
const FILE = 'M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM160,51.31,188.69,80H160ZM200,216H56V40h88V88a8,8,0,0,0,8,8h48V216Z'
const SEND = 'M227.32,28.68a16,16,0,0,0-15.66-4.08l-.15,0L19.57,82.84a16,16,0,0,0-2.49,29.8L102,154l41.3,84.87A15.86,15.86,0,0,0,157.74,248q.69,0,1.38-.06a15.88,15.88,0,0,0,14-11.51l58.2-191.94c0-.05,0-.1,0-.15A16,16,0,0,0,227.32,28.68ZM157.83,231.85l-.05.14,0-.07-40.06-82.3,48-48a8,8,0,0,0-11.31-11.31l-48,48L24.08,98.25l-.07,0,.14,0L216,40Z'
const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

// The photo's plane: a slim dart, its wings still the photo.
function Dart() {
  return (
    <svg width="96" height="96" viewBox="0 0 96 96" aria-hidden="true">
      <defs>
        <linearGradient id="sp-sky" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#f3c3a0" />
          <stop offset="1" stopColor="#f6dcc0" />
        </linearGradient>
        <linearGradient id="sp-lake" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#9cc3bb" />
          <stop offset="1" stopColor="#5d8d86" />
        </linearGradient>
      </defs>
      <path d="M6 46 L90 14 L44 52 Z" fill="url(#sp-sky)" />
      <path d="M44 52 L90 14 L54 82 Z" fill="url(#sp-lake)" />
      <path d="M44 52 L90 14" stroke="#fff" strokeOpacity="0.7" strokeWidth="1.5" />
      <path d="M44 52 L50 66 L54 82" fill="none" stroke="#173329" strokeOpacity="0.15" strokeWidth="1.2" />
    </svg>
  )
}

// The PDF's plane: a broader glider, white paper with its lines and a green
// edge, facing the other way.
function Glider() {
  return (
    <svg width="104" height="80" viewBox="0 0 104 80" aria-hidden="true">
      <path d="M4 34 L98 8 L62 44 Z" fill="#fff" stroke="#2f6b57" strokeWidth="1.6" strokeLinejoin="round" />
      <path d="M4 34 L62 44 L96 70 Z" fill="#e3eee8" stroke="#2f6b57" strokeWidth="1.6" strokeLinejoin="round" />
      <path d="M62 44 L70 52 L96 70" fill="none" stroke="#2f6b57" strokeOpacity="0.5" strokeWidth="1.2" />
      <path d="M36 28 L76 18 M44 32 L80 23 M52 36 L76 30" stroke="#2f6b57" strokeOpacity="0.35" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  )
}

export function SendPlane() {
  const root = useRef<HTMLElement>(null)

  usePlayOnView(root, TIMELINE, DURATION, 'sp', [0, 1], TALL_PAIR)

  return (
    <section ref={root} className="sp" aria-label="Sending, both ways">
      <Rise lines={['From your hand to your desk.']} />
      <p>And back again.</p>

      <div className="scene-room sp-room">
        <div className="scene-stage sp-stage" data-w="1240" aria-hidden="true">
          {/* The phone */}
          <div className="wt-phone sp-phone">
            <div className="wt-pscreen">
              <div className="wt-layer wt-home sp-home" data-k="s-home">
                <small className="wt-time">12:30</small>
                <b className="wt-brand">QurbSpace</b>
                <span className="wt-circle">L<i /></span>
                <span className="wt-mark"><Icon d={CHECK} size={28} /></span>
                <b className="wt-synced">Everything is synced.</b>
                <span className="wt-send" data-k="s-send">Send</span>
                <span className="wt-tap" data-k="tap-send" style={{ left: 94, top: 293 }} />
                <span className="sp-sent" data-k="s-sent"><Icon d={CHECK} size={12} />Sent to skib laptop</span>
              </div>

              <div className="wt-layer sp-pick" data-k="s-pick">
                <b>Choose</b>
                <div className="sp-thumbs">
                  {Array.from({ length: 9 }, (_, i) => (
                    <span key={i} className={`sp-thumb sp-thumb-${i}`}>
                      {i === 0 && <i data-k="s-check"><Icon d={CHECK} size={10} /></i>}
                    </span>
                  ))}
                </div>
                <span className="wt-tap" data-k="tap-thumb" style={{ left: 26, top: 86 }} />
              </div>

              <div className="wt-layer wt-dim" data-k="s-to">
                <div className="wt-panel sp-to" data-k="s-to-panel">
                  <span className="wt-grab" />
                  <b>Send to</b>
                  <div className="sp-device">
                    <span className="wt-circle sp-circle">L<i /></span>
                    <span>
                      skib laptop
                      <small>On</small>
                    </span>
                  </div>
                  <span className="wt-tap" data-k="tap-device" style={{ left: 30, top: 66 }} />
                </div>
              </div>

              <div className="sp-banner" data-k="s-banner">
                <span className="sp-banner-mark">Q</span>
                <span>
                  <b>skib laptop sent you a file</b>
                  <small>Tickets.pdf</small>
                </span>
              </div>
            </div>
          </div>

          {/* The laptop */}
          <div className="sp-laptop">
            <div className="wt-screen">
              <div className="wt-layer sp-window" style={{ opacity: 1 }}>
                <b className="sp-title">Downloads</b>
                <div className="sp-row sp-fold" data-k="l-new">
                  <span className="wt-pulse" data-k="l-new-pulse" />
                  <span className="sp-photo-tile" />
                  Sunset.jpg
                </div>
                <div className="sp-row">
                  <Icon d={FILE} />
                  Tickets.pdf
                  <span className="sp-send-btn" data-k="l-send-btn"><Icon d={SEND} size={14} /></span>
                  <div className="sp-menu" data-k="l-menu">
                    <small>Send to</small>
                    <span data-k="l-menu-pick"><span className="wt-circle sp-circle">S<i /></span>skib phone</span>
                  </div>
                </div>
                <div className="sp-row">
                  <Icon d={FILE} />
                  Thesis draft.pdf
                </div>
                <div className="sp-row">
                  <Icon d={FILE} />
                  Notes.txt
                </div>
                <svg className="wt-cursor sp-cursor" data-k="l-cursor" width="22" height="22" viewBox="0 0 24 24">
                  <path d="M5 3l14 8-6 1.5L10 19z" fill="#171816" stroke="#fff" strokeWidth="1.5" strokeLinejoin="round" />
                </svg>
                <div className="sp-toast" data-k="l-toast">
                  <span className="sp-banner-mark">Q</span>
                  <span>
                    <b>skib phone sent you a photo</b>
                    <small>Sunset.jpg</small>
                  </span>
                </div>
              </div>
            </div>
            <div className="wt-base sp-base" />
          </div>

          {/* What travels */}
          <span className="sp-fly sp-photo" data-free data-k="photo-1" style={{ background: PHOTO }} />
          <span className="sp-fly" data-free data-k="plane-1"><Dart /></span>
          <span className="sp-fly sp-doc" data-free data-k="doc-1">
            <i />
            <i />
            <i />
            <b>PDF</b>
          </span>
          <span className="sp-fly" data-free data-k="plane-2"><Glider /></span>
        </div>
      </div>

    </section>
  )
}
