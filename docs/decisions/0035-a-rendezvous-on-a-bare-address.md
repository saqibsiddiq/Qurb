# 0035 — A rendezvous on a bare address

**Status:** Accepted
**Date:** 2026-09-24

## Decision

The rendezvous service can present a self-signed certificate, and a device
checks it against a fingerprint carried in the URL:

```
wss://203.0.113.5:9000#40478155b092acf37fd1af1e526936dddd19b2152580b15fdcc0230cd6762e41
```

A URL with no fragment is verified against the usual public roots, so a
deployment with a domain and a real certificate needs none of this. Both are
supported. What is not supported, and has not changed, is an unauthenticated
connection across a network you do not own.

## Why

The identifiers devices announce under are bearer secrets: anybody who sees one
can ask where that device is. So a connection to a rendezvous across somebody
else's network has to be encrypted — which means a certificate, which normally
means a domain name and a certificate authority.

Requiring somebody to buy a domain in order to run a rendezvous is a barrier
this product should not put up. The alternative that was being used instead was
an overlay network, which worked and added a dependency on a third party for
something two devices and a small host can do between themselves — and which
failed, silently, for three hours, because it had quietly dropped off.

Pinning is also simply how qurb checks identity everywhere else. Peers are
checked by pinned fingerprint ([decision 0011](0011-peer-identity-pinning.md))
rather than by a certificate authority, for the reason that applies here too: an
authority adds a third party to a decision two ends can make between themselves.

## Why the fingerprint is in the URL

One string to copy from the server that printed it into the setting on each
device. That is the whole deployment, and it fits in the one text field the
phone already has.

A fragment rather than a query parameter because a fragment is not sent to the
server. It is a fact *about* the server; putting it in the query string would
send the thing being checked to the thing being checked.

The cost is a shell footgun — `#` starts a comment — which the documentation
calls out, because the failure arrives later as a certificate error rather than
at the moment it was mistyped.

## What pinning checks, and what it deliberately does not

**Checked:** the certificate's SHA-256, and the handshake signature. The second
matters as much as the first: pinning the certificate alone would accept anybody
who could replay a copy of it, and a certificate is public. Verifying the
signature is what proves the other end holds the private key.

**Not checked:** the chain, the name, the expiry. Each of those exists to answer
"is this the server I meant", and the fingerprint answers it directly. A
self-signed certificate has no chain to build; its name is whatever it was
generated with; and an expiry would lock out every device on a date nobody
chose.

## SHA-256, not BLAKE3

The rest of qurb uses BLAKE3 and it is faster. This is the one value a person
may have to compare against something printed by `openssl x509 -fingerprint` or
shown by a browser, and being checkable with a tool that already exists is worth
more here than consistency with the rest of the codebase.

Accepted with colons or without, in any case, because that is how the tools
people will paste from print it.

## The certificate is kept, not regenerated

`--tls` with no certificate given makes one in the state directory and reuses it
for ever. The fingerprint is what every device has been told to expect, so a
service that generated a new certificate each time it restarted would lock out
every device it had — and would do it on a restart, which is the moment nobody
is watching.

## What this does not do

- **It does not authenticate the device to the service.** The rendezvous
  accepts anybody who can reach it; what it hands out is scoped by an
  identifier only that group's devices can derive. Unchanged by this.
- **It does not help the relay.** The relay needs no TLS of its own: what it
  carries is already an encrypted session it holds no key for.
- **It does not remove the need for a reachable host.** Two devices on
  different networks still need something with an address to meet at. That is
  what the host is for, and it is the one part nothing can remove.
