# website/

The website for Qurb, at [www.qurb.cloud](https://www.qurb.cloud). It shares a
repository with the engine and nothing else: nothing in `crates/` or
`android/` depends on it, and the Rust workspace builds none of it.

Next.js 16 with the App Router, React 19 and TypeScript; Tailwind CSS 4 for its
reset and a few layout utilities; linted with oxlint. Every page is static.

```bash
npm install
npm run dev      # http://localhost:3000, listening on every interface
npm run build
npm run lint
```

## Deploying

**Every push to `main` deploys it.** The site is the Vercel project `qurb`
(team `saqubs-projects`), connected to this repository, with its Root
Directory set to `website` and Node 24. There is nothing to run by hand:
push, and Vercel builds `next build` here and serves it. Its state is on the
commit in GitHub, and `npx vercel inspect <deployment> --logs` shows a build.

`qurb.cloud` redirects to `www.qurb.cloud`, so `siteUrl` in
`app/site-config.ts`, `robots.txt` and `sitemap.xml` all name the `www`
address; `qurb-theta.vercel.app` serves the same deployment.

**From 20 July to 3 October 2026 nothing deployed.** The site moved from the
repository's root into `website/` and the project went on building from the
root, which no longer held a Next.js app: every build since failed, and
qurb.cloud served the July landing page all that time. Setting the Root
Directory fixed it. A failed build fails quietly — a red mark on a commit —
so a change to the site is not finished until the live page shows it.

## It looks like the apps, on purpose

Redesigned on 2026-10-03 with the apps as the reference: the same design
direction ([../docs/design/direction.md](../docs/design/direction.md),
[decision 0048](../docs/decisions/0048-the-design-direction.md)), the same
parts, the same words.

- **The tokens, the three glass materials, the environment and the
  components** in `app/globals.css` are the desktop window's
  (`crates/desktop/ui/app.css`), copied rather than imported, since the window
  has no build step to share them through. A change to a token there is a
  change here. What the site adds is only what a page needs and a window does
  not: a header, sections, long-form text.
- **The icons are the window's.** `scripts/icons.mjs` reads the window's own
  sprite (`crates/desktop/ui/icons.js`, Lucide) and writes
  `components/icon-paths.ts`. To add one, add its name to the script's list and
  run `node scripts/icons.mjs`; if the window does not have it yet, add it to
  `scripts/desktop-icons.py` first.
- **Inter is served from this site** (`public/fonts/`, the window's files and
  licence), so no page asks another site for anything — which is also what
  lets the privacy page say so.
- **The mark** is `components/mark.tsx`, and `app/icon.svg` is
  `packaging/qurb.svg`.
- **The product on the landing page is drawn, not screenshotted**, in each
  scene's component (below): the apps' screens simplified, with their own
  words and made-up files. A screenshot would go stale with the first change
  to an app; these change only when the design does.
- `app/opengraph-image.png`, the image a shared link shows, was rendered once
  in Chromium from the same font and mark, in October. It still says *Qurb*
  and the October headline: redraw it for QurbSpace.

## The landing page

Redesigned on 2026-10-27 with the owner, scene by scene, at `/preview`
until it became `app/page.tsx` the same day:
[../docs/design/website-brief.md](../docs/design/website-brief.md) is what it
is for and what each scene shows. The scenes are `components/always-near.tsx`,
`walkthrough.tsx`, `send-plane.tsx`, `backups.tsx`, `vault.tsx`,
`family.tsx`, `no-middle.tsx` and `get-it.tsx`, in that order on the page; `components/timeline.ts` is the small engine the scripted ones
share (keyframes in one number, p, and smooth flights), driven
by the clock when each comes into view (`play-on-view.ts`). Its styles
are `app/home.css`, prefixed so they touch nothing else.

- **Richer**: `components/near-field.tsx` (the living dots and the closing
  wordmark they settle into), `depth.tsx` (arrivals, parallax, the lean to
  the pointer, and the smooth wheel by Lenis) and `rise.tsx` (headlines word by word);
  `?settle` shows the ending alone in development.
- **On a phone** the scenes are drawn tall, laptop over phone: `Tall` in
  `components/play-on-view.ts`, the layouts in `components/tall.ts`, and
  the positions under *On a phone* in `app/home.css`.
- **The docs** are `app/docs/`: one page a topic, how-to and technical,
  written in `content.tsx`, with no animations, in the same design (`docs.css`, with `home.css`), the
  top bar and footer shared from `components/qs-chrome.tsx`.
- **The video** in the remote-access scene is `public/media/waves.*`:
  *Ocean surface waves 06* by Mostafameraji, CC0, from Wikimedia Commons,
  made small and silent with ffmpeg.
- **The waitlist** posts to `app/api/waitlist/route.ts`, which adds the
  address to a set in Upstash Redis
  ([decision 0075](../docs/decisions/0075-the-website-keeps-a-waitlist.md)).
  The store is `upstash-kv-charcoal-canvas`, connected to the project for
  every environment; `npx vercel env pull .env.local` (from this folder,
  which is linked) gives a local copy of its address and token.
- **The downloads** come from GitHub Releases by `release` in
  `app/site-config.ts`; set `published: true` once 0.1.0 is there.
- **In development**, `?connect=0.4`, `?remote=…`, `?change=…`, `?sp=…`,
  `?bk=…`, `?vt=…`, `?fm=…`, `?nm=…` hold a scene at
  that moment, and `?gi=android` (linux, windows, mac, ios) shows the last
  scene as that device would; production ignores them.

## Every claim is the product's

The words make no claim [../docs/features.md](../docs/features.md) does not,
at the level it is checked there — and the pages state what is missing as
plainly as what works: no release yet, Linux and Android only, no relay anyone
can use. Change the product, change the page. In particular:

- **The name is Qurb**, as the apps call it. The site used to say *Qurb Cloud*;
  the address is still qurb.cloud.
- **The legal pages are not policies.** There is no company and no service to
  write one for, so Privacy, Terms and Cookies say what is true of the software
  and this site today, and say they are not yet written.
- **Reporting a vulnerability** asks for an issue with no details, because the
  repository does not accept private reports through GitHub yet. Switching on
  GitHub's private vulnerability reporting would give that page a real channel.
- **Status** says there is nothing to report, because Qurb runs no service.

## What is here

```
app/page.tsx              the landing page
app/layout.tsx            the shell every page shares, and the environment
app/site-config.ts        the address, the repository, and the metadata
app/globals.css           the design system, from the window's, and the pages
app/download/             how to get Qurb today: from source, Linux and Android
app/page.tsx, app/home.css the landing page, and its styles
app/docs/                 the docs: how-to and technical pages, with search
app/api/waitlist/         the waitlist, kept in Upstash Redis
app/github/               a redirect to the repository
app/<others>/             security, reporting a vulnerability, cookies,
                          terms, licences, status, notes, contact, in the
                          October design; privacy sends to the docs
components/               the scenes and their engine, the top bar and
                          footer, the docs' parts; the October header,
                          footer and page shell; the mark, and the icons
public/                   the fonts, the video, robots.txt, sitemap.xml
scripts/icons.mjs         the icons, from the window's sprite
```

`.oxlintrc.json` allows the exports Next.js expects of a page (`metadata`,
`viewport`, …) in `react/only-export-components`; without that, every page
warned.

## Not done

- **Light only.** The apps have had a dark mode since 2026-10-08
  ([decision 0056](../docs/decisions/0056-dark-mode.md)); the website has
  not followed.
- **Checked in Chromium only**, at 1440 and 390 pixels wide, from screenshots
  of the production build on 2026-10-03. Not looked at in Firefox or Safari,
  or by the owner.
- **A blog.** Notes points at the repository's phase and decision records
  instead.
