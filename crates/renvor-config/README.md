# renvor-config

Typed, layered configuration and secret redaction for the [Renvor](https://renvor.dev) framework.

Values arrive from exactly three source kinds — built-in defaults, TOML files, and environment
variables. Every source is decoded against the declared schema **before** any merging occurs, so a
conflict between layers fails with an error naming both, rather than being resolved by silently
picking a winner.

## Stability

**This surface is explicitly unstable.** See the [`renvor`](https://crates.io/crates/renvor)
facade documentation.

## Licence

`MIT OR Apache-2.0`, at your option.
