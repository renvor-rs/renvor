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
> `examples/*.rs` from here, where no `.rs` file has ever existed.
>
> **Corrected 2026-08-16 (T138).** This note used to say the loop "runs its body zero times and
> exits 0, so the gate printed `GATE 12 PASS` having run nothing at all". That is not what an
> unmatched glob does, and both shells were measured before this was rewritten. bash leaves an
> unmatched glob **literal** unless `nullglob` is set, so the body ran once with
> `f=examples/*.rs`, `basename` reduced it to `*`, and `cargo run --example '*'` exited non-zero —
> the script died with status **101**. zsh treats an unmatched glob as a hard error and never
> entered the loop, status **1**. The gate therefore **failed loudly** rather than passing
> vacuously. What went wrong was upstream of the shell: a pass was recorded for a gate that could
> not have run to completion. The gate now discovers the real directory, fails if it finds fewer
> than three examples, and is verified under both shells — see T123 and T138.

Every example **compiles**, **runs**, and uses **no hidden global mutable state** — no ambient
singleton, no lazily-initialised global registry, no process-wide mutable default (spec FR-032).
An example that needs a global to work is demonstrating a design the framework does not have.

No example requires a transport, a port, or a database (SC-014).
