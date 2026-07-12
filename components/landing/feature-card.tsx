type FeatureCardProps = {
  title: string
  description: string
  icon: string
}

export function FeatureCard({ title, description, icon }: FeatureCardProps) {
  return (
    <article className="group rounded-[24px] border border-[var(--border)] bg-[var(--panel)] p-5 transition duration-300 hover:-translate-y-0.5 hover:border-[var(--accent)]">
      <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-[var(--border)] bg-[var(--panel-strong)] text-[var(--accent)]">
        <span aria-hidden="true">{icon}</span>
      </div>
      <h3 className="mb-2 text-lg font-semibold text-[var(--text)]">{title}</h3>
      <p className="text-sm leading-6 text-[var(--muted)]">{description}</p>
    </article>
  )
}
