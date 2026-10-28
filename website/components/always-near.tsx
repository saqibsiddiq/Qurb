'use client'

import { useEffect, useId, useRef, useState } from 'react'

// Scene 1 of the website brief (docs/design/website-brief.md): a laptop at
// home, a phone leaving it, and the laptop's folders following the phone
// like ducklings. Drawn as one SVG on a 1200 x 400 stage so it scales as a
// whole. The markup is the resting frame; the animation plays from the first
// frame to it once, and not at all when reduced motion is asked for.

const GREEN = '#2F6B57'
const MUTE = '#7A7D76'
const FOLDER =
  'M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z'
const HOUSE =
  'M219.31,108.68l-80-80a16,16,0,0,0-22.62,0l-80,80A15.87,15.87,0,0,0,32,120v96a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V160h32v56a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V120A15.87,15.87,0,0,0,219.31,108.68ZM208,208H160V152a8,8,0,0,0-8-8H104a8,8,0,0,0-8,8v56H48V120l80-80,80,80Z'

type Pose = { x: number; y: number; s?: number; r?: number }
const pose = ({ x, y, s = 1, r = 0 }: Pose) => `translate(${x}px, ${y}px) rotate(${r}deg) scale(${s})`

// Each folder: on the laptop's screen, hopping out, following in a line, at
// rest beside the phone. The last one on the screen hops first and leads.
const DUCKS: { start: Pose; hop: Pose; trail: Pose; rest: Pose; hopAt: number }[] = [
  { start: { x: 150, y: 150, s: 0.7 }, hop: { x: 180, y: 30, s: 0.78, r: -8 }, trail: { x: 470, y: 140, s: 0.82, r: 6 }, rest: { x: 572, y: 84, s: 0.76, r: 8 }, hopAt: 0.5 },
  { start: { x: 207, y: 150, s: 0.7 }, hop: { x: 250, y: 20, s: 0.82, r: 8 }, trail: { x: 560, y: 128, s: 0.88, r: -3 }, rest: { x: 660, y: 60, s: 0.84, r: -4 }, hopAt: 0.42 },
  { start: { x: 264, y: 150, s: 0.7 }, hop: { x: 330, y: 30, s: 0.85, r: -12 }, trail: { x: 650, y: 118, s: 0.94, r: 4 }, rest: { x: 752, y: 56, s: 0.92, r: 5 }, hopAt: 0.34 },
  { start: { x: 321, y: 150, s: 0.7 }, hop: { x: 430, y: 0, s: 0.9, r: 10 }, trail: { x: 740, y: 110, r: -6 }, rest: { x: 850, y: 80, r: -8 }, hopAt: 0.26 }
]
const PHONE = { start: { x: 470, y: 52 }, mid: { x: 640, y: 52, r: -2 }, late: { x: 830, y: 52, r: -3 }, rest: { x: 950, y: 52, r: -4 } }

// On a phone the drawing is tall: the laptop up at the left, the phone down
// at the right and larger on the screen, the folders hopping out of the
// laptop and trailing down to it.
const DUCKS_TALL: typeof DUCKS = [
  { start: { x: 100, y: 110, s: 0.7 }, hop: { x: 160, y: 0, s: 0.78, r: -8 }, trail: { x: 300, y: 70, s: 0.82, r: 6 }, rest: { x: 350, y: 236, s: 0.76, r: 8 }, hopAt: 0.5 },
  { start: { x: 157, y: 110, s: 0.7 }, hop: { x: 230, y: -10, s: 0.82, r: 8 }, trail: { x: 320, y: 130, s: 0.88, r: -3 }, rest: { x: 376, y: 462, s: 0.84, r: -4 }, hopAt: 0.42 },
  { start: { x: 214, y: 110, s: 0.7 }, hop: { x: 300, y: 0, s: 0.85, r: -12 }, trail: { x: 340, y: 190, s: 0.94, r: 4 }, rest: { x: 360, y: 390, s: 0.92, r: 5 }, hopAt: 0.34 },
  { start: { x: 271, y: 110, s: 0.7 }, hop: { x: 360, y: -10, s: 0.9, r: 10 }, trail: { x: 360, y: 250, r: -6 }, rest: { x: 372, y: 310, r: -8 }, hopAt: 0.26 }
]
const PHONE_TALL = { start: { x: 395, y: 40 }, mid: { x: 420, y: 130, r: -2 }, late: { x: 432, y: 220, r: -3 }, rest: { x: 440, y: 300, r: -4 } }

const WIDE = { ducks: DUCKS, phone: PHONE, box: '0 0 1200 400', laptop: '', home: 'translate(240 322)', trail: 'M 440 150 C 600 30 830 40 970 150', glow: [600, 250, 520, 170] }
const TALL = { ducks: DUCKS_TALL, phone: PHONE_TALL, box: '0 0 640 620', laptop: 'translate(-70 -60)', home: 'translate(170 262)', trail: 'M 340 120 C 420 200 330 300 430 380', glow: [330, 380, 300, 220] }

const DURATION = 3800
const DELAY = 500
const GLIDE = 'cubic-bezier(.45, 0, .25, 1)'
const SPRING = 'cubic-bezier(.3, 1.35, .55, 1)'

