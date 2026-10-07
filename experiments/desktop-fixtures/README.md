# Desktop fixtures

Throwaway. The desktop window's places, rendered against made-up data so that
layout and behaviour can be looked at without a daemon, a paired device or a
folder full of files.

## What it fakes

Exactly one thing: `window.__TAURI__.core.invoke`. Every command the window
calls is answered from [`fixtures.js`](fixtures.js) instead of from the engine.

Everything else is the real thing. The page fetches
`crates/desktop/ui/index.html` at load and uses its markup, then loads that
directory's `app.css` and every script `index.html` names, in its order, so a
change to any of them is visible here on the next reload and there is no copy
to keep in step.

An earlier version pasted the markup in instead, and it went stale within the
hour: a control gained a new range in the real window and the fixture kept the
old one, which showed up as a bug that did not exist.

## What it is not

Not a test of the Rust commands. Those are covered by `qurb-cli`'s `view`
tests and by running the application. This checks what the window *does with*
an answer, not whether the answer is right.

It cannot see a command that fails in the application itself, because it
answers them all; `scripts/desktop-smoke.sh` drives the real window for that.
Set `window.__fixtureQr` to an SVG from the Rust renderer before choosing
*Show a code* to see a real QR code here.

## Corners cut

- The fixture data is hand-written and does not have to be self-consistent.
- No error paths beyond the one deliberate failure in `find` (search for
  `boom`), a refused *Free local space* on an only copy, and a wrong
  passphrase on the unlock screen (the right one is `open`).
- Served over plain HTTP with no CSP, where the real window runs under a strict
  one. A resource the real window would refuse to load would work here.

## Running it

From the repository root, because the paths into `crates/` are relative:

```bash
python3 -m http.server 8731
```

Then open `http://localhost:8731/experiments/desktop-fixtures/`.

`?state=` picks what Home shows: `synced` (the default, and quiet — no
conflicts, nothing moving), `syncing`, `away`, `alone` (nothing paired) or
`attention`. `?screen=files` (or `devices`, `storage`, `vault`, `settings`,
`activity`, `deleted`) opens a place directly. `?locked` shows the unlock
screen.

Add `?setup` to see the setting-up screens instead of the running window:
`http://localhost:8731/experiments/desktop-fixtures/?setup`.

Add `&at=storage` as well to be taken straight to the storage question, and
`&custom=75` to have a custom amount typed into it; `&at=phrase` goes on to
the 24 words. The fixture disk has about
188 GiB free, so the two largest presets show as too big.

Pairing answers "waiting" for four seconds, then a phone asks to join showing
232 760 (decision 0053): *Approve* pairs it, *Decline* goes back to waiting.
So the countdown, the question and the arrival can all be looked at without a
second device. It returns no QR,
which is also worth seeing: the window has to cope with a code it could not
draw, because that code still works typed and read aloud.

The phrase shown there is a fixed list of words that is **not** a valid
recovery phrase and is not a key to anything. Confirmation accepts any answer,
because the real check is against the phrase the session is holding and a
fixture has no way to be that.
