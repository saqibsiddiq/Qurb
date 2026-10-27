'use client'

import { useEffect, useRef } from 'react'

// The page's living ground (docs/design/website-brief.md, *Richer*): a field
// of tiny dots — files, if you like — drifting slowly behind everything and
// gathering near the pointer or the finger, nearness made something you can
// feel. Near the end of the page a second set flies in and settles into the
// wordmark of the element marked data-settle.
//
// One canvas, fixed behind the page, drawn each frame; it stops when the tab
// is hidden. With reduced motion asked for, the dots hold still and the
// wordmark is simply there.

const GREEN = '47, 107, 87'

type Dot = { x: number; y: number; vx: number; vy: number; r: number; a: number; tx: number; ty: number; seed: number }

export function NearField() {
  const ref = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = ref.current
    const ctx = canvas?.getContext('2d')
    if (!canvas || !ctx) return
    const still = matchMedia('(prefers-reduced-motion: reduce)').matches
    let w = 0
    let h = 0
    let dpr = 1
    let field: Dot[] = []
    let word: Dot[] = []
    const pointer = { x: -9999, y: -9999, on: false }
    const settle = document.querySelector<HTMLElement>('[data-settle]')
    // While designing, ?settle shows the ending alone, for screenshots.
    if (process.env.NODE_ENV !== 'production' && new URLSearchParams(location.search).has('settle'))
      document.documentElement.classList.add('end-held')

    const dot = (x: number, y: number, a: number): Dot => ({
      x,
      y,
      vx: (Math.random() - 0.5) * 0.18,
      vy: (Math.random() - 0.5) * 0.18,
      r: 0.7 + Math.random() * 1.1,
      a,
      tx: 0,
      ty: 0,
      seed: Math.random() * 1000
    })

    // Where the wordmark's dots belong: its letters, drawn once off screen
    // at the size the element has, sampled on a grid.
    const targets = () => {
      if (!settle) return []
      const rect = settle.getBoundingClientRect()
      const text = settle.textContent ?? ''
      const off = document.createElement('canvas')
      off.width = Math.max(1, Math.round(rect.width))
      off.height = Math.max(1, Math.round(rect.height))
      const o = off.getContext('2d')!
      const size = rect.height * 0.86
      o.font = `600 ${size}px Inter, system-ui, sans-serif`
      // The page draws the name tight (letter-spacing -0.06em); so must this.
      ;(o as CanvasRenderingContext2D & { letterSpacing: string }).letterSpacing = `${-0.06 * size}px`
      o.textAlign = 'center'
      o.textBaseline = 'middle'
      o.fillText(text, off.width / 2, off.height / 2)
      const data = o.getImageData(0, 0, off.width, off.height).data
      const step = Math.max(4, Math.round(Math.sqrt((off.width * off.height * 0.26) / (w < 700 ? 500 : 1100))))
      const pts: [number, number][] = []
      for (let y = 0; y < off.height; y += step)
        for (let x = 0; x < off.width; x += step) if (data[(y * off.width + x) * 4 + 3] > 128) pts.push([x, y])
      return pts
    }

    const resize = () => {
      dpr = Math.min(2, devicePixelRatio || 1)
      w = innerWidth
      h = innerHeight
      canvas.width = Math.round(w * dpr)
      canvas.height = Math.round(h * dpr)
      canvas.style.width = `${w}px`
      canvas.style.height = `${h}px`
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const n = Math.round(Math.min(w < 700 ? 110 : 260, (w * h) / 6500))
      field = Array.from({ length: n }, () => dot(Math.random() * w, Math.random() * h, 0.18 + Math.random() * 0.32))
      word = targets().map(([tx, ty]) => ({ ...dot(Math.random() * w, Math.random() * h, 0.55 + Math.random() * 0.35), tx, ty }))
    }

    const move = (e: PointerEvent) => {
      pointer.x = e.clientX
      pointer.y = e.clientY
      pointer.on = true
    }
    const leave = () => (pointer.on = false)

    let frame = 0
    let t = 0
    let scattered = true
    const tick = () => {
      t += 1
      ctx.clearRect(0, 0, w, h)

      // The field: a slow drift, a little wander, and a pull toward the
      // pointer that eases off with distance and never quite arrives.
      for (const d of field) {
        if (!still) {
          d.vx += Math.sin((t + d.seed) * 0.004) * 0.004
          d.vy += Math.cos((t + d.seed * 1.3) * 0.004) * 0.004
          if (pointer.on) {
            const dx = pointer.x - d.x
            const dy = pointer.y - d.y
            const dist = Math.hypot(dx, dy)
            if (dist < 220 && dist > 24) {
              const f = (1 - dist / 220) * 0.06
              d.vx += (dx / dist) * f
              d.vy += (dy / dist) * f
            }
          }
          d.vx *= 0.965
          d.vy *= 0.965
          d.x += d.vx + 0.05
          d.y += d.vy - 0.03
          if (d.x < -10) d.x = w + 10
          if (d.x > w + 10) d.x = -10
          if (d.y < -10) d.y = h + 10
          if (d.y > h + 10) d.y = -10
        }
        const near = pointer.on ? Math.max(0, 1 - Math.hypot(pointer.x - d.x, pointer.y - d.y) / 220) : 0
        ctx.fillStyle = `rgba(${GREEN}, ${Math.min(0.9, d.a + near * 0.5)})`
        const r = d.r + near * 0.8
        ctx.fillRect(d.x - r, d.y - r, r * 2, r * 2)
      }

      // The wordmark: as it comes into view its dots fly in from wherever
      // they drift and settle into the letters.
      if (settle && word.length) {
        const rect = settle.getBoundingClientRect()
        const s = still ? 1 : Math.min(1, Math.max(0, 1 - (rect.top - h * 0.25) / (h * 0.55)))
        if (rect.bottom > 0 && rect.top < h && s > 0.01) {
          scattered = false
          ctx.fillStyle = `rgba(${GREEN}, ${0.72 * s})`
          ctx.beginPath()
          for (const d of word) {
            const tx = rect.left + d.tx
            const ty = rect.top + d.ty
            if (still) {
              d.x = tx
              d.y = ty
            } else {
              d.x += Math.sin((t + d.seed) * 0.01) * 0.3 * (1 - s)
              d.y += Math.cos((t + d.seed) * 0.01) * 0.3 * (1 - s)
              d.x += (tx - d.x) * 0.1 * s
              d.y += (ty - d.y) * 0.1 * s
            }
            ctx.moveTo(d.x + d.r + 0.4, d.y)
            ctx.arc(d.x, d.y, d.r + 0.4, 0, Math.PI * 2)
          }
          ctx.fill()
        } else if (s <= 0.01 && !scattered) {
          // Out of sight, they wander off again, so the next arrival flies in.
          scattered = true
          for (const d of word) {
            d.x = Math.random() * w
            d.y = Math.random() * h
          }
        }
      }
      frame = still ? 0 : requestAnimationFrame(tick)
    }

    const visible = () => {
      cancelAnimationFrame(frame)
      if (!document.hidden) frame = requestAnimationFrame(tick)
    }
    // Inter may still be loading; the wordmark's shape waits for it.
    document.fonts?.ready.then(resize)
    resize()
    tick()
    let resizing = 0
    const onResize = () => {
      clearTimeout(resizing)
      resizing = window.setTimeout(resize, 150)
    }
    addEventListener('resize', onResize)
    addEventListener('pointermove', move, { passive: true })
    document.addEventListener('pointerleave', leave)
    document.addEventListener('visibilitychange', visible)
    if (still) addEventListener('scroll', tick, { passive: true })
    return () => {
      cancelAnimationFrame(frame)
      removeEventListener('resize', onResize)
      removeEventListener('pointermove', move)
      document.removeEventListener('pointerleave', leave)
      document.removeEventListener('visibilitychange', visible)
      removeEventListener('scroll', tick)
    }
  }, [])

  return <canvas ref={ref} className="near-field" aria-hidden="true" />
}
