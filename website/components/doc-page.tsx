import Link from 'next/link'
import type { ReactNode } from 'react'
import { Icon } from './icon'
import { SiteFooter } from './site-footer'
import { SiteHeader } from './site-header'

/**
 * A page of words: one frosted stage the text floats in, as the window’s
 * places sit in its stage (direction §37).
 */
export function DocPage({
  title,
  intro,
  current,
  children
}: {
  title: string
  intro: ReactNode
  current?: string
  children: ReactNode
}) {
  return (
    <div className="page">
      <SiteHeader current={current} />
      <main className="wrap">
        <article className="doc glass-frosted">
          <Link href="/" className="back">
            <Icon name="arrow-right" size={16} />
            Qurb
          </Link>
          <h1>{title}</h1>
          <p className="intro">{intro}</p>
          {children}
        </article>
      </main>
      <SiteFooter />
    </div>
  )
}

/** A page that is not written yet, saying so in the same words everywhere. */
export function NotWritten({ children }: { children: ReactNode }) {
  return (
    <p className="callout draft">
      <Icon name="clock" size={20} />
      <span>
        <b>Not written yet.</b> {children}
      </span>
    </p>
  )
}

/** A row that goes somewhere: the window’s row, as a link. */
export function LinkRow({ href, icon, title, text }: { href: string; icon: Parameters<typeof Icon>[0]['name']; title: string; text: string }) {
  return (
    <a className="row" href={href}>
      <span className="tile">
        <Icon name={icon} />
      </span>
      <span>
        <span className="name">{title}</span>
        <span className="sub">
          <span>{text}</span>
        </span>
      </span>
      <Icon name="chevron-right" size={18} />
    </a>
  )
}
