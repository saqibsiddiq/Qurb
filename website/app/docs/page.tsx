import type { Metadata } from 'next'
import { DocPage, LinkRow } from '@/components/doc-page'
import { repoDoc, repoUrl } from '../site-config'

export const metadata: Metadata = {
  title: 'Docs',
  description: "Qurb’s documentation: how it works, what it does today, and why each choice was made."
}

export default function DocsPage() {
  return (
    <DocPage
      title="Docs"
      current="/docs"
      intro="Qurb’s documentation lives beside its code, so it changes when the code does. These are the places to start."
    >
      <h2>Using it</h2>
      <div className="rows">
        <LinkRow
          href={repoDoc('docs/features.md')}
          icon="circle-check"
          title="What Qurb does today"
          text="Every feature, where you meet it, and how far it has been checked"
        />
        <LinkRow
          href={repoDoc('docs/trying-it.md')}
          icon="laptop"
          title="Trying it on your own hardware"
          text="From one machine to a phone, step by step"
        />
        <LinkRow
          href={repoDoc('docs/anywhere.md')}
          icon="arrow-up-down"
          title="Syncing from outside the house"
          text="What must be reachable, and what never needs to be"
        />
        <LinkRow
          href={repoDoc('docs/glossary.md')}
          icon="search"
          title="Glossary"
          text="Every term, defined plainly"
        />
      </div>

      <h2>How it works</h2>
      <div className="rows">
        <LinkRow
          href={repoDoc('docs/CODEBASE.md')}
          icon="folder-open"
          title="Understanding Qurb from scratch"
          text="The one document to read first, if you know nothing about it"
        />
        <LinkRow
          href={repoDoc('docs/architecture.md')}
          icon="monitor-smartphone"
          title="The design"
          text="Every part of the system, including what is not built yet"
        />
        <LinkRow
          href={repoDoc('docs/design/direction.md')}
          icon="settings"
          title="The design direction"
          text="How the apps look and move, and why"
        />
      </div>

      <h2>Why it is the way it is</h2>
      <div className="rows">
        <LinkRow
          href={`${repoUrl}/tree/main/docs/decisions`}
          icon="git-compare"
          title="Decisions"
          text="Each choice with lasting consequences, numbered, with its reasons and its costs"
        />
        <LinkRow
          href={`${repoUrl}/tree/main/docs/phases`}
          icon="history"
          title="What each phase built and measured"
          text="With the conditions every number was measured under"
        />
        <LinkRow
          href={repoDoc('docs/roadmap.md')}
          icon="clock"
          title="Roadmap"
          text="What comes next, and the honest risks"
        />
      </div>
    </DocPage>
  )
}
