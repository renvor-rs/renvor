# Phase 012 — Evidence

**Phase**: 012 — REST documentation and production examples; the L-1 (TLS) and L-2 (toolchain)
carry-overs from Phase 011
**Companion to**: [`phase-012-specification-and-decision-brief.md`](phase-012-specification-and-decision-brief.md)
(the specification; every `FR-012-n`, `SR-012-n`, `AC-012-n`, `U-n`, and `A-n` below is defined
there) · [`phase-012-task-plan.md`](phase-012-task-plan.md) (the requirements → tasks →
acceptance → evidence matrix; every implementation batch appends here) ·
[`phase-012-security-carryover.md`](phase-012-security-carryover.md) ·
[`phase-011-limitations.md`](phase-011-limitations.md) (L-1 and L-2, the rows this phase works
against — both **open**)
**Drafted**: 2026-09-07, as a skeleton, by the B1 documentation task (T-012-02), on branch
`feat/phase-012-b1-toolchain`, stacked on `docs/phase-012-decision-brief` (pull request #65,
unmerged) — the planning branch's head at the time of drafting is `6c0d780`; `origin/main` is
`7281e4f91aeb56695d6eceb322065e5f5fca04ef` (the squash of pull request #64)
**Status**: **in progress — B1 not merged.** Nothing here is closed. No limitation is closed, no
decision record is accepted (ADR-0038 is `proposed`), no waiver is created or granted, nothing is
tagged, published, or deployed. Every `__PLACEHOLDER__` below is filled by the batch that earns
it, with the head, the run identifiers, and the compiler identities of the legs that proved it —
and until it is filled the row claims nothing.
**Convention**: a head is a full commit SHA with its tree; a run is a GitHub Actions run
identifier with the job and leg named; a compiler identity is the leg's `rustc -vV` release,
commit, and host and its `cargo -vV` release and commit — three fields and two, never one string.
Evidence is described as what it is: a **launch observation** (Cargo's `Running` line), a
**queried identity** (a binary's own `-vV` or `--version` answer), or a retained measurement — never
as proof of execution through a wrapper.

## 1. L-2 — B1 (T-012-01 … T-012-12): the toolchain activation unit

| Item | Value |
|---|---|
| Branch | `feat/phase-012-b1-toolchain`, stacked on `docs/phase-012-decision-brief` (#65) |
| Head at the review checkpoint | `__HEAD__` (tree `__TREE__`) |
| CI run identifiers | `__RUN__` — `verify (1.94.0)`, `verify (stable)`, the four `platform (…)` legs, `security`, `docs`, CodeQL |
| Compiler identity, MSRV leg (`verify (1.94.0)`) | `__IDENTITY_MSRV__` — `rustc -vV` release, commit, host; `cargo -vV` release, commit |
| Compiler identity, stable leg (`verify (stable)`) | `__IDENTITY_STABLE__` — the same five values; the release CI resolved for the current stable channel on that day |
| Control toolchain per leg (U-10) | the MSRV leg's control is `stable`; the stable leg's control is `1.94.0`; the `Control toolchain identity` step proves the two differ in release **and** commit — run identifiers and the two identities: `__CONTROL_IDENTITIES__` |
| What the batch delivers | the contract revisions C-4 1.3.0, C-5 1.2.0, C-1 1.5.0, C-2 (additive), `support-policy.md` 1.2.0 (T-012-02); ADR-0038 `proposed` (T-012-01); templates at version 8 with the `toolchain` group (T-012-03); the framework-checkout reads (T-012-04); the version-2 record with `[toolchain]` and `[verified_with]` and the reader dispatch (T-012-05); the seal that forces `RUSTUP_AUTO_INSTALL=0` and drops the install-server pair (T-012-06); the identify-before-invoking preflight (T-012-07); the evidence mechanism as amended (T-012-08); the two notices (T-012-09); selection across staging and placement (T-012-10); legacy trees (T-012-11); no provisioning, offline preserved (T-012-12) |
| What the batch does **not** deliver | `doctor`'s toolchain section, xtask step 1's identity line, the census assertion, the dated `SUPPORT.md`/`rust-toolchain.toml` sentences, and the limitations ledger — all **B1f** (T-012-13 … T-012-17); `verification-sequence.md` 2.4.0 moves with B1f, because its step-1 text describes an xtask change B1 does not make |
| Acceptance tests, RED then GREEN | the tests named in `phase-012-task-plan.md` §1.1 for every requirement above; the refusal envelopes retained under `__ENVELOPES__`; the `SKIPPED:` lines observed locally for the two-toolchain controls and their required runs on both CI legs under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` |
| Contract numbers confirmed against the brief's §8 | `__SECTION_8_DIFF__` — the revision texts diffed against the compatibility table in the pull request body; the maintainer's disposition of the old-reader/new-record item (1.3.0 recommended; 2.0.0 the alternative) |
| Out-of-repo evidence cited | §2 below; `/Users/ahmedanbar/Documents/renvor/renvor-t-012-08m-evidence/2026-09-07-eedd9ed/` |

### 1.1 `clippy-driver` identity query

The observed `clippy-driver` **executable** — the one Cargo's `Running` line names — is queried
itself with its supported version query, `clippy-driver --version`, under the seal and after
FR-012-7a's identification safeguards (brief §4.3.2, the clippy correction; FR-012-7d (b)). The
query has to parse under FR-012-7e's grammar (`clippy <release> (<hex> <date>)`) on every
supported toolchain, or the observed driver cannot be identified.

**Measurement, recorded verbatim as stated by the B1 orchestrating session (2026-09-07):**
`clippy-driver --version` was run on the installed toolchains **1.90.0, 1.94.0, 1.95.0, and
1.97.1**, in an isolated directory, and answered under the grammar with exit 0; on **1.94.0** the
answer is

```text
clippy 0.1.94 (4a4ef493e3 2026-03-02)
```

which parses to `driver_release = "0.1.94"`, `driver_commit = "4a4ef493e3"`.

What is and is not established by this:

- The retained T-012-08m directory does **not** carry the raw output of this query. It carries
  the **launcher's** answer — `cargo clippy --version` via the pin — in
  `validator-extension/remeasure.out` (line 34: `clippy 0.1.94 (4a4ef493e3 2026-03-02)`), the same
  string on 1.94.0. That the two agree on a real toolchain is exactly why control (xiv) of
  FR-012-7d uses a stub whose launcher and driver answers differ: the record must be shown to
  carry the driver's.
- The answers on 1.90.0, 1.95.0, and 1.97.1 are not quoted here because they were not handed
  over as strings; they are retained by T-012-08's per-leg test
  `clippy_driver_version_answers_under_the_grammar_on_this_toolchain`, whose output on both CI
  legs is: `__CLIPPY_DRIVER_ANSWERS__`.
- Nothing about a `clippy-driver` built from another channel, or about a toolchain outside the
  four, is claimed.

### 1.2 Actual-generator cache-path measurement (FR-012-7d (j))

Carried into implementation by the maintainer's U-2 amendment, not another feasibility round:
whether `renvor new`'s staging and `generate auth`'s scratch copy (`std::fs::copy`, which
**preserves the source mtime on macOS** — measured, §2) verify **all-`Fresh`** against a shared
absolute `CARGO_TARGET_DIR`, on each supported platform leg. The result decides nothing about
correctness — a cached run is a supported, explicitly recorded outcome — but it is the fact the
cached-representation rule was written for, and it must be measured rather than assumed.

| Platform leg | `renvor new` staging against a shared target: `observation` | `generate auth` scratch copy against a shared target: `observation` | Run identifier | Notes |
|---|---|---|---|---|
| `ubuntu-latest`, 1.94.0 | `__NEW_LINUX_MSRV__` | `__AUTH_LINUX_MSRV__` | `__RUN__` | |
| `ubuntu-latest`, stable | `__NEW_LINUX_STABLE__` | `__AUTH_LINUX_STABLE__` | `__RUN__` | |
| `macos-latest`, 1.94.0 | `__NEW_MACOS_MSRV__` | `__AUTH_MACOS_MSRV__` | `__RUN__` | `fs::copy` preserves mtime here (§2) |
| `macos-latest`, stable | `__NEW_MACOS_STABLE__` | `__AUTH_MACOS_STABLE__` | `__RUN__` | |
| `windows-latest`, 1.94.0 | `__NEW_WINDOWS_MSRV__` | `__AUTH_WINDOWS_MSRV__` | `__RUN__` | |
| `windows-latest`, stable | `__NEW_WINDOWS_STABLE__` | `__AUTH_WINDOWS_STABLE__` | `__RUN__` | |

**Local measurement (macOS 14, `aarch64-apple-darwin`, rustc 1.94.0, 2026-09-07).** Two
consecutive `renvor new` runs against one absolute `CARGO_TARGET_DIR` both recorded
`observation = "launched"`, with `units_fresh = 0` for clippy, build, and test in each. The
mechanism is the staging name: C-5 stages under `<destination parent>/.renvor-staging-<pid>-<nanos>-<counter>`,
a different absolute path on every run, and Cargo's fingerprints are path-sensitive, so the
second run's units are not the first run's units and nothing is `Fresh`. On this platform, by
this path, the generator does not reach the cached case — which is a fact about the staging
naming, **not** a claim that the cached case cannot arise (a caller who arranges an identical
staging path, or a future stable staging name, would reach it, and the record represents it).

The per-leg cells above are filled from `tests/generated.rs`'s
`a_second_generation_against_a_shared_build_directory_records_its_observation`, which asserts only
the invariant that an observation and an identity agree — a launch means an identity was queried,
no launch means the identity is absent rather than filled in — and prints the measurement with the
prefix `MEASUREMENT renvor-new-shared-target:` so each platform leg's log carries the answer.
`generate auth`'s scratch copy is not measured locally: it needs a framework checkout and a full
starter build, and the local disk budget did not allow one. Its cells are filled from the CI legs.

Each cell is `launched`, `cached`, or `mixed`, read from the placed record's `[verified_with]`,
with the `units_launched`/`units_fresh` counts of the build and test checks beside it when the
row is filled. The controlled, non-cached tests of T-012-08 (a private, empty target per control)
are a separate matter and are **not** what this table records.

## 2. T-012-08m — the evidence-mechanism feasibility round (complete 2026-09-07; cited, not copied)

T-012-08m measured four candidate evidence mechanisms against FR-012-7d's requirements and
controls, was validated in three rounds by a separate agent, and was extended once under the
maintainer's authorisation (the clippy check on a binary-only project with `CARGO_INCREMENTAL=0`).
Its evidence is retained **out of repo** at

```text
/Users/ahmedanbar/Documents/renvor/renvor-t-012-08m-evidence/2026-09-07-eedd9ed/
```

and is cited from here, from the brief (§4.3.1), from the task plan (§1.1 FR-012-7d), and from
ADR-0038 §Evidence. It is not copied into the tree: it is measurement data from one machine (Darwin
25.3.0 arm64, cargo 1.94.0, APFS), retained as research evidence, and the artifact analysis in it
is explicitly **not** a production mechanism (U-2: no artifact scanning in production, no
mandatory witness).

| File | What it holds |
|---|---|
| `00-toolchains.txt` | the bare-binary identities of 1.94.0, 1.95.0, and 1.97.1 (`rustc -vV`, `cargo -vV`); rustup 1.29.0's `--version` under FR-012-7a step (2)'s isolation (`info: no `rustc` is currently active`; the isolated `RUSTUP_HOME` received only `settings.toml`) |
| `01-runs.jsonl`, `03-phase2.jsonl`, `runs/`, `obs/` | the raw runs and observations of the four candidates |
| `02-summary.json` | the sentinels (`probe/keep`, `foreign/marker`) unchanged before and after |
| `07-comparison.md` (and `07-comparison-v1-superseded.md`) | the comparison of the candidates, with a dated erratum on the `std::fs::copy` sentence |
| `08-validation-round-1.md`, `08-validation-round-2.md` | the separate agent's validation of the comparison |
| `09-clippy-identity.txt` | the clippy launch chains and producer markers under ordinary selection and under `RUSTC` = 1.95.0: the chain's trailing `rustc` names the override while the driver's artifacts carry 1.94.0's marker |
| `10-clippy-bin-only-incremental-off.{txt,jsonl}`, `11-extension-clippy-witness.md`, `12-validation-extension.md`, `validator-extension/` | the extension: with incremental off, the clippy check on a binary-only crate writes only 0-byte `.rmeta` and `.d` files — no eligible marker — so the mandatory per-check witness **refuses a supported configuration**; validated **CONFIRMED** |
| `13-fscopy-mtime.txt`, `validator-extension/fscopy-probe` | `std::fs::copy` preserves the source mtime on macOS on both code paths (measured 2026-09-07T11:28:09Z) |
| `scripts/` | the measurement scripts |

What the round decided, and what it did not: it led to the maintainer's U-2 policy amendment
(brief §4.3.1) — launch observation plus queried identity from the actual checks, cached checks
represented explicitly, no mandatory artifact witness — and to A-8 and the clippy correction
(§4.3.2). It implemented nothing, and the actual generator's cache paths (§1.2) were deliberately
left to T-012-08 rather than to another feasibility round.
