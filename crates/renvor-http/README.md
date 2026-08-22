# renvor-http

REST and HTTP delivery adapter for the [Renvor](https://renvor.dev) framework.

**Pre-release, unpublished, and explicitly unstable.** Breaking changes are permitted without a
compatibility procedure while the instability window described in the facade's documentation is
open. No semantic-versioning promise applies.

This crate is Renvor's first real transport. It depends **inward** on `renvor-core` and brings the
HTTP stack with it; the kernel itself resolves no HTTP dependency under any feature combination.

Most users should take the `renvor` facade with the `transport-rest` feature rather than depending
on this crate directly.

## Security defaults

Deny-first, and each default is the answer to a question an attacker would otherwise answer:

- **no trusted proxies** — forwarding headers are ignored unless the direct peer is explicitly trusted;
- **CORS denies by default** — exact origins only, and a wildcard origin with credentials is refused
  when the configuration is built, not when a request arrives;
- **host validation fails closed** against explicit configuration;
- **request identifiers are always generated** — a caller-supplied value never becomes trusted identity.

The normative statements are `contracts/http-security.md`, `contracts/http-routing.md`, and
`contracts/http-runtime.md` in the [Renvor repository](https://github.com/renvor-rs/renvor).

## Licence

`MIT OR Apache-2.0`, at your option.
