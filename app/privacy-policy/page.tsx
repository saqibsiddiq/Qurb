import { LegalPage } from '@/components/landing/legal-page'

export default function PrivacyPolicyPage() {
  return (
    <LegalPage
      title="Privacy Policy"
      intro="This placeholder content will later describe how device pairing, file access, metadata handling, and support interactions are managed securely."
      sections={[
        {
          heading: 'Data collection',
          body: 'Qurb Cloud will describe what data is collected, when it is stored locally versus remotely, and how the platform avoids unnecessary telemetry.'
        },
        {
          heading: 'Device and account controls',
          body: 'This section will explain account provisioning, paired devices, encryption keys, and the user’s rights to review, delete, or export their data.'
        },
        {
          heading: 'Retention and deletion',
          body: 'The final policy can explain how backups, version history, and deleted files are retained or removed according to clear user intent.'
        }
      ]}
    />
  )
}
