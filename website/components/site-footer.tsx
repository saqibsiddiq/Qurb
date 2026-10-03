import Link from 'next/link'
import { repoUrl } from '@/app/site-config'
import { Mark } from './mark'

const columns = [
  [
    'Qurb',
    [
      ['How it works', '/#how'],
      ['Get Qurb', '/download'],
      ['Docs', '/docs'],
      ['Status', '/status'],
      ['Notes', '/blog']
    ]
  ],
  [
    'Project',
    [
      ['GitHub', repoUrl],
      ['Security', '/security-policy'],
      ['Reporting a vulnerability', '/responsible-disclosure'],
      ['Licences', '/license-information'],
      ['Contact', '/contact']
    ]
  ],
  [
    'This site',
    [
      ['Privacy', '/privacy-policy'],
      ['Cookies', '/cookie-policy'],
      ['Terms', '/terms-of-service']
    ]
  ]
] as const

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="wrap">
        <div className="cols">
          <div>
            <Link href="/" className="brand">
              <Mark size={26} />
              <span>Qurb</span>
            </Link>
            <p className="tagline">
              Your files. Your devices. Your space.
              <br />
              Built in the open, for Linux and Android first.
            </p>
          </div>
          {columns.map(([title, links]) => (
            <div key={title}>
              <h4>{title}</h4>
              <ul>
                {links.map(([label, href]) => (
                  <li key={label}>
                    {href.startsWith('http') ? <a href={href}>{label}</a> : <Link href={href}>{label}</Link>}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <p className="small">© 2026 Qurb. Inter is under the SIL Open Font License; the icons are Lucide’s, under ISC.</p>
      </div>
    </footer>
  )
}
