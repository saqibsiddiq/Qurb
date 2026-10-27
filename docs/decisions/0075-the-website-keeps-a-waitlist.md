# 0075 — The website says QurbSpace, keeps a waitlist, and downloads from GitHub Releases

**Status:** Proposed — built on the branch `website-qurbspace` at `/preview`
(2026-10-27). The waitlist works: Upstash for Redis provisioned
(`upstash-kv-charcoal-canvas`) and connected to the project `qurb` for
production, preview and development the same day; an address posted locally
was stored, read back and removed, and a malformed one refused. The downloads
say *Coming soon* until 0.1.0 is published on GitHub. Becomes Accepted when the new landing page replaces
`app/page.tsx` on `main`.
**Date:** 2026-10-27

## What was asked

The owner, redesigning the website (docs/design/website-brief.md): the
product is called **QurbSpace** on the site as in the app; the page ends in
"join the waitlist, visit the github page and download button depending on
device OS"; asked where the waitlist should keep its addresses and where the
downloads should come from, "upstash and github releases".

## Decision

1. **QurbSpace** on the website, everywhere it names the product. The address
   stays qurb.cloud, and *Qurb* (قرب, nearness) is what the name means — said
   once, in the footer.
2. **A waitlist of email addresses**, posted to `/api/waitlist` and added to
   one Redis set, `waitlist`, in **Upstash Redis** through the Vercel
   Marketplace (its REST address and token in the project's environment).
   Only the address is kept: no name, no device, no IP address written
   anywhere by the site. Without the integration the route answers 503 and
   the page says the waitlist is not open yet — it never pretends to have
   taken an address.
3. **Downloads from GitHub Releases**, by the file names in
   [releasing.md](../releasing.md), offered for the visitor's own device:
   Android and Linux (a choice of .deb, .rpm and Arch package), Windows
   marked early because it has never been run there, Mac and iPhone sent to
   the waitlist. `release.published` in `website/app/site-config.ts` stays
   false until the release is on GitHub; until then every visitor is offered
   the waitlist, and no button links to a file that is not there.

## Why

- The site had stored nothing about anyone, and its privacy page said so. A
  waitlist changes that, so it keeps the least that does the job — one
  address, for one message — and says so: on the docs' *What this website
  keeps*, since the owner took the privacy page out of the new design.
- Upstash is a set and a REST call: no database schema, no server to run, and
  it comes through the Vercel project the site already deploys from.
- GitHub Releases is where the code already is, costs nothing, and keeps the
  packages beside their checksums (`SHA256SUMS`).

## What was rejected

- **A form service or a newsletter platform.** More than a list is needed for
  neither, and either would hold the addresses under someone else's terms.
- **A database such as Postgres.** A table for one column.
- **Hosting the packages on the website.** Vercel is not a download host, and
  a second copy of each package is a second place for it to be wrong.
- **Linking the buttons to the release before it exists.** A 404 for the
  first person to try.

## Tradeoff accepted

- The site now holds personal data — email addresses — and Upstash holds them
  for it. The privacy page has to change with it, and someone has to send the
  one message and then delete the list.
- No rate limit on `/api/waitlist` yet: a script could fill the set with
  junk addresses. Adding one (Upstash's own rate limiter, or Vercel's
  firewall) is the next step if it is abused.

## How hard to reverse

Easy. The route and the form are two files; the set can be exported and
deleted; the downloads are links. The name is words.
