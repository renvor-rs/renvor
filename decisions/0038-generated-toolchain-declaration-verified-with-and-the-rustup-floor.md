# ADR-0038: Declare every generated project's toolchain, record what verified it as launch observation plus queried identity, provision nothing inside the seal, and require rustup 1.28.1

| Field | Value |
|---|---|
| **ID** | 0038 |
| **State** | `proposed` |
| **Reviewer** | none yet — proposed; accepted only by the maintainer under the single-maintainer waiver pattern |
| **Review date** | none yet — proposed; accepted only by the maintainer under the single-maintainer waiver pattern |
| **Superseded by** | *(not superseded)* |

> **This record is `proposed` in the Phase 012 B1 pull request and is accepted by nobody.** No
> review of it, independent or otherwise, has occurred, and none is claimed. It is drafted here
> because the batch that carries its behaviour is the batch that must propose it
> (`governance/phase-012-specification-and-decision-brief.md` §6.8); it confers no authority,
> closes no limitation, and changes no support promise until a maintainer marks it `accepted` —
> which, with no independent reviewer available, would itself require a waiver recorded in
> [`governance/waivers.md`](../governance/waivers.md) with an absolute expiry date, as every
> accepted record since ADR-0003 has. Automated and maintainer reviews are **advisory**, never
> independent.
>
> What it rests on is stated in §Evidence with dates and paths, and nothing in it is described as
> merged, measured by CI, or closed: B1 is on branch `feat/phase-012-b1-toolchain`, stacked on the
> planning branch of pull request #65, and neither pull request is merged.

## Context

**L-2, carried from Phase 011.** A generated project was verified — `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo build`, `cargo test`, a route-dump start —
with whatever compiler resolved on the generating machine, and the tree recorded nothing about
which one. A tree verified on one compiler may fail `clippy -D warnings` on another, so the
verification proved something about a compiler the tree could not name
(`governance/phase-011-limitations.md`, L-2; `phase-012-security-carryover.md` §2). The
maintainer decided the mechanism on 2026-09-06 (D-L2-1): a rendered `rust-toolchain.toml` **and**
`rust-version` in `Cargo.toml`, with `verified_with` in the provenance record — and left toolchain
selection, override handling, operation without rustup, evidence, and compatibility to be
specified together. The brief's §5 specifies them; this record is the lasting commitment behind
it.

**The generator must never install a compiler.** Constitution III and VI, and the offline
guarantee of FR-006, forbid a generator that downloads what it needs. Yet a rustup proxy run in a
directory that names a toolchain — or with `RUSTUP_TOOLCHAIN` set to one — installed that
toolchain **from a listing as innocent as `rustup --version`** before rustup 1.28.0
(`find_or_install_override_toolchain_or_default` in `rustup_mode.rs`'s `--version` branch); and
1.28.0 is the release that introduced `RUSTUP_AUTO_INSTALL`, the variable that turns the
behaviour off (brief §2.2, F-2; §5.3 FR-012-7a). A generated pin therefore creates the exact
situation in which an old rustup provisions silently, which is why the declaration and the
no-provisioning protection had to become active in one head (task plan §2, correction 1 of
2026-09-07).

**What evidence a verification can honestly give.** The 2026-09-06 draft wanted the record to
carry "the compiler actually used". T-012-08m (2026-09-07; four candidate mechanisms measured
against the evidence requirement, validated in three rounds by a separate agent) and its
maintainer-authorised extension showed the limits: Cargo's `Running` lines are a **launch
observation**, the launched binary's `-vV` answer is a **queried identity**, an artifact's
embedded marker is a **producer identity** — and none of the three alone proves fresh execution
through a wrapper. The extension showed that the recommended mandatory per-check artifact
witness **refuses a supported configuration**: the clippy check on a binary-only project with
`CARGO_INCREMENTAL=0` (the starter's shape, and the repository's own `starter_matrix.rs` and
`parity.rs` run `renvor new` that way) writes only 0-byte metadata and dependency-info files, so
there is no marker to witness. The maintainer amended the evidence requirement on 2026-09-07
(U-2, brief §4.3.1), then confirmed A-8 and corrected the clippy identity rule (brief §4.3.2).

**Cached checks.** Against a shared absolute `CARGO_TARGET_DIR` — which the seal honours and the
repository's own tests use — a check whose units are all `Fresh` passes without any compiler
launch. That made two sentences false as written: C-5's VERIFY step (step 5 in C-5 1.1.0's
numbering, step 4 since 1.2.0) "the generated project … **compiles**" and the constitution's "Generated projects must format, **compile**, migrate, start,
and execute representative operations". Round 3 of the brief reported the conflict rather than
resolving it (R3-3); the maintainer's reading is A-8 (2).

