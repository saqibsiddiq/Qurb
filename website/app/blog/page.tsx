import { LegalPage } from '@/components/landing/legal-page'

export default function BlogPage() {
  return (
    <LegalPage
      title="Blog"
      intro="A placeholder publishing space for product updates, technical notes, and founder writing."
      sections={[
        {
          heading: 'This page is reserved for future editorial content',
          body: 'In time, it can host announcement posts, architecture notes, product progress reports, and privacy-first writing that reflects the company voice.'
        },
        {
          heading: 'Planned structure',
          body: 'The blog will likely include a newsletter-style feed, article cards, and a single-post layout that supports thoughtful, evergreen writing.'
        }
      ]}
    />
  )
}
