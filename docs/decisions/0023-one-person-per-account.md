# 0023 — One person per operating-system account

**Status:** Accepted
**Date:** 2026-09-22

## Decision

Several people share a computer by having separate operating-system accounts.
qurb **inherits** that separation rather than reimplementing it: there is no
notion of a qurb user, no login, no account switcher.

Inside one account, a person may have several folders — work and personal, say —
each with its own identity and its own paired devices. `qurb init <dir>` makes
one; a small registry records which exist so later commands need no path.

## Why not qurb accounts

The alternative is a user list inside qurb: profiles, a chooser at startup,
per-profile keys, and a way to stop one profile reading another's files.

Every part of that already exists in the operating system, done better:

- **`$HOME` separation.** Each account has its own folders and its own store.
- **File permissions.** The store is owner-only; the kernel enforces it.
- **The keystore.** Keychain, DPAPI and the Secret Service are already scoped
  per account, and on Android per app *per user*. A qurb-level profile would
  have to invent its own protection or share one keystore entry between people,
  which is worse than either.
- **Session lifecycle.** The daemon starts when a person logs in and stops when
  they log out, because that is what a per-user service does.

Reimplementing this would mean writing an access-control system, getting it
right, and being trusted about it — to arrive where a second `useradd` already
is. The project's value rests on a small amount of security-critical code that
can be reasoned about; a home-grown multi-user layer is the opposite of that.

## What this means in practice

**Two people, one computer, two logins.** Nothing shared: different `$HOME`,
different identities, different phones paired to each. Neither can read the
other's files, and neither needs to know the other uses qurb. Two daemons may
run at once under fast user switching, which is fine — they bind ephemeral
ports and announce under different rendezvous identifiers.

**Two people, one login.** Not supported, and deliberately. If two people share
an account they already share everything on it; qurb pretending otherwise would
be security theatre — the other person can read the store directly.

**One person, several folders.** Supported. Each is a separate store with its
own identity, so a work folder and a personal one pair with different devices
and never mix.

## The default location

A new folder goes in **Downloads** — `~/Downloads/qurb`, or wherever
`XDG_DOWNLOAD_DIR` points on a system where that folder is not called
"Downloads".

Not a hidden directory, and not the home root. Files that sync between devices
are files someone wants to *find*, and Downloads is where every desktop already
looks and every file picker already opens. A hidden folder would be technically
tidier and practically useless.

## Consequences

- `qurb init` with no path creates `~/Downloads/qurb`. Existing installs are
  unaffected: the old `~/qurb` is still resolved, so nobody's folder moves.
- Commands take the folder they were last given, so the common case is
  `qurb status` rather than retyping a path. The registry lives under
  `$XDG_CONFIG_HOME/qurb/folders` — it lists paths and holds no secrets.
- A folder on a removable disk stays listed while it is unplugged, rather than
  being forgotten the first time someone opens the list.
- **Nothing stops a determined person reading another account's store if they
  have root.** That is true of every file on the machine and is not a property
  qurb can or should try to provide.
