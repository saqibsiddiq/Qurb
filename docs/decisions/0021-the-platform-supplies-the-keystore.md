# 0021 — The platform supplies the keystore

**Status:** Accepted
**Date:** 2026-09-17

## Decision

On mobile, the master key is kept by the app, not by this codebase.
`qurb-keys` defines a trait:

```rust
pub trait SecretStore: Send + Sync {
    fn put(&self, label: &str, secret: &[u8]) -> Result<()>;
    fn get(&self, label: &str) -> Result<Option<Vec<u8>>>;
    fn remove(&self, label: &str) -> Result<()>;
}
```

and a `Protection::Platform` that uses it. `qurb-mobile` exposes the same shape
as a UniFFI callback interface, which the app implements in Kotlin or Swift.

What is stored is the 32-byte master key itself, not something wrapping it.

## Why the platform has to do it

The desktop reaches Keychain, DPAPI and the Secret Service through one crate.
Neither mobile keystore works that way:

- **Android** — the Keystore is a Java API. Reaching it from Rust means JNI and
  a `Context`, which exists only inside an app.
- **iOS** — the Keychain needs entitlements, which belong to an app bundle and
  are checked against its code signature.

Both are a few lines on their own side. Neither is a few lines from here, and a
JNI binding living in this crate would make it depend on an Android application
class it can never construct in a test.

## Why two traits instead of one

`qurb-keys` could have depended on UniFFI and exported the callback interface
directly, saving an adapter.

It should not. `qurb-keys` is used by the daemon, the CLI and every test in the
workspace, none of which have any business knowing that a phone exists. A crate
that everything depends on should not acquire a dependency for the benefit of
one caller.

The adapter is about twenty lines and lives in `qurb-mobile`, where mobile
concerns belong.

## Why the key itself, not a wrapping key

The alternative: the platform keeps a random 32-byte secret, the vault file
holds the master key wrapped under it, and nothing sensitive is handed across
the boundary except at setup.

Rejected because it buys nothing and costs a second. The existing wrapping path
uses Argon2id, which is correct for a passphrase — low entropy, needs to be
expensive to attack — and pointless for a 256-bit random value, which cannot be
guessed regardless. Running a deliberately slow key-derivation function at every
app launch, on a phone, to protect something already unguessable, is a cost with
no matching benefit.

Both platforms are designed for secrets of this size. Android's Keystore and
iOS's Keychain both hold small opaque blobs as their normal case.

## What it is worth, and what it is not

On Android, a key in the Keystore is held by hardware the app cannot read
directly, and on a device with a secure element it need never exist in the
app's memory in exportable form. On iOS, an item marked
`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` is unreadable while the phone is
locked and does not travel to a backup or another device.

**Neither helps while the app is running and holding the key.** That is what it
means to be a program that can decrypt your files. The keystore protects a phone
that is off, locked, or being read by something that is not this app.

The iCloud-syncing Keychain is specifically wrong here and the reference
implementation says so: a master key that synchronises to another device through
Apple defeats the point of a recovery phrase the user holds.

## Failure has to be explicit

A vault records how it is kept. Opening a platform-protected vault without a
keystore fails with a message naming the missing keystore.

It must not fall back to reading the file, because there is no key in the file —
the fallback would produce a confusing error at a later, less obvious point. And
it must not be mistaken for corruption or a wrong passphrase, because the fix is
entirely different: pass the keystore.

## Consequences

- `Protection::Platform` is format byte 4 in the vault. Adding it broke
  `a_newer_format_is_refused_rather_than_misread`, which had written
  `FORMAT_PASSPHRASE + 1` as "a format from the future" — that being the number
  the new format took. The test now anchors to the last format, so it keeps
  testing what it names.
- A device set up with a keystore and later opened without one is locked out
  until the app passes the keystore again. The recovery phrase is the way back
  if the keystore itself is lost, which is one more reason the phrase matters.
- Nothing implements the contract. It is tested against a fake, which checks
  this side of the boundary and says nothing about whether Keychain and the
  Android Keystore behave as documented. That needs a device and an app.
