import type { Metadata } from 'next'
import Link from 'next/link'
import { DocPage, LinkRow } from '@/components/doc-page'
import { repoUrl } from '../site-config'

export const metadata: Metadata = {
  title: 'Contact',
  description: 'How to reach the people building Qurb.'
}

export default function ContactPage() {
  return (
    <DocPage title="Contact" intro="Qurb is built in the open, and the way to reach the people building it is its repository.">
      <div className="rows">
        <LinkRow
          href={`${repoUrl}/issues/new`}
          icon="plus"
          title="Open an issue"
          text="A bug, a question, or something you’d like Qurb to do"
        />
        <LinkRow href={`${repoUrl}/issues`} icon="search" title="See what’s already open" text="Someone may have asked already" />
      </div>
      <p>
        Found a security problem? Please don’t open a public issue for it — see{' '}
        <Link href="/responsible-disclosure">reporting a vulnerability</Link>.
      </p>
    </DocPage>
  )
}
