import { LegalPage } from '@/components/landing/legal-page'

export default function GitHubPage() {
  return (
    <LegalPage
      title="GitHub"
      intro="A placeholder GitHub landing page that can later point to the public repository, issue tracker, or project discussions."
      sections={[
        {
          heading: 'Repository',
          body: 'This page will eventually link to the public source code and the engineering conversation around the project.'
        },
        {
          heading: 'Community',
          body: 'Contributors and interested users can be directed here when the project transition from private iteration to public collaboration begins.'
        }
      ]}
    />
  )
}
