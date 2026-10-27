'use client'

import { useRef } from 'react'
import { Rise } from './rise'
import { usePlayOnView } from './play-on-view'
import { TALL_PAIR } from './tall'
import { press, show, tap, type Keys } from './timeline'

// The backups scene, the owner's addition to the brief: folders chosen on the
// phone — WhatsApp, Camera, Documents — go to the computer and keep going
// there on their own, and come back when they are needed. As docs/features.md
// has it: WhatsApp's folder kept on the computer, the phone's folders as a
// tree with a checkbox each, chosen on the phone or the computer; a file
// restored to where it was is built, and not yet seen on the S23.

const DURATION = 11000

const tick = (at: number): Keys => ({ o: [[at, 0], [at + 0.015, 1]], s: [[at, 0.4], [at + 0.025, 1]] })

const TIMELINE: Record<string, Keys> = {
  'tap-1': tap(0.07),
  'c-1': tick(0.075),
  'tap-2': tap(0.13),
  'c-2': tick(0.135),
  'tap-3': tap(0.19),
  'c-3': tick(0.195),
  'p-save': { s: press(0.24) },
  'tap-save': tap(0.24),
  'fly-1': { o: [[0.26, 0], [0.27, 1], [0.38, 1], [0.4, 0]], path: { t0: 0.26, t1: 0.39, facing: 0, bank: 0.2, points: [[200, 170], [380, 90], [560, 80], [700, 120]] } },
  'fly-2': { o: [[0.31, 0], [0.32, 1], [0.43, 1], [0.45, 0]], path: { t0: 0.31, t1: 0.44, facing: 0, bank: 0.2, points: [[200, 220], [380, 150], [600, 130], [860, 120]] } },
  'fly-3': { o: [[0.36, 0], [0.37, 1], [0.48, 1], [0.5, 0]], path: { t0: 0.36, t1: 0.49, facing: 0, bank: 0.2, points: [[200, 270], [380, 230], [560, 230], [700, 220]] } },
  'l-empty': { o: [[0.36, 1], [0.39, 0]] },
  'l-f1': { o: [[0.38, 0], [0.41, 1]], s: [[0.38, 0.85], [0.41, 1]] },
  'l-f2': { o: [[0.43, 0], [0.46, 1]], s: [[0.43, 0.85], [0.46, 1]] },
  'l-f3': { o: [[0.48, 0], [0.51, 1]], s: [[0.48, 0.85], [0.51, 1]] },
  // It keeps going: two new photos follow on their own.
  'cap-auto': { o: show(0.53, 0.56, 0.68, 0.71) },
  'dot-1': { o: [[0.56, 0], [0.57, 1], [0.63, 1], [0.64, 0]], path: { t0: 0.56, t1: 0.635, facing: 0, bank: 0, points: [[230, 230], [430, 170], [640, 200], [880, 236]] } },
  'dot-2': { o: [[0.6, 0], [0.61, 1], [0.67, 1], [0.68, 0]], path: { t0: 0.6, t1: 0.675, facing: 0, bank: 0, points: [[230, 230], [430, 190], [640, 220], [880, 236]] } },
  'n-old': { o: [[0.635, 1], [0.645, 0]] },
  'n-mid': { o: [[0.635, 0], [0.645, 1], [0.675, 1], [0.685, 0]] },
  'n-new': { o: [[0.675, 0], [0.685, 1]] },
  'l-f2-pulse': { o: [[0.635, 0], [0.645, 1], [0.66, 0], [0.675, 0], [0.685, 1], [0.71, 0]] },
  // And comes back when it is needed.
  'p-tree': { o: [[0.72, 1], [0.745, 0]] },
  'p-restore': { o: [[0.72, 0], [0.745, 1]], x: [[0.72, 40], [0.745, 0]] },
  'p-restore-btn': { s: press(0.8) },
  'tap-restore': tap(0.8),
  back: { o: [[0.82, 0], [0.83, 1], [0.93, 1], [0.945, 0]], path: { t0: 0.82, t1: 0.94, facing: 180, bank: 0.2, points: [[700, 140], [560, 60], [380, 70], [190, 150]] } },
  'p-done': { o: [[0.94, 0], [0.955, 1]], s: [[0.94, 0.6], [0.965, 1]] }
}

