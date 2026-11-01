# 0079 — A phone keeps one identity, however often it is set up

**Status:** Accepted — built 2026-11-01; tested over real connections
(`crates/peer/tests/permanent_identity.rs`) and in the store's rules. Not
yet run on a phone: it needs the laptop and the phone on a release with it.
**Date:** 2026-11-01

## What was asked

After the owner's S23 had become three devices in a day — "SM-S911B", "skib
phone", and "SM-S911B" again, each reinstall a new identity, linked
afterwards by his word (decision 0077): "the phone must retain its single
identity each time, it must not make 3 separate identities on each install."

## Why it happened

A device's identity is a key pair and a device id, both random, made when
it is first set up and kept in the app's data. A reinstall or *Clear data*
loses both, and the next setup makes new ones. Decision 0077 recognises the
same phone afterwards; this keeps it from changing at all.

## Decision

1. **A permanent identity, made, not kept.** A phone's identity is derived
   (`Identity::derived`) from a seed made from the person's key
   (`Purpose::DeviceIdentity`) and what Android keeps for this install of
   the app across a reinstall and *Clear data* — `ANDROID_ID`
   (`Identity::permanent_seed`). The same phone set up with the same
   person's key makes the same identity; another phone or another person, a
   different one. An Ed25519 key, whose signatures are deterministic, and a
   certificate with every field fixed, so the certificate — and so the
   fingerprint every device pins — is the same byte for byte.
2. **The raw `ANDROID_ID` stays on the phone.** Other devices are told only
   the marker made from it one way (decision 0077); none of them can make
   the phone's identity, though each holds the person's key.
3. **Taken over by telling the computer** (`Request::Become`, a new request
   an older build ignores). A phone joining by code must present *some*
   identity before the key reaches it — the key travels in that connection —
   so it joins under a passing one, then says which identity it is becoming:
   - a computer that does not know that identity turns the phone's
     fingerprint to it and keeps its device id: everything kept for it stays
     its. A phone already installed does this on its next sync, once;
   - a computer that knows it — the phone was set up again — answers with
     the device id it had, and counts the passing pairing as that device.
     The phone takes that device id, **only while nothing has been made in
     its store**, since every version counts against the id it was made
     under; otherwise it keeps its own, and the two are one by 0077's rule.
4. **The phone switches** when every computer of the person's seen in the
   last month has taken it, so none is left not knowing it; a computer not
   seen for a month is not waited for.

## Left undone

- **A factory reset** changes `ANDROID_ID`: that phone is a new identity,
  linked by name or the person's word (0077). So does a change of the app's
  signing key, which the S23 has already been through.
- **Computers** keep their identity files in the synced folder's `.qurb`,
  which a reinstall of the app does not touch; a folder deleted and set up
  again is a new identity. Not changed.
- **A guest's device** saying it is becoming another is refused in the code;
  no test drives that refusal yet.
- What a phone kept under an identity before taking a new device id is not
  moved over.
