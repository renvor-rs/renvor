# renvor

Facade crate for the [Renvor](https://github.com/renvor-rs/renvor) framework.

> `renvor.dev` is reserved for the project but **serves no content yet**, so this README
> links to the repository instead. It will point at the site once that site is deployed.

## What this crate is, and what it is not

This is the facade over Renvor's **transport-independent kernel**. It re-exports
`renvor-core`, and — behind the default-on `config` feature — `renvor-config`.

It gives you:

- a seven-phase application lifecycle: `Load`, `Validate`, `Register`, `Boot`, `Ready`,
  `Drain`, `Stop`, with rollback in reverse **actual** initialisation order;
- providers with declared capabilities, resolved in a single pass under a counted work
  budget, refusing cycles and ambiguity by naming every party involved;
- layered configuration — defaults, TOML files, environment — with per-key attribution and
  a `Secret<T>` that is redacted in every output form;
- an enforced deadline on **every** call the kernel makes into your code;
- liveness and readiness as two independent answers, because a draining application is
  alive and not ready;
- a failure-injection harness covering all 21 phase-and-behaviour combinations.

It does **not** give you a transport. There is no HTTP server, no database adapter, and no
way to receive a request — that is Phase 004's work. You can start and stop an application;
you cannot yet serve anything with one.

**Nothing is published, and every API is explicitly unstable.** This crate does not exist on
crates.io. Its surface will change once the first real transport adapter exercises it.

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
