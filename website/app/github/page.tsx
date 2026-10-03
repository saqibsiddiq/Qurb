import { redirect } from 'next/navigation'
import { repoUrl } from '../site-config'

/** /github is a short way to the code. */
export default function GitHubPage() {
  redirect(repoUrl)
}
