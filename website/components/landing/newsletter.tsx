export function Newsletter() {
  return (
    <section id="newsletter" className="py-16 md:py-24">
      <div className="rounded-[30px] border border-[var(--border)] bg-[var(--panel)] p-6 md:p-8">
        <div className="mx-auto max-w-2xl text-center">
          <div className="mb-3 text-[11px] uppercase tracking-[0.32em] text-[var(--gold)]">Join the waitlist</div>
          <h2 className="serif-display text-3xl leading-tight text-[var(--text)] md:text-4xl">
            Be the first to try it.
          </h2>
          <p className="mt-3 text-sm leading-6 text-[var(--muted)]">
            Currently in development. Join the list and follow the journey as Qurb Cloud takes shape.
          </p>
          <form className="mt-6 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <label className="sr-only" htmlFor="email">Email</label>
            <input
              id="email"
              type="email"
              placeholder="you@example.com"
              className="w-full max-w-[320px] rounded-full border border-[var(--border)] bg-[var(--panel-strong)] px-4 py-3 text-sm outline-none placeholder:text-[var(--muted)] focus:border-[var(--accent)]"
            />
            <button type="button" className="rounded-full bg-[var(--accent)] px-5 py-3 text-sm font-medium text-[#f8f4ed] transition hover:bg-[var(--accent)]/90">
              Join the waitlist
            </button>
          </form>
        </div>
      </div>
    </section>
  )
}
