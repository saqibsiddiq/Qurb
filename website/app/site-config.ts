import type { Metadata, Viewport } from 'next'

export const siteUrl = 'https://qurb.cloud'

/** Where the code is. Public, so every page may link into it. */
export const repoUrl = 'https://github.com/saqibsiddiq/Qurb'

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
