type FAQSectionProps = {
  items: Array<{
    question: string
    answer: string
  }>
}

export function FAQSection({ items }: FAQSectionProps) {
  return (
    <section id="faq" className="py-16 md:py-24">
      <div className="mb-8 max-w-3xl">
        <div className="mb-3 text-[11px] font-medium uppercase tracking-[0.32em] text-[var(--gold)]">FAQ</div>
        <h2 className="serif-display text-3xl leading-tight text-[var(--text)] md:text-5xl">
          Questions that come up when privacy becomes the product.
        </h2>
      </div>

      <div className="grid gap-3">
        {items.map((item) => (
          <details key={item.question} className="group rounded-[22px] border border-[var(--border)] bg-[var(--panel)] p-5">
            <summary className="cursor-pointer list-none text-base font-medium text-[var(--text)]">
              {item.question}
            </summary>
            <p className="mt-3 text-sm leading-6 text-[var(--muted)]">{item.answer}</p>
          </details>
        ))}
      </div>
    </section>
  )
}
