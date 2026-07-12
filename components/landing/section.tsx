type SectionProps = {
  id?: string
  eyebrow: string
  title: string
  description: string
  children: React.ReactNode
}

export function Section({ id, eyebrow, title, description, children }: SectionProps) {
  return (
    <section id={id} className="py-16 md:py-24">
      <div className="mb-8 max-w-3xl">
        <div className="mb-3 text-[11px] font-medium uppercase tracking-[0.32em] text-[var(--gold)]">
          {eyebrow}
        </div>
        <h2 className="serif-display text-3xl leading-tight text-[var(--text)] md:text-5xl">
          {title}
        </h2>
        <p className="mt-4 max-w-2xl text-base leading-7 text-[var(--muted)] md:text-lg">
          {description}
        </p>
      </div>
      {children}
    </section>
  )
}
