import Link from 'next/link'
import { repoUrl } from '@/app/site-config'
import { Icon } from './icon'
import { Mark } from './mark'

const places = [
  ['How it works', '/#how'],
  ['Sending', '/#send'],
  ['People', '/#people'],
  ['Privacy', '/#privacy'],
  ['Today', '/#today'],
  ['Docs', '/docs']
] as const

/** The header: clear glass floating over the page, as the sidebar floats over
 *  the window. `current` lights the place being looked at. */
export function SiteHeader({ current }: { current?: string }) {
  return (
    <header className="site-header">
      <div className="wrap">
        <div className="bar glass-clear">
          <Link href="/" className="brand" aria-label="Qurb, home">
            <Mark size={28} />
            <span>Qurb</span>
          </Link>
          <nav className="site-nav" aria-label="Main">
            {places.map(([label, href]) => (
              <Link key={href} href={href} aria-current={current === href ? 'page' : undefined}>
                {label}
              </Link>
            ))}
          </nav>
          <a className="icon-link" href={repoUrl} aria-label="The code, on GitHub" title="The code, on GitHub">
            <Icon name="external-link" />
          </a>
          <Link className="btn primary small" href="/download">
            Get Qurb
          </Link>
        </div>
      </div>
    </header>
  )
}
