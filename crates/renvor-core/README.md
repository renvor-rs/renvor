# renvor-core

The transport-independent kernel of the [Renvor](https://renvor.dev) framework: lifecycle,
provider graph, typed state, cancellation, health, and the error taxonomy.

There is no HTTP here, no persistence, and no CLI — by requirement, not by omission.

## Stability

**This surface is explicitly unstable.** Breaking changes are permitted without a compatibility
procedure, and no semantic-versioning promise applies while the instability window is open.

Most users should depend on the [`renvor`](https://crates.io/crates/renvor) facade rather than on
this crate directly. Depending on `renvor-core` directly is supported when you want the kernel
without the configuration layer's dependencies.

## Licence

`MIT OR Apache-2.0`, at your option.
