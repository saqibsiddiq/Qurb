'use client'

import Link from 'next/link'
import { useEffect, useState } from 'react'

// The top bar's menu on a small screen: an icon at the top right that opens
// the page's links, full screen, large enough for a thumb.
export function SiteMenu({ home }: { home: string }) {
  const [open, setOpen] = useState(false)
  useEffect(() => {
    document.documentElement.style.overflow = open ? 'hidden' : ''
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    addEventListener('keydown', key)
    return () => removeEventListener('keydown', key)
  }, [open])
  const close = () => setOpen(false)
  return (
    <>
      <button type="button" className={`qs-burger${open ? ' on' : ''}`} onClick={() => setOpen(!open)} aria-expanded={open} aria-label={open ? 'Close the menu' : 'Open the menu'}>
        <span />
        <span />
      </button>
      <div className={`qs-menu${open ? ' on' : ''}`} aria-hidden={!open}>
        <nav>
          <Link href={`${home}#how`} onClick={close}>
            How it works
          </Link>
          <Link href="/docs" onClick={close}>
            Docs
          </Link>
          <a href="/github" onClick={close}>
            GitHub
          </a>
        </nav>
        <Link href={`${home}#get`} className="qs-menu-cta" onClick={close}>
          Get QurbSpace
        </Link>
      </div>
    </>
  )
}