const CHECK = 'M229.66,77.66l-128,128a8,8,0,0,1-11.32,0l-56-56a8,8,0,0,1,11.32-11.32L96,188.69,218.34,66.34a8,8,0,0,1,11.32,11.32Z'
const FOLDER =
  'M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z'
const Icon = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
    <path d={d} fill="currentColor" />
  </svg>
)

const TREE = ['WhatsApp', 'Camera', 'Documents', 'Downloads', 'Music']

export function Backups() {
  const root = useRef<HTMLElement>(null)
  usePlayOnView(root, TIMELINE, DURATION, 'bk', [0, 1], TALL_PAIR)

  return (
    <section ref={root} className="scene bk scene-split" aria-label="Backups">
      <Rise lines={['Pick what to keep safe.']} />
      <p>Your phone keeps those folders backed up on your computer, on its own. Need them back? They’re there.</p>

      <div className="scene-room">
        <div className="scene-stage" data-w="1240" aria-hidden="true">
          <div className="wt-phone sp-phone">
            <div className="wt-pscreen">
              <div className="wt-layer bk-tree" data-k="p-tree" style={{ opacity: 1 }}>
                <b>Folders to back up</b>
                {TREE.map((name, i) => (
                  <div key={name} className="bk-node">
                    <span className="bk-box">
                      {i < 3 && (
                        <i data-k={`c-${i + 1}`}>
                          <Icon d={CHECK} size={11} />
                        </i>
                      )}
                    </span>
                    <Icon d={FOLDER} size={16} />
                    {name}
                    {i < 3 && <span className="wt-tap" data-k={`tap-${i + 1}`} style={{ left: -12, top: 4 }} />}
                  </div>
                ))}
                <span className="wt-send bk-save" data-k="p-save">Back up 3 folders</span>
                <span className="wt-tap" data-k="tap-save" style={{ left: 94, top: 392 }} />
              </div>

              <div className="wt-layer bk-restore" data-k="p-restore">
                <b>Your folder on skib laptop</b>
                {['WhatsApp', 'Camera', 'Documents'].map((name, i) => (
                  <div key={name} className="bk-node">
                    <Icon d={FOLDER} size={16} />
                    {name}
                    {i === 0 ? (
                      <span className="bk-pill" data-k="p-restore-btn">
                        Restore
                        <span className="wt-tap" data-k="tap-restore" style={{ left: 8, top: -8 }} />
                      </span>
                    ) : (
                      <span className="bk-pill bk-pill-quiet">Restore</span>
                    )}
                  </div>
                ))}
                <span className="sp-sent bk-done" data-k="p-done"><Icon d={CHECK} size={12} />WhatsApp is back</span>
              </div>
            </div>
          </div>

          <div className="sp-laptop">
            <div className="wt-screen">
              <div className="wt-layer sp-window" style={{ opacity: 1 }}>
                <b className="sp-title">Backups · skib phone</b>
                <small className="bk-empty" data-k="l-empty">Nothing backed up yet</small>
                <div className="bk-cards">
                  <span data-k="l-f1"><Icon d={FOLDER} size={24} /><b>WhatsApp</b><small>Chats and media</small></span>
                  <span data-k="l-f2">
                    <span className="wt-pulse" data-k="l-f2-pulse" />
                    <Icon d={FOLDER} size={24} />
                    <b>Camera</b>
                    <small className="wt-swap">
                      <span data-k="n-old">1,204 photos</span>
                      <span data-k="n-mid" className="bk-n">1,205 photos</span>
                      <span data-k="n-new" className="bk-n">1,206 photos</span>
                    </small>
                  </span>
                  <span data-k="l-f3"><Icon d={FOLDER} size={24} /><b>Documents</b><small>86 files</small></span>
                </div>
                <span className="bk-auto" data-k="cap-auto">Backing up new photos…</span>
              </div>
            </div>
            <div className="wt-base" />
          </div>

          {[1, 2, 3].map((n) => (
            <span key={n} className="bk-chip" data-free data-k={`fly-${n}`}>
              <Icon d={FOLDER} size={14} />
              {TREE[n - 1]}
            </span>
          ))}
          <span className="bk-dot" data-free data-k="dot-1" />
          <span className="bk-dot" data-free data-k="dot-2" />
          <span className="bk-chip" data-free data-k="back">
            <Icon d={FOLDER} size={14} />
            WhatsApp
          </span>
        </div>
      </div>
    </section>
  )
}
