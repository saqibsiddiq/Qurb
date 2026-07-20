export function Footer() {
  return (
    <footer className="border-t border-[var(--border)] bg-[var(--panel)]">
      <div className="mx-auto flex max-w-6xl flex-col gap-4 px-4 py-6 sm:px-6 lg:flex-row lg:items-center lg:justify-between lg:px-8">
        <div className="flex items-center gap-3 text-sm font-semibold text-[var(--text)]">
          <span className="flex h-9 w-9 items-center justify-center rounded-2xl border border-[var(--border)] bg-[var(--bg-elevated)] shadow-sm">
            <span className="text-base font-semibold text-[var(--accent)]">Q</span>
          </span>
          Qurb Cloud
        </div>

        <div className="text-sm font-medium text-[var(--muted)]">© 2026 Qurb Cloud</div>

        <div className="flex flex-wrap gap-4 text-sm font-medium text-[var(--text)]">
          <a href="/privacy-policy" className="transition hover:text-[var(--accent)]">Privacy Policy</a>
          <a href="/terms-of-service" className="transition hover:text-[var(--accent)]">Terms</a>
          <a href="/contact" className="transition hover:text-[var(--accent)]">Contact</a>
          <a href="/github" className="transition hover:text-[var(--accent)]">GitHub</a>
        </div>
      </div>
    </footer>
  )
}
