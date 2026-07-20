import type { Metadata, Viewport } from 'next'

export const siteUrl = 'https://qurb.cloud'

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: 'Qurb Cloud',
    template: '%s | Qurb Cloud'
  },
  description:
    'Qurb Cloud turns your desktop into your own private cloud server so your files stay with you, your devices stay in sync, and your storage ownership stays personal.',
  applicationName: 'Qurb Cloud',
  keywords: [
    'private cloud',
    'cloud storage',
    'self-hosted cloud',
    'desktop cloud server',
    'privacy-first storage'
  ],
  openGraph: {
    title: 'Qurb Cloud',
    description: 'Your cloud. Your computer. Your data.',
    url: siteUrl,
    siteName: 'Qurb Cloud',
    type: 'website',
    images: [
      {
        url: '/icon.svg',
        width: 1200,
        height: 630,
        alt: 'Qurb Cloud logo'
      }
    ]
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Qurb Cloud',
    description: 'Your cloud. Your computer. Your data.',
    images: ['/icon.svg']
  },
  alternates: {
    canonical: siteUrl
  },
  robots: {
    index: true,
    follow: true
  },
  icons: {
    icon: '/icon.svg'
  }
}

export const viewport: Viewport = {
  themeColor: '#f4efe7',
  width: 'device-width',
  initialScale: 1
}
