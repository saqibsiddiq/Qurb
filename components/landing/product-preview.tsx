export function ProductPreview() {
  return (
    <div className="grid items-center gap-4 md:grid-cols-[1.15fr_0.85fr]">
      <div className="relative overflow-hidden rounded-[30px] border border-[var(--border)] bg-[var(--panel)] p-4 shadow-[var(--shadow)]">
        <div className="mb-4 flex items-center gap-2 text-[10px] uppercase tracking-[0.32em] text-[var(--muted)]">
          <span className="h-2 w-2 rounded-full bg-[var(--accent)]" />
          Desktop preview
        </div>
        <div className="rounded-[22px] border border-[var(--border)] bg-[var(--panel-strong)] p-4">
          <div className="mb-4 flex items-center gap-2 text-[11px] uppercase tracking-[0.28em] text-[var(--muted)]">
            <span className="h-1.5 w-1.5 rounded-full bg-[var(--gold)]" />
            Home computer
          </div>
          <div className="space-y-3">
            {[ 'Photos', 'Documents', 'Archive' ].map((name, index) => (
              <div key={name} className="flex items-center justify-between rounded-[16px] border border-[var(--border)] bg-[var(--bg-elevated)] px-4 py-3 text-sm text-[var(--text)]">
                <span>{name}</span>
                <span className="text-[11px] uppercase tracking-[0.24em] text-[var(--accent)]">
                  {index === 0 ? 'Synced' : 'Ready'}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="relative rounded-[28px] border border-[var(--border)] bg-[var(--panel)] p-4 shadow-[var(--shadow)]">
        <div className="mb-3 text-[10px] uppercase tracking-[0.32em] text-[var(--muted)]">Phone</div>
        <div className="mx-auto w-[180px] rounded-[28px] border border-[var(--border)] bg-[var(--bg-elevated)] p-3">
          <div className="h-6 rounded-full bg-[var(--panel)]" />
          <div className="mt-3 space-y-2">
            <div className="h-14 rounded-[18px] bg-[var(--panel-strong)]" />
            <div className="h-10 rounded-[14px] bg-[var(--panel-strong)]" />
            <div className="h-10 rounded-[14px] bg-[var(--panel-strong)]" />
          </div>
        </div>
        <div className="pointer-events-none absolute left-[26%] top-[50%] hidden h-px w-16 bg-[var(--accent)]/50 md:block" />
      </div>
    </div>
  )
}
