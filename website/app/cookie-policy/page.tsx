import type { Metadata } from 'next'
import { DocPage } from '@/components/doc-page'

export const metadata: Metadata = {
  title: 'Cookies',
  description: 'This site sets no cookies.'
}

export default function CookiesPage() {
  return (
    <DocPage title="Cookies" intro="This site sets no cookies, so there is nothing to accept or refuse.">
      <p>
        No analytics, no advertising, no tracking pixels, and nothing loaded from another site that could set one of its
        own. If that ever changes, this page will say so first.
      </p>
    </DocPage>
  )
}
