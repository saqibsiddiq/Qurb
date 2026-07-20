import { LegalPage } from '@/components/landing/legal-page'

export default function StatusPage() {
  return (
    <LegalPage
      title="Status"
      intro="A placeholder service-status page that can later report system health, release updates, and maintenance windows."
      sections={[
        {
          heading: 'Current state',
          body: 'This placeholder should soon be replaced with a real operations dashboard or a calm status feed for public incidents and roadmap progress.'
        },
        {
          heading: 'Operational transparency',
          body: 'The final status page should communicate reliability, maintenance notices, and the maturity of the platform communication model.'
        }
      ]}
    />
  )
}
