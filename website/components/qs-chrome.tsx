import Link from 'next/link'
import { SiteMenu } from './site-menu'

// The top bar and footer, shared by the landing page and the docs.
export const HOME = '/'

export function SiteNav() {
  return (
    <nav className="qs-nav">
      <Link href={HOME} className="qs-wordmark">
        QurbSpace
      </Link>
      <div className="qs-links">
        <Link href={`${HOME}#how`}>How it works</Link>
        <Link href="/docs">Docs</Link>
        <a href="/github">GitHub</a>
        <Link className="qs-cta" href={`${HOME}#get`}>
          Get QurbSpace
        </Link>
      </div>
      <SiteMenu home={HOME} />
    </nav>
  )
}

export function SiteFooter() {
  return (
    <footer className="qs-foot">
      <div>
        <b>QurbSpace</b>
        <small>
          Qurb (<span lang="ur">قرب</span>) means nearness.
        </small>
      </div>
      <nav>
        <Link href="/docs">Docs</Link>
        <a href="/github">GitHub</a>
        <Link href="/security-policy">Security</Link>
        <Link href="/license-information">Licences</Link>
      </nav>
    </footer>
  )
}
