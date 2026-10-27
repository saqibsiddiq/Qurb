// The little animation engine both scripted scenes use (the walkthrough and
// the paper plane): every moving part has keyframes in one number, p, from 0
// to 1, and draw() sets each part from them. What drives p — the scroll, or
// the clock — is up to the scene.

export type Track = [number, number][]
export type Keys = { o?: Track; x?: Track; y?: Track; r?: Track; s?: Track; h?: Track; w?: Track; dash?: Track; path?: Flight; tall?: Keys }

// On a phone a scene is drawn tall — laptop above, phone below — instead of
// side by side. A Warp moves a point of the wide drawing to where it belongs
// in the tall one; it is applied to what flies free between the devices
// (elements marked data-free). A key's `tall` replaces any of its tracks
// in the tall drawing, where bending its wide ones is not enough.
export type Warp = (x: number, y: number) => [number, number]

// Shown between a and d, fading in over a-b and out over c-d.
export const show = (a: number, b: number, c = 2, d = 2): Track => [[a, 0], [b, 1], [c, 1], [d, 0]]
export const tap = (at: number): Keys => ({ o: [[at, 0], [at + 0.004, 0.55], [at + 0.02, 0]], s: [[at, 0.3], [at + 0.02, 1.5]] })
export const press = (at: number): Track => [[at, 1], [at + 0.008, 0.9], [at + 0.018, 1]]

export const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2)
export function at(track: Track, p: number) {
  if (p <= track[0][0]) return track[0][1]
  for (let i = 1; i < track.length; i++) {
    const [p1, v1] = track[i]
    if (p <= p1) {
      const [p0, v0] = track[i - 1]
      return v0 + (v1 - v0) * ease((p - p0) / (p1 - p0 || 1))
    }
  }
  return track[track.length - 1][1]
}

export function draw(timeline: Record<string, Keys>, els: Map<string, HTMLElement>, p: number, warp?: Warp) {
  for (const [key, wide] of Object.entries(timeline)) {
    const el = els.get(key)
    if (!el) continue
    const k = warp && wide.tall ? { ...wide, ...wide.tall } : wide
    const bend = warp && el.hasAttribute('data-free') ? warp : undefined
    if (k.o) el.style.opacity = String(at(k.o, p))
    if (k.path) {
      const f = fly(k.path, p, bend)
      el.style.transform = `translate(${f.x}px, ${f.y}px) rotate(${f.r}deg) scale(${k.s ? at(k.s, p) : 1})`
    } else if (k.x || k.y || k.r || k.s) {
      let x = k.x ? at(k.x, p) : 0
      let y = k.y ? at(k.y, p) : 0
      if (bend) [x, y] = bend(x, y)
      el.style.transform = `translate(${x}px, ${y}px) rotate(${k.r ? at(k.r, p) : 0}deg) scale(${k.s ? at(k.s, p) : 1})`
    }
    if (k.h) el.style.height = `${at(k.h, p)}px`
    if (k.w) el.style.width = `${at(k.w, p)}%`
    // A line drawn along its length: the path has pathLength="1" and a dash of 1.
    if (k.dash) el.style.strokeDashoffset = String(at(k.dash, p))
    el.style.visibility = k.o && at(k.o, p) < 0.01 ? 'hidden' : ''
  }
}

// A flight: a smooth curve through points (each the top-left of the moving
// element), travelled at an even speed from t0 to t1, easing out of the start
// and into the end, with the element turned to face where it is going.
// `facing` is the angle its drawing already points at, and `bank` how much of
// the turn it takes: a label banks less than a plane, to stay readable.
// Keyframes would make it slow at every point; this keeps it gliding.
export type Flight = { t0: number; t1: number; points: [number, number][]; facing: number; bank?: number }
type Point = [number, number]
const cr = (pts: Point[], s: number): Point => {
  const n = pts.length - 1
  const i = Math.min(n - 1, Math.max(0, Math.floor(s)))
  const t = s - i
  const p0 = pts[Math.max(0, i - 1)], p1 = pts[i], p2 = pts[i + 1], p3 = pts[Math.min(n, i + 2)]
  const f = (a: number, b: number, c: number, d: number) =>
    0.5 * (2 * b + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t * t + (-a + 3 * b - 3 * c + d) * t * t * t)
  return [f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]
}
const glide = (t: number) => (1 - Math.cos(Math.PI * t)) / 2
const lengths = new WeakMap<Flight, { s: number[]; len: number[] }>()
export function fly(f: Flight, p: number, warp?: Warp) {
  let table = lengths.get(f)
  if (!table) {
    const steps = 400, n = f.points.length - 1
    const s: number[] = [0], len: number[] = [0]
    let prev = cr(f.points, 0)
    for (let i = 1; i <= steps; i++) {
      const si = (i / steps) * n
      const pt = cr(f.points, si)
      s.push(si)
      len.push(len[i - 1] + Math.hypot(pt[0] - prev[0], pt[1] - prev[1]))
      prev = pt
    }
    table = { s, len }
    lengths.set(f, table)
  }
  const u = glide(Math.min(1, Math.max(0, (p - f.t0) / (f.t1 - f.t0))))
  const want = u * table.len[table.len.length - 1]
  let lo = 0, hi = table.len.length - 1
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1
    if (table.len[mid] < want) lo = mid
    else hi = mid
  }
  const span = table.len[hi] - table.len[lo] || 1
  const s = table.s[lo] + ((want - table.len[lo]) / span) * (table.s[hi] - table.s[lo])
  const w = (pt: Point): Point => (warp ? warp(pt[0], pt[1]) : pt)
  const [x, y] = w(cr(f.points, s))
  const n = f.points.length - 1
  const a = w(cr(f.points, Math.max(0, s - 0.02))), b = w(cr(f.points, Math.min(n, s + 0.02)))
  const heading = (Math.atan2(b[1] - a[1], b[0] - a[0]) * 180) / Math.PI
  // The turn from its drawing's own direction, the short way round.
  const turn = ((heading - f.facing + 540) % 360) - 180
  return { x, y, r: turn * (f.bank ?? 1) }
}
