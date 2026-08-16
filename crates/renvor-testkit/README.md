# renvor-testkit

Test harness for [Renvor](https://renvor.dev) applications.

Start a real application, inject a failure at a chosen lifecycle phase, and assert on the order
that actually happened — with no HTTP client, no port, and no database. Deadlines and drain
budgets are exercised without real elapsed time.

Add this crate under `[dev-dependencies]`. Nothing in `renvor`, `renvor-core`, or `renvor-config`
depends on it, which is what keeps its time-control machinery out of a production binary.

## Stability

**This surface is explicitly unstable.** See the [`renvor`](https://crates.io/crates/renvor)
facade documentation.

## Licence

`MIT OR Apache-2.0`, at your option.
