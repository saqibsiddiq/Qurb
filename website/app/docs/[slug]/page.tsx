import type { Metadata } from 'next'
import Link from 'next/link'
import { notFound } from 'next/navigation'
import { ALL } from '../content'

export function generateStaticParams() {
  return ALL.map((d) => ({ slug: d.slug }))
}

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params
  const d = ALL.find((x) => x.slug === slug)
  return d ? { title: d.title, description: d.summary } : {}
}

export default async function DocPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params
  const i = ALL.findIndex((x) => x.slug === slug)
  if (i < 0) notFound()
  const d = ALL[i]
  const prev = ALL[i - 1]
  const next = ALL[i + 1]
  return (
    <article className="dk">
      <nav className="doc-crumbs" aria-label="Breadcrumb">
        <Link href="/docs">Docs</Link>
        <span>/</span>
        {d.section}
      </nav>
      <h1>{d.title}</h1>
      <p className="doc-summary">{d.summary}</p>
      <div className="doc-body">{d.body}</div>
      <nav className="doc-pager">
        {prev ? (
          <Link href={`/docs/${prev.slug}`} className="doc-prev">
            <small>‹ Previous</small>
            {prev.title}
          </Link>
        ) : (
          <span />
        )}
        {next && (
          <Link href={`/docs/${next.slug}`} className="doc-next">
            <small>Next ›</small>
            {next.title}
          </Link>
        )}
      </nav>
    </article>
  )
}