export function AlwaysNear() {
  const stage = useRef<SVGSVGElement>(null)
  // Its own ids, so that a page could draw it more than once.
  const id = useId().replace(/[^a-zA-Z0-9]/g, '')
  const [tall, setTall] = useState(false)
  useEffect(() => setTall(matchMedia('(max-width: 900px)').matches), [])
  const L = tall ? TALL : WIDE

  useEffect(() => {
    const svg = stage.current
    if (!svg) return
    svg.dataset.ready = ''
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return

    const { ducks: DUCKS, phone: PHONE } = tall ? TALL : WIDE
    const timing = { duration: DURATION, delay: DELAY, fill: 'both' as const }
    const run: Animation[] = []
    const phone = svg.querySelector<SVGGElement>('.qs-phone')!
    run.push(
      phone.animate(
        [
          { offset: 0, transform: pose(PHONE.start), easing: GLIDE },
          { offset: 0.22, transform: pose(PHONE.start), easing: GLIDE },
          { offset: 0.5, transform: pose(PHONE.mid), easing: GLIDE },
          { offset: 0.76, transform: pose(PHONE.late), easing: GLIDE },
          { offset: 1, transform: pose(PHONE.rest) }
        ],
        timing
      )
    )
    svg.querySelectorAll<SVGGElement>('.qs-duck').forEach((el, i) => {
      const d = DUCKS[i]
      const landed = d.hopAt + 0.1
      run.push(
        el.animate(
          [
            { offset: 0, transform: pose(d.start) },
            { offset: d.hopAt, transform: pose(d.start), easing: SPRING },
            { offset: landed, transform: pose(d.hop), easing: GLIDE },
            { offset: Math.max(landed + 0.12, 0.76 + i * 0.02), transform: pose(d.trail), easing: SPRING },
            { offset: 1, transform: pose(d.rest) }
          ],
          timing
        )
      )
    })
    run.push(
      svg.querySelector('.qs-arrived')!.animate(
        [{ opacity: 0, transform: 'scale(.6)' }, { offset: 0.86, opacity: 0, transform: 'scale(.6)', easing: SPRING }, { opacity: 1, transform: 'scale(1)' }],
        timing
      ),
      svg.querySelector('.qs-trail')!.animate([{ opacity: 0 }, { offset: 0.62, opacity: 0 }, { opacity: 1 }], timing)
    )
    run[0].finished.then(() => svg.classList.add('qs-settled')).catch(() => {})
    return () => run.forEach((a) => a.cancel())
  }, [tall])

  return (
    <svg ref={stage} className={`qs-near${tall ? ' qs-near-tall' : ''}`} viewBox={L.box} role="img" aria-label="A laptop at home and a phone away from it, with the laptop's folders following the phone.">
      <defs>
        <radialGradient id={`${id}-glow`}>
          <stop offset="0" stopColor="#BFE3D2" stopOpacity="0.6" />
          <stop offset="1" stopColor="#BFE3D2" stopOpacity="0" />
        </radialGradient>
        <filter id={`${id}-lift`} x="-50%" y="-50%" width="200%" height="200%">
          <feDropShadow dx="0" dy="10" stdDeviation="10" floodColor="#173329" floodOpacity="0.12" />
        </filter>
        <filter id={`${id}-phone-shadow`} x="-50%" y="-30%" width="200%" height="160%">
          <feDropShadow dx="0" dy="22" stdDeviation="18" floodColor="#171816" floodOpacity="0.16" />
        </filter>
        <linearGradient id={`${id}-screen`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#F4F8F5" />
          <stop offset="1" stopColor="#E3EEE8" />
        </linearGradient>
      </defs>

      <ellipse cx={L.glow[0]} cy={L.glow[1]} rx={L.glow[2]} ry={L.glow[3]} fill={`url(#${id}-glow)`} />
      <path className="qs-trail" d={L.trail} fill="none" stroke={GREEN} strokeOpacity="0.28" strokeWidth="2" strokeDasharray="1 9" strokeLinecap="round" />

      {/* The laptop, at home. */}
      <g transform={L.laptop || undefined}>
        <rect x="130" y="92" width="300" height="192" rx="14" fill="#1C1D1B" />
        <rect x="138" y="100" width="284" height="176" rx="8" fill={`url(#${id}-screen)`} />
        <rect x="100" y="284" width="360" height="14" rx="7" fill="#D6D7D1" />
        <rect x="250" y="284" width="60" height="5" rx="2.5" fill="#C3C4BE" />
      </g>
      <g transform={L.home} fill={MUTE}>
        <path d={HOUSE} transform="scale(0.0547)" />
        <text x="20" y="12" className="qs-caption">At home</text>
      </g>

      {/* The phone, with you. */}
      <g className="qs-phone" style={{ transform: pose(L.phone.rest) }}>
        <rect width="132" height="268" rx="30" fill="#1C1D1B" filter={`url(#${id}-phone-shadow)`} />
        <rect x="6" y="6" width="120" height="256" rx="24" fill="#FFFFFF" />
        <rect x="49" y="15" width="34" height="10" rx="5" fill="#1C1D1B" />
        <g className="qs-arrived">
          {[[30, 82], [76, 82], [30, 128], [76, 128]].map(([x, y]) => (
            <g key={`${x}-${y}`} transform={`translate(${x} ${y})`}>
              <rect width="38" height="38" rx="11" fill="#E8F0EC" />
              <path d={FOLDER} fill={GREEN} transform="translate(9 9) scale(0.078)" />
            </g>
          ))}
        </g>
        <text x="66" y="296" textAnchor="middle" className="qs-caption" fill={MUTE}>With you</text>
      </g>

      {L.ducks.map((d, i) => (
        <g key={i} className="qs-duck" style={{ transform: pose(d.rest) }}>
          <g className="qs-bob" style={{ animationDelay: `${i * -0.9}s` }}>
            <rect width="56" height="56" rx="16" fill="#FFFFFF" filter={`url(#${id}-lift)`} />
            <path d={FOLDER} fill={GREEN} transform="translate(14 14) scale(0.1094)" />
          </g>
        </g>
      ))}
    </svg>
  )
}
