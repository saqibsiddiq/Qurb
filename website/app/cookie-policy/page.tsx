import { LegalPage } from '@/components/landing/legal-page'

export default function CookiePolicyPage() {
  return (
    <LegalPage
      title="Cookie Policy"
      intro="This placeholder page explains the later use of operational cookies, local preferences, and session persistence for a privacy-conscious product."
      sections={[
        {
          heading: 'What cookies are used',
          body: 'The final page will distinguish between necessary system cookies, optional analytics cookies, and user preference storage.'
        },
        {
          heading: 'User preferences',
          body: 'Users should be able to adjust settings and understand the impact of those choices as the product matures.'
        },
        {
          heading: 'Third-party tools',
          body: 'Any third-party integrations will be clearly disclosed and kept minimal so the website can remain a calm and privacy-respecting experience.'
        }
      ]}
    />
  )
}
