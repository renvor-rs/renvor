# renvor-testkit

Test harness for [Renvor](https://renvor.dev) applications.

Start a real application, inject a failure at a chosen lifecycle phase, and assert on the order
that actually happened — with no HTTP client, no port, and no database. Deadlines and drain
budgets are exercised without real elapsed time.

Add this crate under `[dev-dependencies]`. Nothing in `renvor`, `renvor-core`, or `renvor-config`
depends on it, which is what keeps its time-control machinery out of a production binary.

Phase 011 adds, driver-free: deterministic **factories** (`factory`: `Sequence`, `Factory`,
`UserFactory`, `ItemFactory`), a socket-free **test application** (`app::TestApplication`, behind
`http`), and — for a test that spawns a real binary, as a generated starter's `tests/starter.rs`
does — a blocking loopback **client** (`client::http`, behind `client`; one ISC crate, `minreq`,
with no dependencies and never a TLS feature). Everything else in the crate still opens no socket.

## Stability

**This surface is explicitly unstable.** See the [`renvor`](https://crates.io/crates/renvor)
facade documentation.

## Licence

`MIT OR Apache-2.0`, at your option.
