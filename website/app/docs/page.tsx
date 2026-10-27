import type { Metadata } from 'next'
import Link from 'next/link'
import { repoDoc } from '../site-config'
import { SECTIONS } from './content'

export const metadata: Metadata = {
  title: 'Docs',
  description: 'How to get QurbSpace going, use it every day, keep it safe, and fix what goes wrong.'
}

export default function DocsHome() {
  return (
    <article className="dk">
      <h1>Docs</h1>
      <p className="doc-summary">Everything about QurbSpace, one small step at a time.</p>
      {SECTIONS.map((s) => (
        <section key={s.name} className="doc-group">
          <h2>{s.name}</h2>
          <div className="doc-cards">
            {s.docs.map((d) => (
              <Link key={d.slug} href={`/docs/${d.slug}`} className="doc-card">
                <b>{d.title}</b>
                <span>{d.summary}</span>
              </Link>
            ))}
          </div>
        </section>
      ))}
      <section className="doc-group">
        <h2>For developers</h2>
        <a href={repoDoc('docs/CODEBASE.md')} className="doc-card doc-card-dev">
          <b>How QurbSpace is built →</b>
          <span>The code, its design, and every decision behind it, on GitHub.</span>
        </a>
      </section>
    </article>
  )
}
