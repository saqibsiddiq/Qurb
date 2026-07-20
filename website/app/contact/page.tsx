import { LegalPage } from '@/components/landing/legal-page'

export default function ContactPage() {
  return (
    <LegalPage
      title="Contact"
      intro="A placeholder contact route for founders, partners, press, and early-access conversations."
      sections={[
        {
          heading: 'How to reach the team',
          body: 'Replace this placeholder with a contact form, a direct email address, or a lightweight support entry point as the company grows.'
        },
        {
          heading: 'What to expect',
          body: 'The product is intentionally small and founder-led at this stage, so this page can stay calm and direct while the business expands.'
        }
      ]}
    />
  )
}
