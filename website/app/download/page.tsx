import { LegalPage } from '@/components/landing/legal-page'

export default function DownloadPage() {
  return (
    <LegalPage
      title="Download"
      intro="This placeholder route will later host early-access desktop builds, installer notes, and platform-specific release information."
      sections={[
        {
          heading: 'Current status',
          body: 'Downloads are intentionally disabled for now while the product remains in a private, founder-led development phase.'
        },
        {
          heading: 'What will appear here later',
          body: 'Expected content includes platform-specific installation files, checksum notes, and release announcements specific to desktop and mobile previews.'
        }
      ]}
    />
  )
}
