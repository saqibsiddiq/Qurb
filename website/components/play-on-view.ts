'use client'

import { useEffect, type RefObject } from 'react'
import { draw, type Keys, type Warp } from './timeline'

// A scene that plays once by the clock when half of it is on screen, then
// rests on its last frame; with reduced motion it shows that frame still.
// Once it has played, clicking it plays it again. While designing, ?<hold>=0.4
// shows that scene alone, held at that point.
// `range` plays only that part of the timeline, for scenes that share one.
//
// `tall` is how the scene is drawn on a phone: phone first, big enough to
// read, with the computer small above it. Its size there, and where each
// device goes: `a` and `b` are the devices' inner edges in the wide drawing
// (x), each with where its device's corner was (`from`), where it is now
// (`to`) and how much it is scaled (`s`). A point near one device moves with
// it, a point between them by a share of each, so a flight from one device
// to the other bends into the tall drawing with them.
type Side = { x: number; from: [number, number]; to: [number, number]; s: number }
export type Tall = { w: number; h: number; a: Side; b: Side }
const TALL = '(max-width: 900px)'
// A tall drawing may grow past its own size on a tablet, up to this much.
const TALL_GROW = 1.6

export function usePlayOnView(
  root: RefObject<HTMLElement | null>,
  timeline: Record<string, Keys>,
  duration: number,
  hold: string,
  range: [number, number] = [0, 1],
  tall?: Tall
) {
  const [from, to] = range
  useEffect(() => {
    const section = root.current
    if (!section) return
    const at = (p: number) => from + p * (to - from)
    const els = new Map<string, HTMLElement>()
    section.querySelectorAll<HTMLElement>('[data-k]').forEach((el) => els.set(el.dataset.k!, el))
    // The stage is drawn at its own size (data-w) and scaled to the room.
    const room = section.querySelector<HTMLElement>('.scene-room')
    const stage = section.querySelector<HTMLElement>('.scene-stage')
    let warp: Warp | undefined
    if (tall && stage && room && matchMedia(TALL).matches) {
      stage.dataset.tall = ''
      stage.dataset.w = String(tall.w)
      stage.style.width = `${tall.w}px`
      stage.style.height = `${tall.h}px`
      room.style.aspectRatio = `${tall.w} / ${tall.h}`
      room.style.maxWidth = `${tall.w * TALL_GROW}px`
      const { a, b } = tall
      const map = (d: Side, x: number, y: number) => [d.to[0] + (x - d.from[0]) * d.s, d.to[1] + (y - d.from[1]) * d.s]
      warp = (x, y) => {
        const t = Math.min(1, Math.max(0, (x - a.x) / (b.x - a.x)))
        const [ax, ay] = map(a, x, y)
        const [bx, by] = map(b, x, y)
        return [ax + (bx - ax) * t, ay + (by - ay) * t]
      }
    }
    const paint = (p: number) => draw(timeline, els, at(p), warp)
    const fit = new ResizeObserver(([e]) => {
      if (stage) stage.style.setProperty('--scene-scale', String(Math.min(warp ? TALL_GROW : 1, e.contentRect.width / Number(stage.dataset.w))))
    })
    if (room) fit.observe(room)

    const held = process.env.NODE_ENV !== 'production' && new URLSearchParams(location.search).get(hold)
    if (held) {
      document.documentElement.classList.add('scene-held')
      section.classList.add('scene-shown')
      paint(Number(held))
      return () => fit.disconnect()
    }
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      paint(1)
      return () => fit.disconnect()
    }

    let frame = 0
    const play = () => {
      if (frame) return
      section.classList.remove('scene-done')
      const start = performance.now()
      const tick = (now: number) => {
        const p = Math.min(1, (now - start) / duration)
        paint(p)
        frame = p < 1 ? requestAnimationFrame(tick) : 0
        if (!frame) section.classList.add('scene-done')
      }
      frame = requestAnimationFrame(tick)
    }
    paint(0)
    const seen = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return
        play()
        seen.disconnect()
      },
      { threshold: 0.5 }
    )
    seen.observe(section.querySelector('.scene-room') ?? section)
    const again = section.querySelector<HTMLElement>('.scene-room')
    const replay = () => section.classList.contains('scene-done') && play()
    again?.addEventListener('click', replay)
    return () => {
      cancelAnimationFrame(frame)
      seen.disconnect()
      fit.disconnect()
      again?.removeEventListener('click', replay)
    }
  }, [root, timeline, duration, hold, from, to, tall])
}
