'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { useEffect } from 'react'

// The docs' sidebar: every page by section, the open one marked. On a phone
// it slides in from the left when the top bar's menu is pressed.
type Item = { slug: string; title: string }
export function DocsNav({ sections, open, onClose }: { sections: { name: string; docs: Item[] }[]; open: boolean; onClose: () => void }) {
  const path = usePathname()
  useEffect(() => onClose(), [path, onClose])
  return (
    <>
      <div className={`dn-shade${open ? ' on' : ''}`} onClick={onClose} />
      <aside className={`dn${open ? ' dn-open' : ''}`}>
        <Link href="/docs" className={`dn-home${path === '/docs' ? ' on' : ''}`}>
          Overview
        </Link>
        {sections.map((s) => (
          <div key={s.name} className="dn-section">
            <span>{s.name}</span>
            {s.docs.map((d) => (
              <Link key={d.slug} href={`/docs/${d.slug}`} className={path === `/docs/${d.slug}` ? 'on' : ''}>
                {d.title}
              </Link>
            ))}
          </div>
        ))}
      </aside>
    </>
  )
}
