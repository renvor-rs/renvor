# Renvor examples

**The examples live in [`../crates/renvor/examples/`](../crates/renvor/examples/), not here.**

They are Cargo example targets of the `renvor` facade, which is what lets them exercise the real
public surface a consumer sees. Run them with:

```bash
cargo run -p renvor --example minimal
cargo run -p renvor --example providers
cargo run -p renvor --example configuration     # needs the default `config` feature
```

> **This directory is why quickstart Gate 12 was broken.** The path convention said examples lived
> at `examples/`; the examples were written as facade targets instead; and the gate globbed
> `examples/*.rs` from the repository root. `for f in <no matches>` runs its body zero times and
> exits 0, so the gate printed `GATE 12 PASS` having run nothing at all. The gate now discovers the
> real directory and fails if it finds fewer than three — see T123.

Every example **compiles**, **runs**, and uses **no hidden global mutable state** — no ambient
singleton, no lazily-initialised global registry, no process-wide mutable default (spec FR-032).
An example that needs a global to work is demonstrating a design the framework does not have.

No example requires a transport, a port, or a database (SC-014).
