'use client'

import Link from 'next/link'
import { useRouter } from 'next/navigation'
import { useEffect, useRef, useState } from 'react'
import { HOME } from './qs-chrome'

// The docs' own top bar, as documentation sites have one: the name, a search
// that opens with Ctrl K or ⌘K and finds a page as you type, and the way back.
type Page = { slug: string; title: string; summary: string; section: string }

export function DocsHeader({ pages, onMenu }: { pages: Page[]; onMenu?: () => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [at, setAt] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  const router = useRouter()
  const [mac, setMac] = useState(false)

  useEffect(() => {
    setMac(/Mac|iPhone|iPad/.test(navigator.userAgent))
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setOpen((o) => !o)
      }
      if (e.key === 'Escape') setOpen(false)
    }
    addEventListener('keydown', key)
    return () => removeEventListener('keydown', key)
  }, [])
  useEffect(() => {
    if (open) {
      setQ('')
      setAt(0)
      setTimeout(() => input.current?.focus(), 0)
    }
  }, [open])

  const words = q.trim().toLowerCase()
  const found = pages.filter((p) => !words || `${p.title} ${p.summary} ${p.section}`.toLowerCase().includes(words)).slice(0, 8)
  const go = (p: Page) => {
    setOpen(false)
    router.push(`/docs/${p.slug}`)
  }

  return (
    <>
      <header className="dh">
        <div className="dh-in">
          <button type="button" className="dh-menu" onClick={onMenu} aria-label="All topics">
            <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M4 7h16M4 12h16M4 17h16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </button>
          <Link href={HOME} className="dh-brand">
            QurbSpace
          </Link>
          <span className="dh-slash">/</span>
          <Link href="/docs" className="dh-docs">
            Docs
          </Link>
          <button type="button" className="dh-search" onClick={() => setOpen(true)}>
            <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" strokeWidth="2" />
              <path d="M20 20l-3.5-3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
            <span>Search docs</span>
            <kbd>{mac ? '⌘K' : 'Ctrl K'}</kbd>
          </button>
          <nav className="dh-links">
            <Link href={HOME}>Home</Link>
            <a href="/github">GitHub</a>
            <Link className="dh-cta" href={`${HOME}#get`}>
              Get QurbSpace
            </Link>
          </nav>
        </div>
      </header>

      {open && (
        <div className="dk-palette" role="dialog" aria-label="Search the docs" onClick={() => setOpen(false)}>
          <div className="dk-box" onClick={(e) => e.stopPropagation()}>
            <input
              ref={input}
              value={q}
              placeholder="What are you looking for?"
              onChange={(e) => {
                setQ(e.target.value)
                setAt(0)
              }}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') setAt((a) => Math.min(found.length - 1, a + 1))
                if (e.key === 'ArrowUp') setAt((a) => Math.max(0, a - 1))
                if (e.key === 'Enter' && found[at]) go(found[at])
              }}
            />
            <div className="dk-results">
              {found.map((p, i) => (
                <button type="button" key={p.slug} className={i === at ? 'on' : ''} onMouseEnter={() => setAt(i)} onClick={() => go(p)}>
                  <small>{p.section}</small>
                  <b>{p.title}</b>
                  <span>{p.summary}</span>
                </button>
              ))}
              {!found.length && <p className="dk-none">Nothing matches “{q}”.</p>}
            </div>
          </div>
        </div>
      )}
    </>
  )
}
