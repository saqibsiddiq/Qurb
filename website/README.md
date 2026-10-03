# website/

The website for Qurb, at qurb.cloud. It shares a repository with the engine
and nothing else: nothing in `crates/` or `android/` depends on it, and the
Rust workspace builds none of it.

Next.js 16 with the App Router, React 19 and TypeScript; Tailwind CSS 4 for its
reset and a few layout utilities; linted with oxlint. Every page is static.

```bash
npm install
npm run dev      # http://localhost:3000, listening on every interface
npm run build
npm run lint
```

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
- **The product on the landing page is drawn, not screenshotted**
  (`components/previews.tsx`): the window's sidebar, stage, rows and status
  mark, and the phone's Home under its tab bar, with the apps' own words and
  made-up files. A screenshot would go stale with the first change to an app;
  these change only when the design does, and in one place.
- `app/opengraph-image.png`, the image a shared link shows, was rendered once
  in Chromium from the same font and mark. Redraw it if the headline changes.

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
app/docs/                 the way into the documentation in the repository
app/github/               a redirect to the repository
app/<others>/             security, reporting a vulnerability, privacy,
                          cookies, terms, licences, status, notes, contact
components/               the header, the footer, the page shell, the
                          previews, the mark, and the icons
public/                   the fonts, robots.txt, sitemap.xml
scripts/icons.mjs         the icons, from the window's sprite
```

`.oxlintrc.json` allows the exports Next.js expects of a page (`metadata`,
`viewport`, …) in `react/only-export-components`; without that, every page
warned.

## Not done

- **Light only**, like the apps, until their dark mode is designed.
- **Not deployed from here.** Nothing in the repository says where qurb.cloud
  is hosted; the privacy page says only that a host may keep ordinary request
  logs.
- **Checked in Chromium only**, at 1440 and 390 pixels wide, from screenshots
  of the production build on 2026-10-03. Not looked at in Firefox or Safari,
  or by the owner.
- **A blog.** Notes points at the repository's phase and decision records
  instead.
