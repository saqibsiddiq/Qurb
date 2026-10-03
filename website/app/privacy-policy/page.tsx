import type { Metadata } from 'next'
import Link from 'next/link'
import { DocPage, NotWritten } from '@/components/doc-page'

export const metadata: Metadata = {
  title: 'Privacy',
  description: 'What Qurb and this website learn about you, which today is very little.'
}

export default function PrivacyPage() {
  return (
    <DocPage title="Privacy" intro="What Qurb and this website learn about you — which, today, is very little.">
      <NotWritten>
        This is not yet a privacy policy: there is no company and no service to write one for. What follows is what is
        true of the software and this site now.
      </NotWritten>

      <h2>Qurb</h2>
      <ul>
        <li>There are no accounts. Nothing asks for your name, your email or a password.</li>
        <li>
          Your files stay on your devices and move directly between them. Qurb, the project, runs no server that sees
          them, and no server at all yet.
        </li>
        <li>
          The services your devices use are ones you run yourself. What they can learn is on the{' '}
          <Link href="/security-policy">security page</Link>.
        </li>
        <li>
          The apps send no analytics and no crash reports. If you set up push to wake your phone, the phone registers
          with Google’s Firebase Cloud Messaging for it, and Google learns when it is woken — nothing more.
        </li>
      </ul>

      <h2>This website</h2>
      <ul>
        <li>
          It sets no cookies and runs no analytics — see <Link href="/cookie-policy">cookies</Link>.
        </li>
        <li>It loads nothing from other sites: even its font is served from here.</li>
        <li>Whoever hosts it may keep the ordinary logs any web server keeps, such as addresses and pages asked for.</li>
      </ul>
    </DocPage>
  )
}
