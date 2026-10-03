import type { Metadata } from 'next'
import { DocPage } from '@/components/doc-page'
import { repoDoc } from '../site-config'

export const metadata: Metadata = {
  title: 'Licences',
  description: "Qurb’s licence, and the licences of what it and this site are built with."
}

const thirdParty = [
  ['Inter', 'the typeface, in the apps and here', 'SIL Open Font License 1.1', '/fonts/LICENSE-Inter.txt'],
  ['Lucide', 'the icons, in the apps and here', 'ISC License', 'https://lucide.dev/license'],
  ['Next.js and React', 'this site', 'MIT License', null],
  ['Tailwind CSS', 'this site', 'MIT License', null]
] as const

export default function LicencesPage() {
  return (
    <DocPage title="Licences" intro="Qurb’s own licence, and the licences of what it and this site are built with.">
      <h2>Qurb</h2>
      <p>
        Qurb’s Rust code declares its licence as <strong>MIT OR Apache-2.0</strong>, in its{' '}
        <a href={repoDoc('Cargo.toml')}>workspace manifest</a>. The licence texts themselves are not in the repository
        yet.
      </p>

      <h2>What it’s built with</h2>
      <ul>
        {thirdParty.map(([name, where, licence, link]) => (
          <li key={name}>
            <strong>{name}</strong> — {where}: {link ? <a href={link}>{licence}</a> : licence}
          </li>
        ))}
      </ul>
      <p>
        The engine depends on many Rust libraries, each under its own licence; a complete list for the apps is not
        published yet. <code>cargo tree</code> in the repository shows them all.
      </p>
    </DocPage>
  )
}
