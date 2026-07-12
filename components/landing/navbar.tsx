const links = [
  ['About', '#about'],
  ['Contact', '/contact'],
  ['GitHub', '/github'],
  ['Sign Up', '#newsletter']
]

export function Navbar() {
  return (
    <header className="border-b border-[var(--border)] bg-[var(--panel)] backdrop-blur-sm">
      <div className="mx-auto flex max-w-6xl items-center justify-between px-4 py-4 sm:px-6 lg:px-8">
        <a href="#top" className="flex items-center gap-3 text-sm font-semibold text-[var(--text)]">
          <span className="flex h-9 w-9 items-center justify-center rounded-2xl border border-[var(--border)] bg-[var(--bg-elevated)] shadow-sm">
            <span className="text-base font-semibold text-[var(--accent)]">Q</span>
          </span>
          Qurb Cloud
        </a>

        <nav aria-label="Primary navigation" className="hidden items-center gap-5 text-sm md:flex">
          {links.map(([label, href]) => (
            <a key={label} href={href} className="font-semibold text-[var(--text)] transition hover:text-[var(--accent)]">
              {label}
            </a>
          ))}
        </nav>
      </div>
    </header>
  )
}
