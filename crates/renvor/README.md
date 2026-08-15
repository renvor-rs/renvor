# renvor

Facade crate for the [Renvor](https://github.com/renvor-rs/renvor) framework.

> `renvor.dev` is reserved for the project but **serves no content yet**, so this README
> links to the repository instead. It will point at the site once that site is deployed.

## This release provides no runtime capability

Phase 001 of the Renvor programme establishes governance, naming, toolchain, and
repository security **before** any runtime code exists. This crate is deliberately empty
of capability: it exists so the workspace, package metadata, licence policy, and publish
rehearsal are exercised against a real crate rather than a hypothetical one.

It exposes three constants — `VERSION`, `MSRV`, and `EXECUTABLE` — and nothing else.

## The command is `renover`, not `renvor`

The product is **Renvor**. The installed executable is **`renover`**. The difference is
deliberate and permanent; it is not a typographical error. See
[ADR-0001](https://github.com/renvor-rs/renvor/blob/main/decisions/0001-public-naming-and-namespace.md).

## Supported Rust versions

The minimum supported Rust version is **1.94.0**. This is a fixed support floor, not a
rolling offset from current stable — a new Rust release does not invalidate it. Raising it
requires a planned minor or major release and an accepted decision record. See
[`SUPPORT.md`](https://github.com/renvor-rs/renvor/blob/main/SUPPORT.md).

## Licence

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.

**Project code generated for you by Renvor tooling carries no Renvor licensing
obligation** and is yours outright.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this crate by you, as defined in the Apache-2.0 licence, shall be dual
licensed as above, without any additional terms or conditions.
