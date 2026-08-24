# renvor-database

Transport-independent persistence ports for the [Renvor](https://renvor.dev) framework.

This crate names no database driver. It defines the repository and unit-of-work contracts, the
explicit transaction API, bounded connection settings, the ordered and checksummed migration
contract, seed scopes, and keyset pagination bound to contract C-15.

The adapter that names a driver is `renvor-sqlx`.

**Pre-release, unpublished, and explicitly unstable.**

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
