# renvor-seaorm

The SeaORM adapter for the [Renvor](https://renvor.dev) framework.

Implements `renvor-database`'s transport-independent ports against PostgreSQL and MySQL, so an
application writes idiomatic SeaORM inside explicit Renvor transaction boundaries.
Select exactly one database with the `db-postgres` or `db-mysql` feature; neither is default.

SeaORM is built on SQLx, so this resolves SQLx transitively. Selecting SeaORM changes the
programming model, not the driver family. Direct-SQLx application APIs are **not** part of this
crate's surface.

**Pre-release, unpublished, and explicitly unstable.**

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
