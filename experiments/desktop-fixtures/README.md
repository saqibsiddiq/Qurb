# Desktop fixtures

Throwaway. The desktop window's five screens, rendered against made-up data so
that layout and behaviour can be looked at without a daemon, a paired device or
a folder full of files.

## What it fakes

Exactly one thing: `window.__TAURI__.core.invoke`. Every command the window
calls is answered from [`fixtures.js`](fixtures.js) instead of from the engine.

Everything else is the real thing. The page fetches
`crates/desktop/ui/index.html` at load and uses its markup, then loads that
directory's `app.css` and `app.js`, so a change to any of the three is visible
here on the next reload and there is no copy to keep in step.

An earlier version pasted the markup in instead, and it went stale within the
hour: a control gained a new range in the real window and the fixture kept the
old one, which showed up as a bug that did not exist.

## What it is not

Not a test of the Rust commands. Those are covered by `qurb-cli`'s `view`
tests and by running the application. This checks what the window *does with*
an answer, not whether the answer is right.

## Corners cut

- The fixture data is hand-written and does not have to be self-consistent.
- No error paths beyond the one deliberate failure in `search`.
- Served over plain HTTP with no CSP, where the real window runs under a strict
  one. A resource the real window would refuse to load would work here.

## Running it

From the repository root, because the paths into `crates/` are relative:

```bash
python3 -m http.server 8731
```

Then open `http://localhost:8731/experiments/desktop-fixtures/`.

Add `?setup` to see the setting-up screens instead of the running window:
`http://localhost:8731/experiments/desktop-fixtures/?setup`.

Pairing answers "waiting" for six seconds and then "paired", so the countdown
and the arrival can both be looked at without a second device. It returns no QR,
which is also worth seeing: the window has to cope with a code it could not
draw, because that code still works typed and read aloud.

The phrase shown there is a fixed list of words that is **not** a valid
recovery phrase and is not a key to anything. Confirmation accepts any answer,
because the real check is against the phrase the session is holding and a
fixture has no way to be that.
