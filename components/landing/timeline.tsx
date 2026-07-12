const steps = [
  { title: 'Desktop', note: 'Install the Qurb client on the computer you already own.' },
  { title: 'Pair Phone', note: 'Authorize your phone with secure credentials and a private key exchange.' },
  { title: 'Select Folder', note: 'Choose the folders that should become your cloud home.' },
  { title: 'Done', note: 'Your private sync system begins with calm, consistent backups.' }
]

export function Timeline() {
  return (
    <div className="grid gap-4 md:grid-cols-4">
      {steps.map((step, index) => (
        <div key={step.title} className="rounded-[24px] border border-[var(--border)] bg-[var(--panel)] p-5">
          <div className="mb-4 flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-full bg-[var(--accent-soft)] text-sm font-semibold text-[var(--accent)]">
              {index + 1}
            </div>
            <div className="text-xs uppercase tracking-[0.28em] text-[var(--gold)]">Step</div>
          </div>
          <div className="mb-2 text-lg font-semibold text-[var(--text)]">{step.title}</div>
          <p className="text-sm leading-6 text-[var(--muted)]">{step.note}</p>
        </div>
      ))}
    </div>
  )
}
