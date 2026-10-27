'use client'

import { useEffect, type RefObject } from 'react'
import { draw, type Keys, type Warp } from './timeline'

// A scene that plays once by the clock when half of it is on screen, then
// rests on its last frame; with reduced motion it shows that frame still.
// Once it has played, clicking it plays it again. While designing, ?<hold>=0.4
// shows that scene alone, held at that point.
// `range` plays only that part of the timeline, for scenes that share one.
//
// `tall` is how the scene is drawn on a phone, where it would be too small
// side by side: its size there, and where each device moves to. `a` and `b`
// are the devices' inner edges in the wide drawing, each with how far that
// device moves; a point between them moves by a share of each, so a flight
// from one device to the other bends into the tall drawing with them.
export type Tall = { w: number; h: number; a: { x: number; d: [number, number] }; b: { x: number; d: [number, number] } }
const TALL = '(max-width: 700px)'

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
      room.style.maxWidth = `${tall.w}px`
      const { a, b } = tall
      warp = (x, y) => {
        const t = Math.min(1, Math.max(0, (x - a.x) / (b.x - a.x)))
        return [x + a.d[0] + (b.d[0] - a.d[0]) * t, y + a.d[1] + (b.d[1] - a.d[1]) * t]
      }
    }
    const paint = (p: number) => draw(timeline, els, at(p), warp)
    const fit = new ResizeObserver(([e]) => {
      if (stage) stage.style.setProperty('--scene-scale', String(Math.min(1, e.contentRect.width / Number(stage.dataset.w))))
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
