import type { Metadata, Viewport } from 'next'

/** Where the site is served. Vercel redirects qurb.cloud here, so this is the
 *  canonical address, not the bare domain. */
export const siteUrl = 'https://www.qurb.cloud'

/** Where the code is. Public, so every page may link into it. */
export const repoUrl = 'https://github.com/saqibsiddiq/Qurb'

/** The release the download buttons offer, from GitHub Releases. Until it is
 *  published there, `published` stays false and the buttons say *Coming soon*
 *  and point to the waitlist: a link to a file that is not there yet would
 *  fail for the person who followed it. File names are docs/releasing.md's. */
export const release = { version: '0.1.0', tag: 'v0.1.0', published: false }
export const releaseFile = (name: string) => `${repoUrl}/releases/download/${release.tag}/${name}`
export const releasesUrl = `${repoUrl}/releases`

/** A document in the repository, as GitHub shows it. */
export const repoDoc = (path: string) => `${repoUrl}/blob/main/${path}`

const description =
  'Qurb keeps your files on the devices you own and moves them directly between them, encrypted. No cloud drive in the middle holds a copy.'

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: 'Qurb — Your files. Your devices. Your space.',
    template: '%s · Qurb'
  },
  description,
  applicationName: 'Qurb',
  keywords: ['private file sync', 'peer-to-peer sync', 'end-to-end encrypted', 'Linux', 'Android', 'Dropbox alternative'],
  openGraph: {
    title: 'Qurb',
    description: 'Your files. Your devices. Your space.',
    url: siteUrl,
    siteName: 'Qurb',
    type: 'website'
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Qurb',
    description: 'Your files. Your devices. Your space.'
  },
  alternates: {
    canonical: siteUrl
  },
  robots: {
    index: true,
    follow: true
  }
}

export const viewport: Viewport = {
  themeColor: '#f7f7f4',
  width: 'device-width',
  initialScale: 1
}
