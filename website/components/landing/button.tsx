type ButtonProps = {
  href: string
  children: React.ReactNode
  variant?: 'primary' | 'secondary'
}

export function Button({ href, children, variant = 'primary' }: ButtonProps) {
  const variants = {
    primary:
      'border border-[var(--accent)] bg-[var(--accent)] text-[#f8f4ed] hover:bg-transparent hover:text-[var(--accent)]',
    secondary:
      'border border-[var(--border)] bg-[var(--panel)] text-[var(--text)] hover:border-[var(--accent)] hover:text-[var(--accent)]'
  }

  return (
    <a
      href={href}
      className={`inline-flex items-center justify-center rounded-full px-5 py-3 text-sm font-medium transition duration-300 ${variants[variant]}`}
    >
      {children}
    </a>
  )
}