**A support promise moves.** Requiring rustup 1.28.1 is a floor the support policy did not
state, and [`SUPPORT.md`](../SUPPORT.md) and `contracts/support-policy.md` change only through a
decision record. That is the second reason this record exists.

## Decision

1. **Every generated tree declares its toolchain** (FR-012-1, FR-012-2): `rust-toolchain.toml`
   at its root — `channel = "<pin>"`, `components = ["rustfmt", "clippy"]`,
   `profile = "minimal"` — and `rust-version = "<msrv>"` in `Cargo.toml`, as one template group
   (`toolchain`) that renders at `renvor new` and whenever the record declares a pin, and never
   for a legacy record. A **starter**'s pin is the framework checkout's `rust-toolchain.toml`
   channel and its MSRV the checkout's `[workspace.package].rust-version`, each parsed on its own
   and then compared; a checkout whose pin is malformed, a channel alias (`stable`, `beta`,
   `nightly`, a dated nightly, a custom name, a `path`), or below its own MSRV is **refused by
   name before anything is staged** — an alias is never resolved to a version, silently or
   otherwise. A **skeleton**'s pin and MSRV are both the generator's own `CARGO_PKG_RUST_VERSION`
   (D-L2-4). The README states the pin, the MSRV, why the pin exists, how to change it, that
   rustup 1.28.1 or later honours it and that it is inert without rustup while `rust-version`
   still refuses older compilers (FR-012-3); the Dockerfile's builder tag derives from the pin and
   its builder stage sets `RUSTUP_AUTO_INSTALL=0` (FR-012-9). The template version moves to 8.

2. **The provenance record becomes version 2** (FR-012-4, FR-012-5): `record_version = 2`,
   `[toolchain]` (`pinned`, `rust_version` — rendered values), and `[verified_with]` in the layout
   of `contracts/template-contract.md` 1.3.0 §"Record version 2". Readers dispatch on
   `record_version`: absent is a legacy record, accepted and reported *unknown*; `2` is validated
   strictly; newer is `record_unsupported`, exit 3, before any plan. Only the operations that run
   the five checks — `renvor new` and `renvor generate auth` — write `[verified_with]`; `resource`
   and `migration` leave it byte-identical. The verified tree is fingerprinted by contents
   (`tree_digest` over a fixed scope), and `renvor check` reports the evidence as `current` or
   `historical` — never a count of operations, which nothing records. A generator built before
   this revision cannot read a version-2 record and says so through a generic parse error; the
   remedy is documented — *rebuild the generator, not the project* — not claimed away.

3. **`[verified_with]` is launch observation plus queried identity, measured, never derived**
   (FR-012-7d as amended, U-2 and A-8). The five checks run unchanged in what they compile, run,
   and require, with `-vv` on the three that can launch a compiler. Per check the record carries
   the outcome, the counts of the project's own units Cargo launched and positively reported
   `Fresh`, and — separately — the identity the launched compiler answered to `-vV`. The observed
   `clippy-driver` **executable** is queried itself (`clippy-driver --version`); the trailing
   `rustc` of clippy's chain is not its executing compiler, and `cargo clippy --version` never
   fills the driver's identity. The preflight resolution has its own labelled fields
   (`resolved_rustc_*`) and is never copied into the observed ones. A check whose units were all
   `Fresh` passes and is recorded **as cached** — `observation = "cached"`, the observed identity
   absent, one stderr line — and unavailable fields are never filled from the pin, `PATH`, an old
   record, or `.rustc_info.json`. Missing, truncated, malformed, or unparseable evidence is a
   refusal (`evidence_capture_failed`), never caching. No artifact marker is mandatory for any
   check; the generator scans no artifacts in production; the record carries no `executed_*` or
   `wrapper_substituted` field; and no incremental compilation, target, emit flag, timestamp, or
   cache is changed to manufacture an observation. Override and wrapper **presence** is recorded
   as booleans; no value, path, or raw child output enters the record.

