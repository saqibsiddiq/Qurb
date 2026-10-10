'use client'

import { useEffect } from 'react'

/**
 * The landing page's motion, in one place and in one small script: the page
 * itself is static HTML, readable with no script at all.
 *
 * - `.reveal` rises into place as it scrolls into view, once.
 * - `.statement` lights its words one by one as it passes through the screen.
 *
 * Nothing is hidden before this runs: what is already on screen is marked
 * shown first, and only then does the page take the class that hides the
 * rest, so nothing flickers. With reduced motion asked for, it does nothing,
 * and everything is simply there.
 */
export function Motion() {
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const root = document.documentElement

    const reveals = Array.from(document.querySelectorAll<HTMLElement>('.reveal'))
    const onScreen = (el: HTMLElement) => el.getBoundingClientRect().top < window.innerHeight * 0.92
    for (const el of reveals) if (onScreen(el)) el.classList.add('in')
    root.classList.add('motion')

    const seen = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue
          entry.target.classList.add('in')
          seen.unobserve(entry.target)
        }
      },
      { rootMargin: '0px 0px -8% 0px' }
    )
    for (const el of reveals) if (!el.classList.contains('in')) seen.observe(el)

    // A statement's words light from the first as its middle crosses the
    // screen: none lit as it enters at the bottom, all lit by the time it is
    // a third of the way from the top.
    const statements = Array.from(document.querySelectorAll<HTMLElement>('.statement'))
    let frame = 0
    const light = () => {
      frame = 0
      const h = window.innerHeight
      for (const s of statements) {
        const words = s.querySelectorAll('.w')
        const r = s.getBoundingClientRect()
        const middle = r.top + r.height / 2
        const progress = Math.min(1, Math.max(0, (h * 0.9 - middle) / (h * 0.55)))
        const lit = Math.round(progress * words.length)
        words.forEach((w, i) => w.classList.toggle('lit', i < lit))
      }
    }
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(light)
    }
    if (statements.length) {
      root.classList.add('statements')
      light()
      window.addEventListener('scroll', onScroll, { passive: true })
      window.addEventListener('resize', onScroll)
    }

    return () => {
      seen.disconnect()
      window.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', onScroll)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [])

  return null
}
