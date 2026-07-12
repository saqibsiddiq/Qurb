import { LegalPage } from '@/components/landing/legal-page'

export default function ResponsibleDisclosurePage() {
  return (
    <LegalPage
      title="Responsible Disclosure"
      intro="A placeholder report route for security researchers and community members who want to help improve the product responsibly."
      sections={[
        {
          heading: 'How to report a vulnerability',
          body: 'Replace this placeholder with a secure reporting process, contact address, and expected response window once the company begins formal security operations.'
        },
        {
          heading: 'Research expectations',
          body: 'This section will define the preferred way to coordinate a safe, respectful investigation without disrupting users or exposing sensitive data.'
        }
      ]}
    />
  )
}