4. **The seal provisions nothing, and identifies before it invokes** (FR-012-6, FR-012-7a/7b/7c,
   SR-012-1, SR-012-4): the sealed environment forces `RUSTUP_AUTO_INSTALL=0` and drops
   `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` for every child — the checks, the probes,
   `rustfmt` at generation, `doctor`'s probes. Before anything is staged: locate `rustup` without
   executing a proxy; run its `--version` only under an exclusively created isolation with no
   toolchain named; refuse below the floor with no proxy run; classify `rustc`/`cargo` as proxies
   by file identity, else probe them once under the same isolation; refuse a proxy whose rustup
   cannot be located (`proxy_unidentified`); only then run the resolution probe in the directory
   whose selection is measured, refusing a pinned-but-absent toolchain, a compiler below the MSRV,
   and a missing component by name, and confirming on an identified proxy that an uninstallable
   name answers "is not installed". The generator runs no rustup command that lists, installs,
   updates, or sets a default — `rustup --version` and `rustup show active-toolchain` only.
   `renvor dev` and `renvor routes` are not sealed but force the same variable and drop the same
   pair.

5. **Overrides proceed and are said aloud; nothing falls back silently** (FR-012-8, SR-012-2,
   SR-012-3). A resolution that is not the pin — an override, an absent rustup, a legacy tree —
   proceeds with one stderr line naming the resolved compiler and how it was selected; an observed
   identity that differs from the resolution prints a second line, whether or not the resolution
   equals the pin. `RUSTC`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `RUSTFLAGS`, and
   `RUSTDOCFLAGS` stay in the seal: they are the operator's trust, and refusing them would break
   every cache-using machine. The residual — a trusted wrapper may execute anything inside the
   sealed step — is a ledger row, not a refusal, and the record's own vocabulary ("launch
   observation plus queried identity") never claims otherwise.

6. **`generate auth` stages beside the project** (FR-012-13): its scratch copy is
   `<parent>/.renvor-staging-<pid>-…`, never the system temporary directory, so the copy shares
   the project's ancestors and therefore its directory overrides and ancestor toolchain files; the
   resolution is probed first in the project directory and then in the copy, and a divergence is a
   refusal. Legacy trees (template ≤ 7) keep working through every `generate` action; no pin is
   inserted into one, and every `generate` into one says so once (FR-012-10).

7. **rustup 1.28.1 or later is a support floor** for generating or verifying a pinned project
   (`contracts/support-policy.md` 1.2.0): older or unparseable versions are refused by name. It is
   a floor for the generator's guarantee, not a statement about how older releases behave; no old
   rustup is installed or measured to establish it, and the refusal is proven with stubs.

