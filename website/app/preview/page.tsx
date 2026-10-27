import type { Metadata } from 'next'
import { AlwaysNear } from '../../components/always-near'
import { Backups } from '../../components/backups'
import { Depth } from '../../components/depth'
import { Family } from '../../components/family'
import { GetIt } from '../../components/get-it'
import { NearField } from '../../components/near-field'
import { NoMiddle } from '../../components/no-middle'
import { SiteFooter, SiteNav } from '../../components/qs-chrome'
import { Rise } from '../../components/rise'
import { SendPlane } from '../../components/send-plane'
import { Vault } from '../../components/vault'
import { Walkthrough } from '../../components/walkthrough'
import './preview.css'

// The new landing page, scene by scene, while it is being designed with the
// owner (docs/design/website-brief.md). Not linked from anywhere and kept out
// of search; it replaces app/page.tsx once approved.
export const metadata: Metadata = {
  title: 'QurbSpace preview',
  robots: { index: false, follow: false }
}

export default function Preview() {
  return (
    <main className="qs">
      <NearField />
      <Depth />
      <SiteNav />

      <section className="qs-hero">
        <Rise as="h1" lines={['Your files.', 'Always near.']} accent={1} />
        <p>Everything on your computer, right in your pocket.</p>
        <AlwaysNear />
      </section>

      <Walkthrough chapter="connect" />

      <Walkthrough chapter="remote" />

      <Walkthrough chapter="change" />

      <SendPlane />

      <Backups />

      <Vault />

      <Family />

      <NoMiddle />

      <GetIt />

      {/* The end: the page's dots settle into the name (near-field.tsx). */}
      <div className="qs-end" data-settle aria-hidden="true">
        QurbSpace
      </div>

      <SiteFooter />
    </main>
  )
}
