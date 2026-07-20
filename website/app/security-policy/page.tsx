import { LegalPage } from '@/components/landing/legal-page'

export default function SecurityPolicyPage() {
  return (
    <LegalPage
      title="Security Policy"
      intro="A placeholder security policy that later describes encryption practices, reporting channels, and the company’s security posture."
      sections={[
        {
          heading: 'Encryption and pairing',
          body: 'This page will explain the transport and storage security model behind desktop-to-phone pairing, private sync flows, and user-owned keys.'
        },
        {
          heading: 'Responsible handling',
          body: 'The company will later publish its security review process, software maintenance practices, and vulnerability management approach.'
        },
        {
          heading: 'Operational principles',
          body: 'The platform should remain transparent, deliberate, and minimal when it comes to data access, telemetry, and third-party dependencies.'
        }
      ]}
    />
  )
}