8. **The reading of "compiles" and "compile"** (A-8 (2), 2026-09-07). For C-5's VERIFY step and
   for the constitution, *"compiles" means the required Cargo build check succeeds, including valid cache
   reuse*. This asserts no fresh compiler execution. Formatting, linting, tests, startup, and
   every other required check stay mandatory; a positively cached check keeps its explicit cached
   status and its unavailable observed identity; a capture failure stays a failure. C-5 1.2.0
   states the reading in its body and leaves the step's verb untouched — it renumbers the step to
   4, because 1.2.0 also corrects the protocol block to the order the implementation follows
   (VERIFY, then RECORD, then MANIFEST, then the review, then PLACE); the constitution's text is
   not edited. The alternative of retaining unconditional identity claims ("the compiler that ran
   is always in the record") is removed, because those claims are false whether or not the
   resolution fields exist.

## Alternatives rejected

| Alternative | Rejected because |
|---|---|
| **Record-only `verified_with`, without a pin** (D-L2-1 option (d)) | it names the compiler after the fact and selects nothing: a second machine still verifies with whatever resolves, and the record becomes a note about a compiler the tree cannot ask for. The pin is what makes L-2's consequence — `clippy -D warnings` differing by compiler — a chosen state rather than an accident |
| **A pin without `rust-version`** (option (a)) | rustup honours the pin; a bare toolchain — a distribution compiler, an image without rustup — ignores the file entirely, and the tree would then build on any compiler at all with nothing refusing an older one. `rust-version` is Cargo's own floor and is enforced without rustup; the two together cover both worlds, and the README says which does what |
| **Refusing wrappers and overrides** (`RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`) | they are the operator's own trust, and their intended uses — build caches, cross-compilation — are exactly what CI machines run; refusing them breaks every cache-using machine to close a residual the record can state instead. The residual is a ledger row (SR-012-3), the presence is recorded as booleans, and the observation notice reports a differing identity out loud |
| **A mandatory per-check artifact witness** (T-012-08m's recommendation: launch evidence plus an artifact marker, refusing a run with none) | **rejected by T-012-08m's extension** (2026-09-07): the clippy check on a binary-only project with `CARGO_INCREMENTAL=0` — a supported configuration the repository's own starter matrix uses — writes only 0-byte `.rmeta` and `.d` files, so the witness would refuse every row of the census. Making it hold would mean forcing incremental compilation or an artifact-producing `--emit` on a command the operator did not choose. A producer identity does not prove execution this run either (a caching wrapper restores earlier artifacts). Artifact analysis stays retained research evidence |
| **Refusing all-`Fresh` runs** (the round-2 text of FR-012-7d) | a positively cached check is a real, supported outcome against a shared target directory, and refusing it — or clearing caches, touching timestamps, or forcing recompilation to manufacture a launch — changes what the operator asked for and still proves nothing about a wrapper. Representing the cached state explicitly, with the observed identity absent, keeps every claim inside what was measured (constitution: "Claims … MUST NOT exceed what was measured") |
| **`rustup toolchain list` in `doctor`** (the 2026-09-06 draft's "installed toolchains" table) | a listing command is a command rustup could act on, and the operator's question — "is the pin usable here, and what will run?" — is answered by proxy probes under `RUSTUP_AUTO_INSTALL=0` that cannot install. `doctor` runs `rustup --version` and `rustup show active-toolchain` only, and reports "not probed" below the floor (D-L2-7, brief §5.7; delivered by B1f) |
| **Resolving channel aliases** (`stable`, `beta`, `nightly`, a dated nightly) to a version at generation | the value would be true on the generating machine and on the generating day only; a starter pinned to "whatever `stable` was" is a pin to nothing. The framework's own checkout always carries an exact release, and a checkout that does not is refused by name (`toolchain_pin_unsupported`) rather than guessed at |

## Consequences

- **Every generated tree now carries a compiler choice**, and a generator built before this
  revision cannot read a version-2 record — a breaking change for source-built readers, of which
  nothing published exists (brief §8: 1.3.0 is recommended with the incompatibility stated; 2.0.0
  is the maintainer's alternative if source-built readers are inside the promise). The contract
  numbers stay provisional until the revision texts are diffed against §8 in the B1 pull request.
- **A `--framework-path` checkout pinned to an alias or below its own MSRV is refused** where it
  used to be accepted, because the generator never read the file before; recorded as a narrowing
  of accepted input, with no supported input refused.
- **The seal passes two fewer variables and forces one**, so an operator who relied on
  `RUSTUP_DIST_SERVER` reaching a staged build's rustup loses that — deliberately: the pair
  configures installs, which the seal now forbids. Crate fetching is unchanged (`CARGO_NET_OFFLINE`,
  the seeded lockfile).
- **Verification against a shared target directory can be legitimately cached**, and the record
  then names no observed compiler. Anyone who needs the compiler identity proven must give the
  run a private target — which is what the repository's own controlled tests do (AC-012-5: a
  cached census row is a test-infrastructure defect, never excused).
- **What is claimed is narrower than "the compiler that verified this tree"**: launch observation
  plus queried identity. A trusted wrapper can execute anything, and the record says nothing that
  a wrapper could make false. The carry-over plan's D-L2-1 wording ("the compiler actually used")
  is superseded by dated note, not rewritten.
- **A support floor exists for rustup**, which the policy must review with the MSRV under its
  quarterly rule; it is carried by this record and binds on acceptance.
- **What would reverse this**: a superseding record adopting an execution-proving mechanism that
  holds across wrappers and caches (none was found in T-012-08m), or a rustup release that
  removes the need for the floor — either would narrow, not widen, what the record claims.

## Evidence

Nothing below is a CI result, a merge, or an acceptance. Each item is dated and located; the
retained T-012-08m evidence is **cited, not copied**, and lives outside the repository.

| Item | Where | What it establishes |
|---|---|---|
| **T-012-08m** — four candidate evidence mechanisms measured against FR-012-7d's controls; three validation rounds by a separate agent | `/Users/ahmedanbar/Documents/renvor/renvor-t-012-08m-evidence/2026-09-07-eedd9ed/` (`07-comparison.md`, `08-validation-round-{1,2}.md`, `01-runs.jsonl`, `03-phase2.jsonl`, `runs/`, `obs/`) | launch observation, queried identity, and producer identity are three different things; clippy's launch chain is `[clippy-driver, rustc]` and under `RUSTC`/`build.rustc` the trailing argument named 1.95.0 while the driver's own artifacts were 1.94.0's; `cargo rustc --bin <name> -- -vV` causes no recompilation; a pre-existing `.renvor/probe/keep` survives (`02-summary.json`) |
| **T-012-08m extension** — the clippy check on a binary-only project with `CARGO_INCREMENTAL=0` | the same directory: `11-extension-clippy-witness.md`, `10-clippy-bin-only-incremental-off.{txt,jsonl}`, `12-validation-extension.md`, `validator-extension/` | clippy succeeds in every case and produces no eligible identity marker (only 0-byte `.rmeta` and `.d` files), so a mandatory per-check artifact witness refuses a supported configuration — the reason the fourth alternative above is rejected; validated **CONFIRMED** by the separate agent |
| **`std::fs::copy` preserves the source mtime on macOS** | the same directory: `13-fscopy-mtime.txt` (author) and `validator-extension/fscopy-probe` (validator); host Darwin 25.3.0 arm64, measured 2026-09-07T11:28:09Z | source and both copies at the same nanosecond mtime, on both code paths — so `generate auth`'s scratch copy does not by itself give new mtimes; the claim that it "guarantees fresh mtimes" is removed (U-2), and the actual-generator cache-path measurement is T-012-08's, recorded in `governance/phase-012-evidence.md` §1.2 when done |
| **rustup 1.29.0's `--version` resolves the active toolchain** | the same directory: `00-toolchains.txt` (measured 2026-09-07T09:16:17Z, under FR-012-7a step (2)'s isolation: `info: no `rustc` is currently active`; the isolated `RUSTUP_HOME` received only `settings.toml`) | why the floor check itself runs only under the isolation, never in a pinned directory (brief §4.3 correction 4) |
| **`clippy-driver --version` answers under the identity grammar** | measured 2026-09-07 by the B1 orchestrating session on the installed toolchains 1.90.0, 1.94.0, 1.95.0, and 1.97.1, in an isolated directory, exit 0; on 1.94.0 the answer is `clippy 0.1.94 (4a4ef493e3 2026-03-02)` (recorded in `governance/phase-012-evidence.md` §1.1). The retained T-012-08m directory carries the **launcher's** answer to `cargo clippy --version` via the pin (`validator-extension/remeasure.out`, line 34), the same string on 1.94.0 — which is exactly why control (xiv) needs a stub whose launcher and driver answers differ | the driver's own supported version query parses under FR-012-7e's grammar (`clippy <release> (<hex> <date>)`), so the observed executable can be identified itself; the per-leg verification of the query is T-012-08's test `clippy_driver_version_answers_under_the_grammar_on_this_toolchain`, not yet run in CI |
| **The A-8 reading of "compiles"/"compile"** | `governance/phase-012-specification-and-decision-brief.md` §4.3.1 R3-3 and §4.3.2 A-8 (2), 2026-09-07 | the required Cargo build check succeeds, valid cache reuse included; no fresh compiler execution is asserted — recorded in C-5 1.2.0's body beside the VERIFY step and in this record's decision 8 |
| **The rustup floor's origin** | brief §2.2 (F-2) and §5.3 FR-012-7a: `RUSTUP_AUTO_INSTALL` introduced in 1.28.0; 1.28.1 chosen as the floor | the floor is the first release in which the no-install guarantee can be requested; nothing about older releases is measured or claimed |

## Compliance

- **Constitution III and VI** — the generator installs nothing and evaluates nothing from the
  checkout; the seal forces the no-install variable and drops the install-server pair; refusals
  are by name, before anything is staged.
- **Constitution "Claims … MUST NOT exceed what was measured"** — the record's vocabulary is
  launch observation plus queried identity; a cached check is recorded as cached with the observed
  identity absent; no field claims execution through a wrapper.
- **Constitution VII and the generator obligation** — every generated tree formats, lints,
  builds (under the A-8 reading), tests, and starts before it is placed, with the compiler it
  declares or with an override said aloud.
- **Constitution XII** — a lasting public and operational commitment (the pin, the record
  format, the floor) is captured as a decision record, proposed before it is treated as accepted.
- **Contracts** — C-4 1.3.0, C-5 1.2.0, C-1 1.5.0, C-2 (additive), `support-policy.md` 1.2.0;
  `verification-sequence.md` 2.4.0 follows with the census assertion in B1f.
- **`PLAN.md` §6.1** — the ADR precedes the checklist and the tasks that implement it; A-0
  granted implementation authority for B1 on 2026-09-07 and no merge authority.
