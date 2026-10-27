import { DocsShell } from '../../components/docs-shell'
import { SiteFooter } from '../../components/qs-chrome'
import { SECTIONS } from './content'
import '../home.css'
import './docs.css'

// The docs, laid out as documentation sites are: their own top bar with a
// search, a sidebar of every page, the page, and what is on it.
export default function DocsLayout({ children }: { children: React.ReactNode }) {
  const sections = SECTIONS.map((s) => ({ name: s.name, docs: s.docs.map(({ slug, title, summary }) => ({ slug, title, summary })) }))
  return (
    <main className="qs qs-docs">
      <DocsShell sections={sections}>{children}</DocsShell>
      <SiteFooter />
    </main>
  )
}
