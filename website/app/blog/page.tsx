import type { Metadata } from 'next'
import { DocPage, LinkRow } from '@/components/doc-page'
import { repoUrl } from '../site-config'

export const metadata: Metadata = {
  title: 'Notes',
  description: 'Nothing published here yet; what changes is written down in the repository as it happens.'
}

export default function NotesPage() {
  return (
    <DocPage
      title="Notes"
      intro="Nothing is published here yet. What changes in Qurb, and why, is written down in the repository as it happens."
    >
      <div className="rows">
        <LinkRow
          href={`${repoUrl}/tree/main/docs/phases`}
          icon="history"
          title="What each phase built"
          text="And what it measured, under what conditions, and what it left undone"
        />
        <LinkRow
          href={`${repoUrl}/tree/main/docs/decisions`}
          icon="git-compare"
          title="Decisions"
          text="Each one with its reasons, and the ones later reversed, marked so"
        />
        <LinkRow href={`${repoUrl}/commits/main`} icon="clock" title="Every change" text="The repository’s history, newest first" />
      </div>
    </DocPage>
  )
}
