# renvor-error

Stable public API error codes and [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) Problem
Details for the [Renvor](https://renvor.dev) framework.

A public API error code is a compatibility promise, so it lives here rather than in the HTTP
adapter — a promise that lives in the transport is a promise about that transport. This crate
depends on `serde` and `serde_json` and nothing else: no status-code type, no header type, no
router.

Redaction is enforced by the types rather than by review. `detail` is a `&'static str` derived from
the code, so no runtime value can inhabit it; `InvalidParam` has no field a rejected value could
occupy; and a reason is a `&'static str` drawn from a closed vocabulary, so a third-party
validator's message — which quotes the offending input — cannot be stored in one.

## Stability

**This surface is explicitly unstable.** See the [`renvor`](https://crates.io/crates/renvor)
facade documentation.

## Licence

`MIT OR Apache-2.0`, at your option.
