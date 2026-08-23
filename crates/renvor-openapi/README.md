# renvor-openapi

OpenAPI 3.2.0 description generation and API compatibility checking for the
[Renvor](https://renvor.dev) framework.

The description is generated from the same route registry that builds the router and the same
schema values the runtime enforces, so it cannot describe an API the application does not serve.
Output is deterministic, validated against the official OpenAPI 3.2 schema by an independent
validator, and compared against a committed snapshot with semantic breaking-change classification.

## Stability

**This surface is explicitly unstable.** See the [`renvor`](https://crates.io/crates/renvor)
facade documentation.

## Licence

`MIT OR Apache-2.0`, at your option.
