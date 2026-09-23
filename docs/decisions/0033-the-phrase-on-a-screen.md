# 0033 — The recovery phrase on a screen

**Status:** Accepted
**Date:** 2026-09-23

## Decision

The window can create a device, which means it has to put the 24 words on a
screen. Four rules govern how:

1. **The phrase is held in Rust, not in the page.** Created, kept in the
   session, fetched once to be drawn, and dropped the moment it has served.
2. **Confirmation is checked against the held copy.** The page sends back three
   words and a yes or no comes out; it never needs to keep the phrase to
   compare against.
3. **It is never written down by us.** Not to a log, not to a file, not to
   `localStorage`, not into any debugging output.
4. **It can be shown again**, derived from the stored key.

## Why the session holds it

The phrase has to cross into the window — there is no way to show somebody
their key without showing it to them. What can be avoided is the page *keeping*
it.

If the page held the phrase to check the confirmation answers against, it would
have to keep all 24 words in a JavaScript variable for the length of the step,
in a document that also runs an interval timer and touches the DOM. Holding it
in Rust instead means the page can drop its copy the instant it has drawn the
list, and the check is a boolean crossing the boundary rather than a key.

The held copy is dropped the moment confirmation succeeds. `RecoveryPhrase` has
a `Drop` that wipes its words and a `Debug` that prints `<redacted>`, so
dropping it is a real erasure and no incidental `{:?}` can leak it.

## Why confirmation exists at all

Somebody who has not actually written the words down cannot answer, and finding
that out now — while the phrase is still on the screen — is the entire point.
Three words at random positions, re-randomised each time, so that pressing
"show me them again" and coming back is not a way to learn the answer to the
same question.

Case and surrounding whitespace are ignored. People retype from paper, where
neither was recorded, and refusing a correct answer for its capitalisation
would teach them the check is arbitrary.

Answering nothing fails. "All zero of the given answers matched" must not be a
way past the step.

## Why it can be shown again

The key is already in the folder. Anybody who can read it can read the files,
so showing them the words gives away nothing they did not have. Refusing would
protect nobody and would strand the honest case: somebody who set up a device,
lost the paper, and wants to write it out again before they lose the device
too.

What cannot happen is recovering it from anywhere else. There is nowhere else.

The settings screen hides the words again on a second press, so they are not
left on a screen somebody walks away from, and says plainly that this is still
the key.

## Why the window opens before there is a device

Setting a device up is the job of a screen, so it cannot be a precondition of
the screen existing. The earlier version opened the key on the way up and
refused to start without one — correct for a terminal, useless for the window
whose job is to create the key.

So the window opens in one of two situations and moves from the first to the
second exactly once: **unmade**, where only the setting-up commands answer, and
**running**, where there is a store to query and a daemon publishing what it is
doing. Every other command answers "this device is not set up yet" until then,
rather than being absent.

## One definition of a set-up device

`qurb_cli::setup` has `create`, `enrol` and `inspect`, and both the terminal
and the window call them. The terminal had its own copies; a device created by
one route and missing a file the other writes is the kind of difference that
only shows up much later, on the device that was set up the unusual way.

## What this does not do

- **No passphrase prompt in the window.** A passphrase-protected key is asked
  for on the terminal the application was launched from. The window cannot ask,
  because opening the key is what decides whether there is anything to show.
  Launched from a menu, it says so on the first screen instead of failing
  silently.
- **No folder picker.** A text field with `~` expansion and a live description
  of what is there. A native dialog is a platform API per platform, for a
  question people answer once.
- **No storage allowance during setup.** The default is no limit, and the
  Storage screen sets one. Asking somebody to budget disk before they have put
  a file in the folder is asking a question they cannot answer.
- **No pairing.** Still `qurb pair` and `qurb join`. The last onboarding screen
  says so rather than implying the device is alone.
