import { LegalPage } from '@/components/landing/legal-page'

export default function DocsPage() {
  return (
    <LegalPage
      title="Documentation"
      intro="A placeholder documentation hub for installation guidance, architecture notes, and user support content."
      sections={[
        {
          heading: 'Getting started',
          body: 'This space will eventually contain onboarding instructions, setup steps, privacy notes, configuration examples, and support references.'
        },
        {
          heading: 'Developer resources',
          body: 'Engineers will later be able to browse API references, client workflows, and deployment guidance for different hardware configurations.'
        }
      ]}
    />
  )
}
