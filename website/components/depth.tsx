'use client'

import Lenis from 'lenis'
import { useEffect } from 'react'

// Depth for the landing page (docs/design/website-brief.md, *Richer*):
// headlines rise and scenes ease in as they arrive; each scene's devices
// sit a little deeper than its words, drifting at their own pace as the page
// scrolls, and lean slightly toward the pointer; and the wheel scrolls
// smoothly. None of it with reduced motion asked for.
//
// The smooth wheel is Lenis (MIT), which tells a trackpad from a mouse
// wheel; a first version of our own smoothed both alike and made a
// trackpad feel sticky. Touch, the keyboard and the scrollbar scroll as
// they always do.
export function Depth() {
  useEffect(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const root = document.documentElement
    root.classList.add('motion')

    // Arrivals.
    const arrive = new IntersectionObserver(
      (entries) =>
        entries.forEach((e) => {
          if (e.isIntersecting) {
            e.target.classList.add('in')
            arrive.unobserve(e.target)
          }
        }),
      { rootMargin: '0px 0px -12% 0px' }
    )
    document
      .querySelectorAll('.rise, .scene > p, .sp > p, .fm-caps, .wt-soon, .scene-room, .qs-hero p, .gi-note, .gi-get, .gi-wait')
      .forEach((el) => arrive.observe(el))

    // Each scene's devices drift against the scroll and lean to the pointer,
    // written straight to the transform each frame — one write a scene, no
    // CSS transition restarted on every frame.
    const fine = matchMedia('(pointer: fine)').matches
    const rooms = [...document.querySelectorAll<HTMLElement>('.scene-room')].map((el) => ({ el, rx: 0, ry: 0, tx: 0, ty: 0 }))
    const lean = (e: PointerEvent) => {
      const r = rooms.find((x) => x.el === e.currentTarget)!
      const b = r.el.getBoundingClientRect()
      r.ty = ((e.clientX - b.left) / b.width - 0.5) * 5
      r.tx = ((e.clientY - b.top) / b.height - 0.5) * -4
    }
    const rest = (e: PointerEvent) => {
      const r = rooms.find((x) => x.el === e.currentTarget)!
      r.tx = r.ty = 0
    }
    if (fine)
      rooms.forEach((r) => {
        r.el.addEventListener('pointermove', lean)
        r.el.addEventListener('pointerleave', rest)
      })

    const lenis = fine ? new Lenis({ lerp: 0.1, smoothWheel: true }) : null
    if (lenis) root.classList.add('lenis')

    let frame = 0
    const tick = (time: number) => {
      lenis?.raf(time)
      for (const r of rooms) {
        if (!r.el.classList.contains('in')) continue
        const b = r.el.getBoundingClientRect()
        if (b.bottom < -200 || b.top > innerHeight + 200) continue
        r.rx += (r.tx - r.rx) * 0.12
        r.ry += (r.ty - r.ry) * 0.12
        const par = ((b.top + b.height / 2 - innerHeight / 2) / innerHeight) * -48
        r.el.style.transform = `perspective(1600px) translate3d(0, ${par.toFixed(1)}px, 0) rotateX(${r.rx.toFixed(2)}deg) rotateY(${r.ry.toFixed(2)}deg)`
      }
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)

    return () => {
      cancelAnimationFrame(frame)
      lenis?.destroy()
      root.classList.remove('motion', 'lenis')
      arrive.disconnect()
      rooms.forEach((r) => {
        r.el.removeEventListener('pointermove', lean)
        r.el.removeEventListener('pointerleave', rest)
        r.el.style.transform = ''
      })
    }
  }, [])
  return null
}
