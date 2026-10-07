# 0056 — Dark mode, on the same tokens

**Status:** Accepted — built on the desktop and Android; the desktop looked at
against its fixtures; the S23 see *Checked, and not*
**Date:** 2026-10-08

## What was asked

The design brief put dark mode after the light design: "light first; dark
after light is approved, as a Dark mode on the colour variables" (brief §1).
The direction gives only the light palette (§8). The owner asked for dark mode
next, on 2026-10-08.

## Decision

**One set of roles, two sets of values.** Both apps already name roles rather
than colours: the window's CSS custom properties, and Android's colour
resources. Dark gives each role a second value and changes nothing else.
Screens, components and layouts are the same in both.

**The dark values**, the same on both apps:

| role | light (§8) | dark |
|---|---|---|
| background | `#F7F7F4` | `#121311` |
| surface / secondary | `#FFFFFF` / `#F0F0EC` | `#1C1D1B` / `#242622` |
| text, 2, 3 | `#171816` `#5F625C` `#8A8D86` | `#ECEDE8` `#B0B3AB` `#868980` |
| border / strong | `#E2E3DD` / `#D2D4CD` | `#2E302C` / `#3C3F3A` |
| green / hover / pressed | `#2F6B57` `#285B4A` `#214B3D` | `#3F8C70` `#4A9A7D` `#357A61` |
| green soft | `#E8F0EC` | `#1C2A24` |
| healthy | `#287052` on `#E8F3EC` | `#74C29B` on `#19291F` |
| attention | `#946A24` on `#F7F0DD` | `#DCB064` on `#2D2617` |
| error | `#A3423A` on `#F7E9E7` | `#E5847A` on `#331F1D` |
| neutral | `#626A68` on `#ECEEEC` | `#A2AAA7` on `#232625` |

Warm near-blacks, never pure black, the direction's warm off-white turned
round. The green is lifted so it still reads as Qurb's on a dark surface, and
white words on it still read. A semantic colour becomes a light tint on a
dark ground, as its light form is a dark one on a light ground. Glass is a
slightly lighter dark at the same translucency, with a faint light edge.
Shadows are deeper, since a dark surface has little to cast them against.

**Chosen in Settings: System, Light or Dark**, applied at once. *System*
follows the platform and is the default. It is how this screen looks, so it
is kept on the device (the window's storage; Android's preferences) and
never synced. The window sets the theme in `theme.js`, loaded first from
`<head>`, so nothing draws in the wrong one. Android sets it in
`QurbApplication` before the first screen, through AppCompat's night mode.

**The window's inline colours became tokens.** About eighty translucent
whites, shadows and tints were written as `rgba()` in components, and a dark
palette would have missed them. They became channel tokens, chosen by what
each paints:

- `--paper`: glass fills;
- `--edge` with `--edge-k`: light edges;
- `--ink`: overlays and tracks;
- `--shade` with `--shade-k`: shadows;
- one each for the semantic tints.

Android's drawables likewise name colour roles now (`nav_glass`, `tile_fill`,
`env_glow` and the rest), each with a night value.

**Kept as they are:**

- the QR code's white ground, which a camera needs;
- the camera screen's black;
- white words on green.

## Why this, and not something else

- **Inverting the light palette by formula.** It makes the green muddy and
  the semantic tints garish. Each role was chosen instead, against the light
  one, for the same contrast in the other direction.
- **Material's dynamic colour.** It would replace Qurb's green with the
  wallpaper's, which the direction rules out: the green is the identity
  colour.
- **Syncing the choice.** One person may well want a dark phone and a light
  desktop.

## What it costs

- **The desktop's *System* depends on WebKitGTK** reporting the desktop's
  dark preference to `prefers-color-scheme`, which depends on the GTK theme
  and portal. Where it does not, *System* is light, and *Dark* works anyway.
- **Two values for each role**, so a colour added later needs both. The
  window's palette is one block of tokens and Android's one file each, so a
  missing dark value shows as a light patch rather than hiding.
- **Not the owner's review.** The brief has dark mode follow the owner's
  review of the light design, which has not happened. The dark values are
  this record's, and the review may change them.

## Checked, and not

- The window against its fixtures, in headless Chromium, in both themes:
  - Home, with something needing attention;
  - Files with a details panel;
  - Storage and Settings;
  - setup;
  - *Show a code* with a device asking.

  Light is unchanged by the token rewrite (side by side with a screenshot
  from before). The first dark render found the dialog's backdrop drawn
  light, a darkening overlay converted as a light one, which was fixed.
- The window's smoke test (`scripts/desktop-smoke.sh`), in the real
  application.
- **Not yet watched**: the S23 in dark (installed; the phone was locked when
  it went on); the real window following the desktop's dark preference.

## Reversing it

Delete the `:root[data-theme="dark"]` block and `theme.js`'s line in
`index.html`; on Android, `values-night/` and the parents back to
`Theme.Material3.Light`.
