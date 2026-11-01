# 0081 — The Vault switch apart from the backups

**Status:** Accepted, 2026-11-02. Built the same day; tested in the engine
(`crates/storage/tests/outside.rs`), not yet on a phone.
**Date:** 2026-11-02

## What was asked

The owner's review of 2026-11-02, D-3: the *Private Vault* switch on a
computer's page is for the Vault folder in Files only; switching it off must
not stop WhatsApp or the folders being backed up to that computer.

## What it was

Since decision 0062 the folders and WhatsApp are backed up *into* the
phone's vault (under `.qurb-outside/`), and a computer keeps the vault when
it is one of the phone's holders. One switch, adding or removing the holder,
so turning the Vault off stopped every backup to that computer, and the page
did not say so.

## Decided

- A holder is a computer that keeps this phone's **backups**. Whether it
  keeps the **Private Vault's own files** too is a flag of its own on the
  holder (`holders.vault`, migration V29, on by default so nothing that is
  kept today changes).
- A holder with the flag off is shown only the paths under `.qurb-outside/`
  (`Db::tree_for`). What it already has of the Vault it keeps: a holder does
  nothing about a file missing from its list, as before.
- On the phone, *Folders* and *WhatsApp* are always offered on one's own
  computer's page; choosing either makes the computer a holder of the
  backups only, if it was not one. *Private Vault* sets the flag
  (`set_keeps_vault_files`).
- Another person's computer, which keeps the vault sealed (decision 0060),
  is unchanged: its switch still adds or removes it as a holder.

## Gaps

- Not yet tried on the S23 against the laptop.
- The sealed listing for another person's computer does not look at the
  flag; it has no UI to turn it off, so nothing reaches that path today.
