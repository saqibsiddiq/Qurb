'use client'

import { useCallback, useState, type ReactNode } from 'react'
import { DocsHeader } from './docs-header'
import { DocsNav } from './docs-nav'
import { DocsToc } from './docs-toc'

// The docs' frame: top bar, sidebar, the page, and On this page.
type Section = { name: string; docs: { slug: string; title: string; summary: string }[] }
export function DocsShell({ sections, children }: { sections: Section[]; children: ReactNode }) {
  const [menu, setMenu] = useState(false)
  const close = useCallback(() => setMenu(false), [])
  const pages = sections.flatMap((s) => s.docs.map((d) => ({ ...d, section: s.name })))
  return (
    <>
      <DocsHeader pages={pages} onMenu={() => setMenu(true)} />
      <div className="docs">
        <DocsNav sections={sections} open={menu} onClose={close} />
        <div className="docs-main">{children}</div>
        <DocsToc />
      </div>
    </>
  )
}
