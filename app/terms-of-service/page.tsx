import { LegalPage } from '@/components/landing/legal-page'

export default function TermsOfServicePage() {
  return (
    <LegalPage
      title="Terms of Service"
      intro="This placeholder agreement will, in time, describe the service relationship, supported use cases, and responsibilities of both the platform and the user."
      sections={[
        {
          heading: 'Service availability',
          body: 'The final terms will describe uptime expectations, support commitments, and what makes the product reliable in practice.'
        },
        {
          heading: 'User responsibilities',
          body: 'This section will discuss local hardware ownership, secure configuration, and appropriate use of private cloud infrastructure.'
        },
        {
          heading: 'Updates and changes',
          body: 'The platform will later communicate how product updates, roadmap changes, and policy revisions are shared with users.'
        }
      ]}
    />
  )
}
