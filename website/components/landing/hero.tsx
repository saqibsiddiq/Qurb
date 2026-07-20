import { Button } from '@/components/landing/button'

export function Hero() {
  return (
    <section id="top" className="mx-auto max-w-6xl px-4 pb-16 pt-10 sm:px-6 lg:px-8 lg:pb-20 lg:pt-16">
      <div className="mx-auto max-w-4xl text-center">
        <div className="mb-5 inline-flex rounded-full border border-[var(--border)] bg-[var(--panel)] px-3 py-1 text-[10px] uppercase tracking-[0.34em] text-[var(--gold)]">
          Currently in development
        </div>

        <h1 className="serif-display mx-auto max-w-3xl text-5xl leading-[0.98] text-[var(--text)] sm:text-6xl md:text-7xl">
          Your cloud.
          <br />
          Owned by you.
        </h1>

        <p className="mx-auto mt-5 max-w-2xl text-base leading-7 text-[var(--muted)] md:text-lg">
          A private cloud platform that lets your desktop become your own quiet home for files.
        </p>

        <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <input
            aria-label="Email address"
            type="email"
            placeholder="Email address"
            className="w-full max-w-[300px] rounded-full border border-[var(--border)] bg-[var(--panel)] px-4 py-3 text-sm text-[var(--text)] outline-none transition focus:border-[var(--accent)] sm:w-[320px]"
          />
          <Button href="#newsletter">Join the waitlist</Button>
        </div>

        <div className="mt-4 text-sm text-[var(--muted)]">
          <a href="#about" className="transition hover:text-[var(--accent)]">
            Follow the journey
          </a>
        </div>
      </div>
    </section>
  )
}
