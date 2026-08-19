# Phase 003 — Complete resolved transitive dependency inventory

**Feature**: [Phase 003 — interactive CLI](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/spec.md) | **Satisfies**: FR-044, SC-015 | **Tasks**: T083, T084
**Produced**: 2026-08-18 | **Toolchain**: 1.94.0 | **Source of truth**: the tracked `Cargo.lock`, read by `cargo metadata --locked`

## Why this document exists, and why it is not `research.md`

[`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md) evaluates the **direct candidates** — the
fifteen decisions D1–D15, each a package chosen deliberately. That is not the set that ships.
A consumer resolves the **transitive closure**, and every package in it carries a licence, an
MSRV, and an advisory history whether or not anybody evaluated it.

T083 says this explicitly: produce the inventory *"from the **actual `Cargo.lock`**, not from
`research.md`"*. An inventory derived from the design document would reproduce the design
document's blind spots — it would list fifteen packages and call the job done, when the real
number is 128.

**Reproduce it**: `cargo metadata --format-version 1 --locked`, walk the resolve graph from
`renvor-cli` over `normal` and `build` edges, and subtract that from the same walk including
`dev`. The exact script is not committed because the numbers below are the artifact; the command
is one line and the check is `cargo deny check`.

## Summary

| Measure | Count |
|---|---|
| External packages in the whole workspace lockfile | **184** |
| Reachable from `renvor-cli` over **normal/build** edges (what a consumer resolves) | **128** |
| **Dev-only** — test machinery, never in a consumer's graph | **49** |
| Declared directly in `crates/renvor-cli/Cargo.toml`, **external** (normal) | **11** |
| Declared directly, **external** (dev) | **6** |
| Distinct external packages declared directly, **either edge** | **14** |
| Arrived **transitively**, declared in no manifest of ours | **163** |
| Packages with **no declared licence** | **0** |
| Packages whose declared MSRV exceeds **1.94.0** | **0** |
| `cargo deny check licenses advisories bans sources` | **all four pass** |
| Duplicate-version warnings | **13** (policy: warn, not deny — see below) |

### Why 11, 6, and 14 are all correct, and none of them is 17

They measure different sets and the naive sum double-counts. `renvor-cli` declares **13** normal
dependencies, but two of those — `renvor-core` and `renvor-config` — are workspace members and so
are not *external* packages: 11 external. It declares **6** dev dependencies, of which
`serde_json`, `tempfile`, and `toml` are **also** declared as normal dependencies, so the union of
distinct external packages declared on either edge is **14**, not 17.

This is spelled out because the same class of arithmetic produced a wrong row in the Phase 002
inventory, which was corrected there for the same reason: a count is only useful if the set it
counts is named.

### The number that matters most

**128 packages reach the shipped executable and 11 of them were chosen on purpose.** The
other 117 arrived because something else wanted them. Research D1–D15 evaluated the ones we chose.
Nobody evaluated the other 117 individually, and this document does not pretend otherwise — what it
provides instead is the machine-checked properties that hold across *all* of them: every one has an
allow-listed licence, none declares an MSRV above the project's, and none carries an open advisory.

## What changed in Phase 003

Five packages were added directly by this phase, each with a recorded decision:

| Package | Version | Edge | Decision | Why |
|---|---|---|---|---|
| `cap-std` | 4.0.2 | normal | D6 rev 2 | Structural path containment. Replaced `walkdir`. |
| `semver` | 1.0.28 | normal | T065 | Version comparison in `doctor`. Already in the graph; this adds an edge, not a crate. |
| `wait-timeout` | 0.2.1 | normal | T071 | A bounded wait for the container probe. `std::process` has none. |
| `portable-pty` | 0.9.0 | **dev** | D15 | Drives the real wizard, on Windows as well as Unix. |
| `insta` | 1.48.0 | **dev** | D14 | Snapshots the JSON document shapes. |

`portable-pty` and `insta` are **dev-only** and therefore reach no consumer. That is stated because
`portable-pty` is the largest single addition — 13 crates, including `bitflags 1.3.2` beside the
`2.x` already present, and `shared_library 0.1.9` and `winreg 0.10.1`, both old and Windows-only.
None of them is in a shipped `renvor`.

## Duplicate versions

Thirteen crates resolve to more than one version:

`hashbrown`, `io-lifetimes`, `syn`, `windows-sys`, `windows-targets`, and the eight
`windows_*` target shims.

`deny.toml` sets `multiple-versions = "warn"` deliberately, and the reasoning is recorded there:
these are imposed by transitive dependencies and outside this project's control, so failing on them
would make the gate unactionable and invite pressure to weaken it. All but one predate Phase 003;
`bitflags` 1.3.2 was added by `portable-pty` and is **dev-only**.

## The absence assertions

Two properties of this closure are asserted **by tests** rather than by this document, because a
document goes stale and a test fails:

- **No archive-extraction crate is reachable** (FR-040) — `tests/capabilities.rs`. Demonstrated
  firing: adding `flate2` made it fail, naming `["flate2"]`.
- **No HTTP client is reachable** (FR-043) — same file, same walk, with a negative control proving
  the walk can see crates that *are* present.

Both walk the same lockfile this inventory reads.

## Normal closure — 128 packages reaching the executable

| Package | Version | Licence | Declared MSRV | Edge |
|---|---|---|---|---|
|`ambient-authority`|0.0.2|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|—|transitive|
|`anstyle`|1.0.14|MIT OR Apache-2.0|1.66.0|transitive|
|`bitflags`|2.13.1|MIT OR Apache-2.0|1.56.0|transitive|
|`block-buffer`|0.12.1|MIT OR Apache-2.0|1.85|transitive|
|`bytes`|1.12.1|MIT|1.57|transitive|
|`cap-primitives`|4.0.2|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|—|transitive|
|`cap-std`|4.0.2|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|—|direct|
|`cfg-if`|1.0.4|MIT OR Apache-2.0|1.32|transitive|
|`clap`|4.6.6|MIT OR Apache-2.0|1.85|direct|
|`clap_builder`|4.6.6|MIT OR Apache-2.0|1.85|transitive|
|`clap_derive`|4.6.4|MIT OR Apache-2.0|1.85|transitive|
|`clap_lex`|1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`convert_case`|0.10.0|MIT|—|transitive|
|`cpufeatures`|0.3.0|MIT OR Apache-2.0|1.85|transitive|
|`crossterm`|0.29.0|MIT|1.63.0|transitive|
|`crossterm_winapi`|0.9.1|MIT|—|transitive|
|`crypto-common`|0.2.2|MIT OR Apache-2.0|1.85|transitive|
|`derive_more`|2.1.1|MIT|1.81.0|transitive|
|`derive_more-impl`|2.1.1|MIT|1.81.0|transitive|
|`digest`|0.11.3|MIT OR Apache-2.0|1.85|transitive|
|`document-features`|0.2.12|MIT OR Apache-2.0|1.56|transitive|
|`dyn-clone`|1.0.20|MIT OR Apache-2.0|1.60|transitive|
|`equivalent`|1.0.2|Apache-2.0 OR MIT|1.6|transitive|
|`errno`|0.3.14|MIT OR Apache-2.0|1.56|transitive|
|`fastrand`|2.5.0|Apache-2.0 OR MIT|1.63|transitive|
|`fixedbitset`|0.5.7|MIT OR Apache-2.0|1.56|transitive|
|`foldhash`|0.1.5|Zlib|1.60|transitive|
|`fs-set-times`|0.20.3|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|—|transitive|
|`futures-core`|0.3.34|MIT OR Apache-2.0|1.36|transitive|
|`futures-sink`|0.3.34|MIT OR Apache-2.0|1.36|transitive|
|`getrandom`|0.4.3|MIT OR Apache-2.0|1.85|transitive|
|`hashbrown`|0.17.1|MIT OR Apache-2.0|1.85.0|transitive|
|`hashbrown`|0.17.1|MIT OR Apache-2.0|1.85.0|transitive|
|`heck`|0.5.0|MIT OR Apache-2.0|1.56|transitive|
|`hybrid-array`|0.4.14|MIT OR Apache-2.0|1.85|transitive|
|`indexmap`|2.14.0|Apache-2.0 OR MIT|1.85|transitive|
|`inquire`|0.9.4|MIT|1.80.0|direct|
|`io-extras`|0.19.0|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.70|transitive|
|`io-lifetimes`|3.0.1|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.70|transitive|
|`io-lifetimes`|3.0.1|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.70|transitive|
|`ipnet`|2.12.1|MIT OR Apache-2.0|—|transitive|
|`itoa`|1.0.18|MIT OR Apache-2.0|1.68|transitive|
|`libc`|0.2.189|MIT OR Apache-2.0|1.65|transitive|
|`linux-raw-sys`|0.12.1|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.63|transitive|
|`litrs`|1.0.0|MIT OR Apache-2.0|1.56|transitive|
|`lock_api`|0.4.14|MIT OR Apache-2.0|1.71.0|transitive|
|`log`|0.4.33|MIT OR Apache-2.0|1.71.0|transitive|
|`maybe-owned`|0.3.4|MIT OR Apache-2.0|—|transitive|
|`memchr`|2.8.3|Unlicense OR MIT|1.61|transitive|
|`memo-map`|0.3.3|Apache-2.0|1.43|transitive|
|`minijinja`|2.24.0|Apache-2.0|1.70|direct|
|`mio`|1.2.2|MIT|1.71|transitive|
|`once_cell`|1.21.4|MIT OR Apache-2.0|1.65|transitive|
|`parking_lot`|0.12.5|MIT OR Apache-2.0|1.71|transitive|
|`parking_lot_core`|0.9.12|MIT OR Apache-2.0|1.71.0|transitive|
|`petgraph`|0.8.3|MIT OR Apache-2.0|1.64|transitive|
|`pin-project-lite`|0.2.17|Apache-2.0 OR MIT|1.37|transitive|
|`proc-macro2`|1.0.107|MIT OR Apache-2.0|1.71|transitive|
|`quote`|1.0.47|MIT OR Apache-2.0|1.71|transitive|
|`r-efi`|6.0.0|MIT OR Apache-2.0 OR LGPL-2.1-or-later|1.68|transitive|
|`redox_syscall`|0.5.18|MIT|—|transitive|
|`rustc_version`|0.4.1|MIT OR Apache-2.0|1.32|transitive|
|`rustix`|1.1.4|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.63|transitive|
|`rustix-linux-procfs`|0.1.1|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|1.63|transitive|
|`scopeguard`|1.2.0|MIT OR Apache-2.0|—|transitive|
|`secrecy`|0.10.3|Apache-2.0 OR MIT|1.60|transitive|
|`semver`|1.0.28|MIT OR Apache-2.0|1.68|direct|
|`serde`|1.0.229|MIT OR Apache-2.0|1.56|direct|
|`serde_core`|1.0.229|MIT OR Apache-2.0|1.56|transitive|
|`serde_derive`|1.0.229|MIT OR Apache-2.0|1.71|transitive|
|`serde_json`|1.0.151|MIT OR Apache-2.0|1.71|direct|
|`serde_spanned`|1.1.1|MIT OR Apache-2.0|1.85|transitive|
|`sha2`|0.11.0|MIT OR Apache-2.0|1.85|direct|
|`signal-hook`|0.3.18|Apache-2.0/MIT|—|transitive|
|`signal-hook-mio`|0.2.5|MIT OR Apache-2.0|—|transitive|
|`signal-hook-registry`|1.4.8|MIT OR Apache-2.0|1.26|transitive|
|`smallvec`|1.15.2|MIT OR Apache-2.0|—|transitive|
|`syn`|3.0.3|MIT OR Apache-2.0|1.71|transitive|
|`syn`|3.0.3|MIT OR Apache-2.0|1.71|transitive|
|`tempfile`|3.27.0|MIT OR Apache-2.0|1.63|direct|
|`terminal_size`|0.4.4|MIT OR Apache-2.0|1.71|transitive|
|`thiserror`|2.0.20|MIT OR Apache-2.0|1.71|transitive|
|`thiserror-impl`|2.0.20|MIT OR Apache-2.0|1.71|transitive|
|`tokio`|1.53.1|MIT|1.71|transitive|
|`tokio-macros`|2.7.2|MIT|1.71|transitive|
|`tokio-util`|0.7.19|MIT|1.71|transitive|
|`toml`|1.1.4+spec-1.1.0|MIT OR Apache-2.0|1.85|direct|
|`toml_datetime`|1.1.1+spec-1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`toml_parser`|1.1.3+spec-1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`toml_writer`|1.1.2+spec-1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`tracing`|0.1.44|MIT|1.65.0|transitive|
|`tracing-attributes`|0.1.31|MIT|1.65.0|transitive|
|`tracing-core`|0.1.36|MIT|1.65.0|transitive|
|`typenum`|1.20.1|MIT OR Apache-2.0|1.41.0|transitive|
|`unicode-ident`|1.0.24|(MIT OR Apache-2.0) AND Unicode-3.0|1.71|transitive|
|`unicode-segmentation`|1.13.3|MIT OR Apache-2.0|1.85.0|transitive|
|`unicode-width`|0.2.2|MIT OR Apache-2.0|1.66|transitive|
|`wait-timeout`|0.2.1|MIT/Apache-2.0|—|direct|
|`wasi`|0.11.1+wasi-snapshot-preview1|Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT|—|transitive|
|`winapi`|0.3.9|MIT/Apache-2.0|—|transitive|
|`winapi-i686-pc-windows-gnu`|0.4.0|MIT/Apache-2.0|—|transitive|
|`winapi-x86_64-pc-windows-gnu`|0.4.0|MIT/Apache-2.0|—|transitive|
|`windows-link`|0.2.1|MIT OR Apache-2.0|1.71|transitive|
|`windows-sys`|0.61.2|MIT OR Apache-2.0|1.71|transitive|
|`windows-sys`|0.61.2|MIT OR Apache-2.0|1.71|transitive|
|`windows-sys`|0.61.2|MIT OR Apache-2.0|1.71|transitive|
|`windows-targets`|0.53.5|MIT OR Apache-2.0|1.60|transitive|
|`windows-targets`|0.53.5|MIT OR Apache-2.0|1.60|transitive|
|`windows_aarch64_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_aarch64_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_aarch64_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_aarch64_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_gnu`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_gnu`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_i686_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_gnu`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_gnu`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_gnullvm`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`windows_x86_64_msvc`|0.53.1|MIT OR Apache-2.0|1.60|transitive|
|`winnow`|1.0.4|MIT|1.65.0|transitive|
|`winx`|0.36.4|Apache-2.0 WITH LLVM-exception|1.63|transitive|
|`zeroize`|1.9.0|Apache-2.0 OR MIT|1.85|transitive|
|`zmij`|1.0.23|MIT|1.71|transitive|

## Dev-only — 49 packages, never in a consumer's graph

| Package | Version | Licence | Declared MSRV | Edge |
|---|---|---|---|---|
|`anstream`|1.0.0|MIT OR Apache-2.0|1.66.0|transitive|
|`anstyle-parse`|1.0.0|MIT OR Apache-2.0|1.66.0|transitive|
|`anstyle-query`|1.1.5|MIT OR Apache-2.0|1.66.0|transitive|
|`anstyle-wincon`|3.0.11|MIT OR Apache-2.0|1.66.0|transitive|
|`anyhow`|1.0.104|MIT OR Apache-2.0|1.68|transitive|
|`bitflags`|2.13.1|MIT OR Apache-2.0|1.56.0|transitive|
|`bstr`|1.13.1|MIT OR Apache-2.0|1.65|transitive|
|`cfg_aliases`|0.1.1|MIT|—|transitive|
|`colorchoice`|1.0.5|MIT OR Apache-2.0|1.66.0|transitive|
|`content_inspector`|0.2.4|MIT/Apache-2.0|—|transitive|
|`crossbeam-deque`|0.8.7|MIT OR Apache-2.0|1.61|transitive|
|`crossbeam-epoch`|0.9.20|MIT OR Apache-2.0|1.61|transitive|
|`crossbeam-utils`|0.8.22|MIT OR Apache-2.0|1.60|transitive|
|`downcast-rs`|1.2.1|MIT/Apache-2.0|—|transitive|
|`dunce`|1.0.5|CC0-1.0 OR MIT-0 OR Apache-2.0|—|transitive|
|`either`|1.17.0|MIT OR Apache-2.0|1.63.0|transitive|
|`escargot`|0.5.15|MIT OR Apache-2.0|1.70|transitive|
|`filedescriptor`|0.8.3|MIT|—|transitive|
|`filetime`|0.2.29|MIT/Apache-2.0|1.75.0|transitive|
|`glob`|0.3.4|MIT OR Apache-2.0|1.63.0|transitive|
|`humantime`|2.4.0|MIT OR Apache-2.0|1.60|transitive|
|`humantime-serde`|1.1.1|MIT OR Apache-2.0|—|transitive|
|`insta`|1.48.0|Apache-2.0|1.66.0|direct|
|`is_terminal_polyfill`|1.70.2|MIT OR Apache-2.0|1.70.0|transitive|
|`lazy_static`|1.5.0|MIT OR Apache-2.0|—|transitive|
|`nix`|0.28.0|MIT|1.69|transitive|
|`normalize-line-endings`|0.3.0|Apache-2.0|—|transitive|
|`once_cell_polyfill`|1.70.2|MIT OR Apache-2.0|1.70.0|transitive|
|`os_pipe`|1.2.3|MIT|1.63|transitive|
|`portable-pty`|0.9.0|MIT|—|direct|
|`rayon`|1.12.0|MIT OR Apache-2.0|1.80|transitive|
|`rayon-core`|1.13.0|MIT OR Apache-2.0|1.80|transitive|
|`same-file`|1.0.6|Unlicense/MIT|—|transitive|
|`serial2`|0.2.38|BSD-2-Clause OR Apache-2.0|1.63|transitive|
|`shared_library`|0.1.9|Apache-2.0/MIT|—|transitive|
|`shell-words`|1.1.1|MIT/Apache-2.0|—|transitive|
|`shlex`|1.3.0|MIT OR Apache-2.0|1.46.0|transitive|
|`similar`|3.2.0|Apache-2.0|1.85|transitive|
|`similar`|3.2.0|Apache-2.0|1.85|transitive|
|`snapbox`|1.2.2|MIT OR Apache-2.0|1.85|transitive|
|`snapbox-macros`|1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`thiserror`|2.0.20|MIT OR Apache-2.0|1.71|transitive|
|`thiserror-impl`|2.0.20|MIT OR Apache-2.0|1.71|transitive|
|`toml_edit`|0.25.13+spec-1.1.0|MIT OR Apache-2.0|1.85|transitive|
|`trycmd`|1.2.1|MIT OR Apache-2.0|1.85|direct|
|`utf8parse`|0.2.2|Apache-2.0 OR MIT|—|transitive|
|`walkdir`|2.5.0|Unlicense/MIT|—|transitive|
|`winapi-util`|0.1.11|Unlicense OR MIT|—|transitive|
|`winreg`|0.10.1|MIT|—|transitive|

## Limitations of this inventory, stated rather than left to be discovered

1. **It is a snapshot.** It describes the lockfile at the commit that produced it. Any later
   resolution can differ, which is why `cargo deny check` runs in CI rather than being read off
   this page.
2. **"Declared MSRV" is what the package says**, not what it actually compiles with. A package with
   no `rust-version` shows `—`; that is an absent declaration, not a guarantee of compatibility.
   The real evidence for MSRV is `cargo xtask verify` passing on 1.94.0, which it does.
3. **Advisory status is as of the run above.** `cargo deny check advisories` passed with
   `ignore = []` — no advisory is suppressed — but an advisory published tomorrow is not in it.
   Response windows are in [`dependency-advisory-policy.md`](dependency-advisory-policy.md).
4. **The closure covers every target.** A Windows-only crate appears here even though it never
   compiles on Linux, because it does ship to Windows users.
