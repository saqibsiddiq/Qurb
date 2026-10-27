import { redirect } from 'next/navigation'

// What the site keeps is said in the docs now (docs/design/website-brief.md,
// *The docs*); this address, linked from before, goes there.
export default function PrivacyPage() {
  redirect('/docs/this-website')
}
