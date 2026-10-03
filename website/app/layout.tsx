import { preload } from 'react-dom'
import './globals.css'
import { metadata, viewport } from './site-config'

export { metadata, viewport }

export default function RootLayout({
  children
}: Readonly<{
  children: React.ReactNode
}>) {
  // Inter is served from this site, as the window bundles it: no page here
  // asks another site for anything.
  preload('/fonts/inter-latin.woff2', { as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' })

  return (
    <html lang="en">
      <body>
        {/* The light the glass sits in (direction §7). */}
        <div className="environment" aria-hidden="true" />
        {children}
      </body>
    </html>
  )
}
