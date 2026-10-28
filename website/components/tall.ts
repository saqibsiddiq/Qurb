import type { Tall } from './play-on-view'

// How the side-by-side scenes are drawn on a phone (play-on-view.ts): phone
// first, at full size, and the computer small above it, so that what the
// phone shows can be read on the phone it is shown on. Positions are in
// app/home.css, under *On a phone*.

// Phone left, laptop right, on a 1240-wide stage: the phone from (40, 15)
// to (90, 250); the laptop from (620, 80) to (88, 20), at 0.42.
export const TALL_PAIR: Tall = {
  w: 420,
  h: 760,
  a: { x: 290, from: [40, 15], to: [90, 250], s: 1 },
  b: { x: 620, from: [620, 80], to: [88, 20], s: 0.42 }
}

// The walkthrough has them the other way round on a 1000-wide stage: the
// laptop from (0, 70) to (82, 20), at 0.4; the phone from (740, 30) to
// (90, 230).
export const TALL_WALK: Tall = {
  w: 420,
  h: 740,
  a: { x: 640, from: [0, 70], to: [82, 20], s: 0.4 },
  b: { x: 740, from: [740, 30], to: [90, 230], s: 1 }
}
