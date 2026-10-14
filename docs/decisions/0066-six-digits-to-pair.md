# 0066 — Six digits to pair, on the same network

**Status:** Accepted, 2026-10-14. Amends [0014](0014-pairing.md), whose code
carried the inviting device's whole fingerprint because "a short code would
need a password-authenticated key exchange to be safe". This is that
exchange. The approval of [0053](0053-approval-same-key-and-safe-copies.md) is
unchanged.
**Date:** 2026-10-14

## What was asked

The owner's item 7 of 2026-10-14: "there is either a code or QR to connect
with devices, however the code is too long and inconvenient, we can instead
use a 6 digit code to connect just like in whatsapp". Asked which way round,
he chose: the laptop shows it, the phone types it.

Today's typed code is `qurb1-` and about a hundred characters, because it
carries 32 bytes of fingerprint, a 16-byte token, an expiry and an address.
Six digits carry 20 bits. They cannot carry any of that, and whoever knows
them must not be able to stand in for the laptop.

## Decision

**Six digits are a password for fetching the long code, not a replacement for
it.** The device showing a code also shows six digits, valid as long as the
code (five minutes). The device typing them:

1. **finds the device offering them on the local network.** While its code
   is shown, the offering device multicasts an *offer* every second, on the
   group beacons use and its own port (47318): eight bytes of magic and the
   port its pairing listener is on. A device typing digits asks once for
   offers and listens for up to ten seconds.
2. **connects to each offer's pairing listener** without pinning a
   certificate (it has none to pin yet) and runs **SPAKE2** (Ed25519 group,
   RustCrypto's `spake2`) with the six digits as the password. Both
   certificates' fingerprints, as each side saw them in the TLS handshake,
   are the exchange's identities, so a machine in the middle, which must
   present its own certificate to each side, makes the two keys differ.
3. **confirms the key both ways.** The offering device sends a confirmation
   with its half; the typing device checks it, then sends its own; only then
   is the long code sent, sealed under the exchanged key
   (XChaCha20-Poly1305).
4. **pairs with the long code exactly as if it had been scanned**: pinned to
   the fingerprint the exchange proved, presenting the token, shown the
   number to compare, approved at the device showing the code.

**Five wrong tries and the digits stop working** for that code: every
exchange that gets as far as the offering device's half counts, whether or
not the other side goes on to confirm, because seeing that half lets the
other side test one guess. The screen says so and the QR code still works.
The number of wrong tries is not reset by a right one.

**What a typed long code and a QR code do is unchanged.** Every place a code
is typed takes either; the long one stays for the command line and for
devices on different networks, which it can reach where it can route today.

## Why it is safe

- **One guess per try, five tries per code.** That is SPAKE2's property: a
  party without the password that takes part in an exchange learns whether
  one guessed password was right, and nothing that lets it test another
  offline. With five tries an attacker on the same network finds the digits
  with probability 5 in a million per code shown.
- **And then the person approves.** A device that guessed right still asks,
  and the device showing the code shows its name and the number it should be
  showing (0053). The number is derived from its certificate, so it differs
  from the number on the phone in the person's hand.
- **A machine in the middle learns nothing it can use.** The identities bound
  into the exchange are the certificates each side actually saw. A relay that
  terminates TLS presents its own to each, the two sides derive different
  keys, and neither confirmation verifies. Taking part in each half is one
  guess at each half.
- **The digits are never sent**, so overhearing the exchange does not give
  them away, and neither does the offer, which carries no secret.

## Costs, accepted

- **Same network only.** The offer is a multicast on the local network; a
  phone on mobile data cannot see a laptop at home. The long code can reach
  only as far as its one address routes anyway (0014), so the reach lost is
  small. Over the internet the rendezvous service could carry offers, but
  only by a meeting point the digits do not give away, as in Magic
  Wormhole's numbered nameplates, which would take digits from the password.
  Not built.
- **A network that blocks multicast** blocks this. Some routers isolate
  wireless clients from each other. Then the QR code.
- **A stranger on the network sees that something is offering a code** for
  the minutes it is shown: an offer is not encrypted, unlike beacons (no key
  is shared yet). It says nothing about who, and carries no secret.
- **Five tries can be spent by someone else** on the same network, after
  which the person uses the QR code or starts again.

## Rejected

- **Six digits as the token, the address found some other way.** Anyone who
  learned them could connect, and a device that connected to the wrong
  listener would be told nothing about the real one. The fingerprint has to
  come from somewhere authenticated.
- **Deriving a rendezvous room from the digits.** A million rooms are tried
  in a moment: the service, or anyone who could see its rooms, would learn
  the digits and could stand in for either side.
- **Dropping the approval now that the digits authenticate.** Six digits
  shown on a screen can be overheard or seen over a shoulder as a long code
  can, and approval is what 0053 added for exactly that.

## Reversing it

Easy. The long code and the QR code are untouched; the offer and the exchange
are additions to the pairing listener, which a build without them never
sends. A device that does not know the requests answers *not found* and the
typing device says the digits were not taken.
