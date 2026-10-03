import type { Metadata } from 'next'
import Link from 'next/link'
import { DocPage, NotWritten } from '@/components/doc-page'

export const metadata: Metadata = {
  title: 'Terms',
  description: 'There is nothing to sign up for, so there are no terms yet.'
}

export default function TermsPage() {
  return (
    <DocPage title="Terms" intro="There’s nothing to sign up for, so there are no terms of service yet.">
      <NotWritten>
        Terms come with a service, and Qurb has none: it is software you build and run on your own devices. What you may
        do with the code is its licence — see <Link href="/license-information">licences</Link>.
      </NotWritten>
    </DocPage>
  )
}
