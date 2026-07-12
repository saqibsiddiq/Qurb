import { Container } from '@/components/landing/container'
import { Footer } from '@/components/landing/footer'
import { Hero } from '@/components/landing/hero'
import { Newsletter } from '@/components/landing/newsletter'
import { PatternBackground } from '@/components/landing/pattern-background'
import { ProductPreview } from '@/components/landing/product-preview'
import { Section } from '@/components/landing/section'
import { Navbar } from '@/components/landing/navbar'

export default function HomePage() {
  return (
    <main className="relative overflow-hidden text-[var(--text)]">
      <PatternBackground />
      <div className="relative z-10">
        <Navbar />
        <Hero />

        <Container>
          <Section
            id="preview"
            eyebrow="Product preview"
            title="A quiet first glance at the idea."
            description="No screenshots. No promises. Just a simple, intentional placeholder for the private cloud experience."
          >
            <ProductPreview />
          </Section>

          <Section
            id="about"
            eyebrow="About"
            title="Qurb Cloud is a simple idea with a clear point of view."
            description="It is a private cloud built around ownership. Your desktop becomes the place your files live, and your devices stay connected to that home with care. Privacy is not a feature list. It is the foundation."
          >
            <p className="max-w-3xl text-base leading-7 text-[var(--muted)] md:text-lg">
              The product is still being shaped, but the philosophy is already clear: keep things close, keep them personal, and keep them under your control.
            </p>
          </Section>

          <Newsletter />
        </Container>

        <Footer />
      </div>
    </main>
  )
}
