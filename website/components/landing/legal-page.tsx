import Link from 'next/link'
import { Container } from '@/components/landing/container'

type LegalSection = {
  heading: string
  body: string
}

type LegalPageProps = {
  title: string
  intro: string
  sections: LegalSection[]
}

export function LegalPage({ title, intro, sections }: LegalPageProps) {
  return (
    <main className="min-h-screen px-4 py-12 sm:px-6 lg:px-8">
      <Container>
        <div className="rounded-[32px] border border-[var(--border)] bg-[var(--panel)] p-6 md:p-10">
          <div className="mb-8">
            <div className="mb-3 text-[11px] uppercase tracking-[0.32em] text-[var(--gold)]">Placeholder policy page</div>
            <h1 className="serif-display text-4xl leading-tight text-[var(--text)] md:text-5xl">{title}</h1>
            <p className="mt-4 max-w-2xl text-base leading-7 text-[var(--muted)]">{intro}</p>
          </div>

          <div className="grid gap-5">
            {sections.map((section) => (
              <section key={section.heading} className="rounded-[24px] border border-[var(--border)] bg-[var(--panel-strong)] p-5">
                <h2 className="mb-2 text-lg font-semibold text-[var(--text)]">{section.heading}</h2>
                <p className="text-sm leading-6 text-[var(--muted)]">{section.body}</p>
              </section>
            ))}
          </div>

          <div className="mt-8 text-sm text-[var(--muted)]">
            Return to the <Link href="/" className="text-[var(--accent)]">landing page</Link> when you are ready to expand this content.
          </div>
        </div>
      </Container>
    </main>
  )
}
