import type { Metadata } from 'next'
import Link from 'next/link'
import { DocPage } from '@/components/doc-page'
import { repoDoc } from '../site-config'

export const metadata: Metadata = {
  title: 'Security',
  description: 'How Qurb protects your files and keys, and the limits it states rather than hides.'
}

export default function SecurityPage() {
  return (
    <DocPage
      title="Security"
      intro="How Qurb protects your files and keys today — and, just as plainly, what it doesn’t protect against yet."
    >
      <h2>Your key</h2>
      <p>
        Each person has one root key: 256 random bits from the operating system, which <em>are</em> your 24-word
        recovery phrase. Every other key is derived from it, one for each purpose. All your devices hold it; nobody else
        ever does, so if you lose the words and every device, your files can’t be recovered by anyone.
      </p>
      <p>
        On a computer you choose how it’s kept: in a file only you can read, in the system keystore, or behind a
        passphrase. On Android it’s kept in the Android Keystore, where only Qurb on that phone can use it.
      </p>

      <h2>Your files</h2>
      <ul>
        <li>
          <strong>Between devices</strong>, everything is encrypted, over QUIC. Each device has its own certificate, and
          pairing carries its full fingerprint across the room — by a code you scan or type — so a device only ever
          talks to devices you added.
        </li>
        <li>
          <strong>On a device</strong>, the files in your Qurb folder are ordinary files, protected the way the rest of
          your disk is. What Qurb keeps in its own store — copies held for your other devices, the parts of files not
          in the folder — is compressed and encrypted with XChaCha20-Poly1305.
        </li>
        <li>
          <strong>Anything a peer sends is checked</strong> before it is written: content is named by its BLAKE3 hash and
          verified against it, and a path that tries to leave the folder or reach Qurb’s own store is refused.
        </li>
      </ul>

      <h2>The services</h2>
      <p>
        The service that introduces devices across networks learns their network addresses and that they want to meet,
        under identifiers it can’t link to a person — never a filename, a file or a key. A relay, where one is
        used, forwards encrypted traffic it cannot read. If you set up push, Google learns that a phone was woken, and
        when — nothing about why. You run both services yourself; Qurb runs neither.
      </p>

      <h2>What it doesn’t protect against yet</h2>
      <ul>
        <li>
          <strong>Your devices share one key.</strong> A file in one device’s Private Vault is private from devices
          that don’t hold its bytes, not cryptographically private from one that does.
        </li>
        <li>
          <strong>Removing a device doesn’t take its key away.</strong> It stops being trusted at once, and keeps
          what it already has. Changing the key is not built yet.
        </li>
        <li>
          <strong>Nobody outside the project has reviewed it.</strong> Everything above is tested, including against
          hostile peers and crashes, but it has not been audited.
        </li>
      </ul>
      <p>
        The reasoning behind each of these is written down:{' '}
        <a href={repoDoc('docs/decisions/0012-key-hierarchy-and-recovery.md')}>keys and recovery</a>,{' '}
        <a href={repoDoc('docs/decisions/0011-peer-identity-pinning.md')}>device identity</a>,{' '}
        <a href={repoDoc('docs/decisions/0016-what-signalling-learns.md')}>what the introduction service learns</a>. Found
        something? <Link href="/responsible-disclosure">Report it privately</Link>.
      </p>
    </DocPage>
  )
}
