import type { Tall } from './play-on-view'

// How the side-by-side scenes are drawn on a phone: 640 wide, the laptop at
// the top, the phone under it. The laptop moves from (620, 80) to (30, 30)
// and the phone from (40, 15) to (200, 520); a flight between them bends
// with them (play-on-view.ts).
export const TALL_PAIR: Tall = { w: 640, h: 1030, a: { x: 290, d: [160, 505] }, b: { x: 620, d: [-590, -50] } }

// The walkthrough has them the other way round, laptop left and phone right,
// on a 1000-wide stage: the laptop from (0, 70) to (30, 30), the phone from
// (740, 30) to (230, 520).
export const TALL_WALK: Tall = { w: 700, h: 1030, a: { x: 640, d: [30, -40] }, b: { x: 740, d: [-510, 490] } }
