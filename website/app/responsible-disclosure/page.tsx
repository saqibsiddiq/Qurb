import type { Metadata } from 'next'
import Link from 'next/link'
import { DocPage } from '@/components/doc-page'
import { Icon } from '@/components/icon'
import { repoUrl } from '../site-config'

export const metadata: Metadata = {
  title: 'Reporting a vulnerability',
  description: 'How to tell the people building Qurb about a security problem without making it public.'
}

export default function DisclosurePage() {
  return (
    <DocPage
      title="Reporting a vulnerability"
      intro="If you’ve found a way into someone’s files, keys or devices through Qurb, thank you — and please tell us before anyone else."
    >
      <h2>How</h2>
      <p>
        <strong>Please don’t describe it in a public issue.</strong> The repository doesn’t accept private
        reports through GitHub yet. Until it does,{' '}
        <a href={`${repoUrl}/issues/new`}>open an issue</a> that says only that you have a security problem to report,
        with no details, and a private way to send them will be arranged with you there.
      </p>
      <p className="callout draft">
        <Icon name="clock" size={20} />
        <span>
          <b>No bounty, and no promised response time yet.</b> Qurb is pre-release and has no company behind it. What it
          can promise is that a report is taken seriously, fixed in the open, and credited if you want it to be.
        </span>
      </p>

      <h2>What helps</h2>
      <ul>
        <li>What you did, on which devices and which version — every build says its own: <code>qurb version</code>, or Settings → Version.</li>
        <li>What you could reach that you shouldn’t have: files, filenames, keys, a device you weren’t paired with.</li>
        <li>Whether it needs the other person’s device, their network, or neither.</li>
      </ul>

      <h2>What Qurb already says about itself</h2>
      <p>
        Some limits are known and written down rather than hidden — your devices share one key, and removing a device
        doesn’t take that key away. They’re on the <Link href="/security-policy">security page</Link>; a report
        about one of them is still welcome if it goes further than the page says.
      </p>
    </DocPage>
  )
}
