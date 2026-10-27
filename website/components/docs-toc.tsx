'use client'

import { usePathname } from 'next/navigation'
import { useEffect, useState } from 'react'

// "On this page": the page's headings, the one being read marked as it
// scrolls past. A page with fewer than two headings shows none.
export function DocsToc() {
  const path = usePathname()
  const [heads, setHeads] = useState<{ id: string; text: string }[]>([])
  const [active, setActive] = useState('')

  useEffect(() => {
    const els = [...document.querySelectorAll<HTMLHeadingElement>('.doc-body h2')]
    els.forEach((h) => {
      if (!h.id) h.id = h.textContent!.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().replace(/ /g, '_')
    })
    setHeads(els.map((h) => ({ id: h.id, text: h.textContent! })))
    setActive(els[0]?.id ?? '')
    const seen = new IntersectionObserver(
      (entries) => {
        const top = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]
        if (top) setActive(top.target.id)
      },
      { rootMargin: '-80px 0px -60% 0px' }
    )
    els.forEach((h) => seen.observe(h))
    return () => seen.disconnect()
  }, [path])

  if (heads.length < 2) return <div className="dt" />
  return (
    <nav className="dt" aria-label="On this page">
      <span>On this page</span>
      {heads.map((h) => (
        <a key={h.id} href={`#${h.id}`} className={h.id === active ? 'on' : ''}>
          {h.text}
        </a>
      ))}
    </nav>
  )
}
