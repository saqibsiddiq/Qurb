import type { Metadata } from 'next'
import { DocPage } from '@/components/doc-page'
import { Icon } from '@/components/icon'

export const metadata: Metadata = {
  title: 'Status',
  description: 'Qurb runs no service of its own, so there is nothing here to be up or down.'
}

export default function StatusPage() {
  return (
    <DocPage
      title="Status"
      intro="Qurb runs no service of its own yet, so there’s nothing here to be up or down."
    >
      <p className="callout">
        <Icon name="info" size={20} />
        <span>
          <b>Your devices depend on you, not on us.</b> They sync directly with each other. The small service that
          introduces them across networks — and the relay, if you use one — run on a computer or server of your own, so
          their status is on that machine. On any device, <code>qurb status</code> says what it holds and which devices
          it trusts.
        </span>
      </p>
      <p>When Qurb runs a service people rely on, its status will be here, with its history.</p>
    </DocPage>
  )
}
