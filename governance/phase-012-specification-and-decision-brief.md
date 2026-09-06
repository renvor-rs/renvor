# Phase 012 — Specification and decision brief

**Companion to**: [`phase-012-task-plan.md`](phase-012-task-plan.md) (the requirements → tasks → acceptance → evidence matrix, the batches, and the consolidated list of unresolved implementation choices) · [`phase-012-security-carryover.md`](phase-012-security-carryover.md) (the L-1/L-2 carry-over plan; this brief supersedes its proposals where the two differ) · [`phase-011-limitations.md`](phase-011-limitations.md) · [`phase-011-evidence.md`](phase-011-evidence.md) (§14, the erratum) · `PLAN.md` §"Phase 012 — REST documentation and production examples", §18, §26 · `CONSTITUTION.md` · contracts C-1 (`command-surface.md` 1.4.0), C-2 (`json-output.md` 1.0.0, wire `schemaVersion` 2), C-4 (`template-contract.md` 1.2.0), C-5 (`generation-transaction.md` 1.1.0), `verification-sequence.md` 2.3.0, `support-policy.md` 1.1.2, C-C7 (`capabilities-contract.md` 1.1.0)
**Drafted**: 2026-09-06, against `main` at `bc979e83ca1bc35559d2f713ebc6686a08d9df85`, by the maintainer's session, on branch `docs/phase-012-decision-brief` (commit `0c9379f`)
**Revised**: 2026-09-07, against `main` at **`7281e4f91aeb56695d6eceb322065e5f5fca04ef`** (the squash of pull request #64, one parent `bc979e8`, tree `368396da82b8b4b3f62936d4d6921b0e025ece4b`), on the same branch, by new commits only — nothing in the branch's history is rewritten; the 2026-09-06 text is `0c9379f`
**Status**: **SPECIFICATION — planning approval requested.** The maintainer's dispositions of 2026-09-07 on **D-L1-1 … D-L1-7** and **D-L2-2 … D-L2-10** are recorded in §4 and applied throughout. This file, its companion task plan, and the carry-over plan implement nothing, change no contract, template, test, or workflow, accept no decision record, close no limitation, and grant no waiver. **L-1 and L-2 stay open** until implementation and validation prove the exact closure conditions in §9.8 and §5.9. Nothing is tagged, released, published, or deployed. The cheat sheet stays local and untracked.
**Working copy**: `specs/012-rest-documentation-and-production-examples/spec.md` under the gitignored `specs/` tree is the same text; this tracked file is the clone-visible mirror and the authority if the two differ.
**Identifiers**: brief-local — `FR-012-n` (functional), `SR-012-n` (security), `AC-012-n` (acceptance criteria), `D-L1-n`/`D-L2-n` (decisions; the numbers the carry-over plan gave them), `WI-012-n` (work items), `U-n` (unresolved implementation choices, listed only in the task plan §4), `T-012-nn` (tasks, task plan §1).

---

## 0. Baseline on 2026-09-07

### 0.1 What `main` is

| | |
|---|---|
| `origin/main` | `7281e4f91aeb56695d6eceb322065e5f5fca04ef` — `fix(ci): verify the selected Rust toolchain and support current stable (#64)`, squash-merged 2026-09-06T19:12:12Z at exactly `b6af9b7478f3c01bd286fb867dcfe8ee619e2b2b`; one parent `bc979e8` (PR #63's squash); tree `368396da…` equals the PR head's tree; GitHub-signed; subject-only, empty body |
| This branch | `docs/phase-012-decision-brief`, merge base `bc979e8`; carried forward by new commits on top of `0c9379f`; `main`'s one commit since the merge base (#64) touches no file this branch touches |
| Worktrees | eleven, all preserved; `framework-wi-012-4` still holds `ci/stable-legs-run-stable` at `b6af9b7`; `framework-phase-012-plan` still holds `docs/phase-012-security-carryover` at `3742ec1` |
| What #64 changed | `.github/workflows/ci.yml` (job-level `RUSTUP_TOOLCHAIN` and `RUSTUP_AUTO_INSTALL=0` on `verify` and `platform`; explicit `components: rustfmt, clippy` on both; the `Toolchain identity` step before the suite and the `Toolchain identity of the built artifacts` step after it), the new `.github/scripts/toolchain-identity.sh`, four `as_chunks::<2>()` sites (`renvor-core` `observe/trace_context.rs`; `renvor-auth` `csrf.rs`, `opaque.rs` ×2), two method-local `#[allow(clippy::result_large_err, reason = …)]` (`Application::boot`, `renvor_testkit::TestApplication::boot`), the §14 erratum appended to `phase-011-evidence.md`, and a three-line dated note in each earlier evidence record and in `waivers.md`. `xtask`, `SUPPORT.md`, `rust-toolchain.toml`, every contract, every template: **unchanged** |

### 0.2 WI-012-4 — completed, with the evidence

WI-012-4 (§11) is the only Phase 012 work item that is done. It is recorded here as it happened, per commit, because its history is the evidence and because two of its five commits were corrections the first genuine stable run forced.

| Commit (2026-09-06, UTC) | What it did | CI outcome on that head |
|---|---|---|
| `3107e38` 12:46 | The identity guard **without** the job-level selection — deliberately, so CI itself reproduced the wrong resolution | **All six verification legs failed at the guard** before any suite ran; the stable legs with "the checkout resolves rustc 1.94.0 … but the job's toolchain is 'stable'". This is the negative control §11's WI-012-4 acceptance asked for: with the variable absent, the guard alone fails the leg |
| `72050b0` 12:49 | Job-level `RUSTUP_TOOLCHAIN` and `RUSTUP_AUTO_INSTALL=0`; the erratum (`phase-011-evidence.md` §14) | `verify (1.94.0)` green; `verify (stable)` **red on clippy 1.98.1** — `result_large_err` at `lifecycle/application.rs` and `chunks_exact_to_as_chunks` at `observe/trace_context.rs` (log retained as `wi-012-4-pr64-72050b0-verify-stable-clippy-1.98.1-failure.log`); `platform (…, 1.94.0)` red for absent `rustfmt`/`clippy` (the file had been adding them implicitly, with a download). **The first directly verified genuine stable run in this investigation** — a failing one |
| `a70e41d` 13:44 | `components: rustfmt, clippy` on the platform legs | five legs green; `verify (stable)` still red on the same two lints |
| `276fed4` 16:59 | Maintainer's dispositions 1 and 2: `as_chunks::<2>()` at four sites; `allow(result_large_err)` on `Application::boot` only | run `34047182503`: `verify (stable)` red on **exactly one** remaining site, `renvor_testkit::TestApplication::boot` (`app.rs`), which the correction round had predicted and left unauthorised; `renvor-core` and `renvor-auth` clean on 1.98.1 |
| `b6af9b7` 18:14 | The same allow on the testkit boot; both reasons restated in the maintainer's wording | run `34051087194`: **every check green** — thirteen successes plus the documented `attest rehearsal artifacts` skip (workflow_dispatch only). **The first directly verified successful stable CI run in this investigation.** `verify (stable)`: rustc **1.98.1 (`48a229cea`)**, host `x86_64-unknown-linux-gnu`, all nine gate steps, census 87/87; artifacts in `target` and `target/starter-matrix` both 1.98.1. MSRV legs: rustc **1.94.0 (`4a4ef493`)** |
| `7281e4f` 19:12 (main) | The squash merge | runs `34054141909` (ci), `34054141901` (docs), `34054141903` (security), `34054141578` (CodeQL): all success; eleven check-runs, ten green, `dependency-review` skipped as on the previous `main` commit (push events carry no pull-request diff); every leg's identity step reports selection by `RUSTUP_TOOLCHAIN`, the absent-toolchain probe refused by name, artifacts matching; CodeQL rust 75 results / 25 rules, actions 0 / 17; open alerts 0 / 0 / 0 |

Retained out of repository beside the cheat-sheet evidence (`renvor-blog-api-cheatsheet-evidence/2026-09-06-ab701d2/logs/`): `ci-stable-legs-run-the-pin.log` (the original finding), `rustup-auto-install-probe.log` (F-2), the `72050b0` failure log, the `276fed4` and `b6af9b7` identity records and local-check records, and `wi-012-4-main-7281e4f-identities.log`. No local full gate was run for the unchanged, CI-verified head `b6af9b7`; the local checks recorded are targeted (fmt, clippy, the trace-context, lifecycle, opaque/csrf, and testkit suites, on 1.94.0 and on local stable 1.97.1 — which raises neither lint, so only CI's 1.98.1 proved the fix).

### 0.3 The `AC-012` requirements, mapped one by one

The brief of 2026-09-06 wrote seven acceptance criteria for the stable leg (§5.9 below keeps their numbers). WI-012-4 met three, half-met one, and left three — the ones that need the generated record — for the L-2 batch. They are **not** treated as met because CI selection is fixed.

| # | Requirement (short) | Status at `7281e4f` | Evidence / what remains |
|---|---|---|---|
| **AC-012-1** | job-level `RUSTUP_TOOLCHAIN` on `verify` and `platform` | **met** | `ci.yml` (the `env:` block of each job); the identity step asserts the variable and rustup's attribution to it |
| **AC-012-2** | `RUSTUP_AUTO_INSTALL=0` on every leg | **met** | same; the identity step's absent-toolchain probe (`RUSTUP_DIST_SERVER=http://127.0.0.1:9`) proves the runner's rustup honours it |
| **AC-012-3** | identity control step, inside the checkout, before the gate | **met and exceeded** | `toolchain-identity.sh identity` (release **and** commit of rustc and cargo against `rustc +<toolchain> -vV`; attribution; components) and `artifacts` (cargo's `.rustc_info.json` in `target` and `target/starter-matrix`) |
| **AC-012-4** | `cargo xtask verify` step 1 records the compiler in its own output | **not met** | `xtask` is unchanged in #64; the CI step records it, a local run does not. → T-012-14 |
| **AC-012-5** | the census asserts each placed record's `verified_with` equals the leg's compiler | **not met** | needs FR-012-4 (the record). → T-012-15 |
| **AC-012-6** | unit control: pin `X` + `RUSTUP_TOOLCHAIN=Y` → `Y`, `environment`; unset → `X`, `toolchain_file` | **not met** | needs the resolution probe (FR-012-7). → T-012-13 |
| **AC-012-7** | `SUPPORT.md` and the toolchain-file comment true again; dated erratum appended | **half met** | the erratum is §14 of `phase-011-evidence.md` (appended, nothing rewritten). The two sentences are true again by behaviour since `7281e4f` but carry no date, and `SUPPORT.md` is unchanged. → T-012-16 adds the dated note to both |

### 0.4 Dated history replaces the present tense

Every sentence of the 2026-09-06 draft that described the stable legs in the present tense ("compile with the pin", "have verified the MSRV a second time on every pull request", "what happens today", "will now run stable for the first time") is rewritten below as dated history. The rule for this brief and the task plan: a CI claim names the head, the run, and the date; an observed run is distinguished from an inferred window.

- **Observed** (job logs read through the GitHub API on 2026-09-06): three stable jobs — `verify (stable)` on `3742ec1` (run `34025561472`, job `101465783525`) and on `5acb492` (run `34018208859`, job `101445807521`), `platform (macos-latest, stable)` on `3742ec1` (job `101465783728`) — and one control, `docs` on `3742ec1` (job `101465783662`, given `1.94.0` by input on purpose). Then the seven heads of #64 (§0.2), and `main` at `7281e4f`.
- **Inferred**: the exposure window **2026-08-11 (`98a4e2c`, the day the pin landed; `ci.yml` followed in `e41ec4f` on 2026-08-12) → 2026-09-06 (`7281e4f`)**. The inference rests on configuration history, not on reading every run: `rust-toolchain.toml` changed in no commit after `98a4e2c`; `ci.yml` changed eleven times in the window and introduced `RUSTUP_TOOLCHAIN` only in #64 (`git log -S RUSTUP_TOOLCHAIN -- .github/workflows/ci.yml` names `3107e38` and `72050b0` alone); sixty-four first-parent merges into `main` lie in the window. Any "verified on stable" sentence sourced from CI in that window describes a second MSRV run; the local `cargo +stable xtask verify` legs recorded in the evidence files were genuine (`+stable` sets `RUSTUP_TOOLCHAIN`).
- The carry-over plan's sentences that read the old CI as stable proof ("CI runs `1.94.0` and `stable`" in its §2.2; the CI clause of its §2.4) are annotated there with a dated note, not rewritten.

---

## 1. Phase 012 scope map

`PLAN.md` states Phase 012 as: *versioned documentation site; API-only quickstart; authenticated application example; all Section 18 v1 documentation; deployment and hardening guides; searchable CLI/API references*, accepted when *documentation builds without broken links; commands run from clean environments; examples are exercised in continuous integration; claims link to evidence; all current limitations are visible*, with the set prepared in `renvor-rs/renvor-docs`, served at `docs.renvor.dev` from an immutable framework artifact, prerelease status stated on every public property until Phase 013 passes (§26.6, §26.7, §26.11).

| Stream | Content | Where specified |
|---|---|---|
| **S1** | Versioned, searchable documentation in `renvor-rs/renvor-docs` | §10.2 |
| **S2** | API-only quickstart and authenticated application example | §10.3 |
| **S3** | Every `PLAN.md` §18 v1 documentation topic | §10.4 |
| **S4** | Deployment and hardening guidance | §10.5 |
| **S5** | CLI/API references generated from an immutable framework artifact | §10.6 |
| **S6** | Clean-environment execution, CI-tested examples, evidence links, visible limitations, prerelease wording | §10.7 |
| **S7** | Security carry-over: L-2 (§3–§8) and L-1 (§9) | this brief |
| **S8** | Correctness triage: WI-012-1, WI-012-2, WI-012-3 (§11); the inventory of every other row that names Phase 012 (§12) | §11, §12 |

Out of scope, as for the carry-over plan: any implementation; an S3 storage adapter; token issuance; GraphQL; frontend flags; the package ecosystem; a merge of anything; publication; deployment of any site (a separate authorization, §10.8).

---

## 2. Findings — now dated history

### 2.1 F-1 — the repository's "stable" CI legs compiled with the pin (2026-08-11 → 2026-09-06; fixed)

**What was wrong, and for how long.** `rust-toolchain.toml` pinned `channel = "1.94.0"` from `98a4e2c` (2026-08-11). `ci.yml` installed the matrix toolchain with `dtolnay/rust-toolchain` (which runs `rustup toolchain install` and `rustup default` and sets no `RUSTUP_TOOLCHAIN`) and then ran plain `cargo xtask verify` in the checkout. rustup's documented precedence (§5.4) puts a directory's toolchain file above the default, so on every stable leg the proxies resolved to 1.94.0 and rustup installed it mid-job. The observed runs and the inferred window are in §0.4; the finding, its logs, and the local reproduction (`rustup show active-toolchain` in any checkout printing `1.94.0-… (overridden by '…/rust-toolchain.toml')`) are §14 of `phase-011-evidence.md`.

**What it did not mean.** Nothing in the tree was known to be wrong on stable — every local stable leg was green. The first genuine CI stable run then found two clippy 1.98 lints (§0.2), which is exactly the class of finding the fix exists to surface; both were fixed under the maintainer's named dispositions, not suppressed at crate or workspace level.

**What is fixed and what remains.** Selection, non-installation, and the identity proof are on `main` (AC-012-1…3). What remains is the generated-project half — the record and the census assertion (AC-012-4…6) — and the two dated notes (AC-012-7). D-L2-2 (§4) approves keeping the fix as it is and specifies only that remainder.

### 2.2 F-2 — rustup installs a pinned-but-absent toolchain from a listing command (unchanged fact)

Measured 2026-09-06 with rustup 1.29.0 (retained as `rustup-auto-install-probe.log`): with a directory pinned to an absent channel, `RUSTUP_AUTO_INSTALL=0 cargo --version` refuses by name and downloads nothing; a plain `rustup toolchain list` in the same directory **downloaded and installed the channel**. The version threshold is now confirmed against rustup's own changelog (D-L2-8, §4): automatic installation was removed in 1.28.0 (2025-03-04), restored as the default in **1.28.1 (2025-03-05)** "but can be opted out by setting `RUSTUP_AUTO_INSTALL` environment variable to `0`" (rust-lang/rustup pull requests #4214 and #4227; CHANGELOG entry `[1.28.1]`, <https://github.com/rust-lang/rustup/releases/tag/1.28.1>), and 1.28.2 added the lower-priority `rustup set auto-install disable`. Consequences: the seal forces the variable (FR-012-6), refuses rustup below 1.28.1 before any proxy runs (FR-012-7), and `doctor` lists nothing (§5.7).

### 2.3 Carried from the plan, unchanged

The generated Dockerfile's comment names a `rust-toolchain.toml` the project does not contain, and its builder image `rust:1.94.0-slim` is pinned independently of anything the tree declares (→ FR-012-9). `.renvor/generated.toml` has no toolchain field. `renvor doctor` probes `cargo` against the generator's own `CARGO_PKG_RUST_VERSION` only. `renvor generate auth` verifies its merged tree in a scratch copy under the **system temporary directory** (`tempfile::tempdir()` in `commands/generate.rs`), which is the case the selection rule of §5.4 exists for.

---

## 3. Three versions, distinguished

The row L-2 conflates three things that must be named apart. Each has one declaration, one enforcer, one record, and one owner. *(Corrected 2026-09-07: the 2026-09-06 table stated the inequality the wrong way round and claimed the pin equals the MSRV; D-L2-4 and D-L2-5 replace both.)*

| | **Minimum supported Rust version (MSRV)** | **Default pinned toolchain** | **Compiler actually used** |
|---|---|---|---|
| What it is | The oldest compiler the project promises to build with | The compiler rustup selects when nothing overrides it | The compiler that ran a specific verification |
| Declared in a generated project | `rust-version = "<msrv>"` in `Cargo.toml` (**new**, D-L2-1) | `rust-toolchain.toml`: `channel = "<pin>"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"` (**new**, D-L2-1) | not declared — **measured** and written to `.renvor/generated.toml` `[verified_with]` (**new**, D-L2-1) |
| Source of the value | starter: the framework checkout's `Cargo.toml` `[workspace.package] rust-version`, read independently of the pin (D-L2-5). Skeleton: the generator's `CARGO_PKG_RUST_VERSION` (D-L2-4) | starter: the framework checkout's `rust-toolchain.toml` `[toolchain] channel`, read independently of the MSRV (D-L2-5). Skeleton: the generator's MSRV, **as a chosen default** (D-L2-4) | the sealed evidence probe (§5.3), never a cache |
| Enforced by | cargo refuses an older compiler ("package requires rustc <msrv> or newer"); a newer one runs; `--ignore-rust-version` bypasses it explicitly | rustup, and only rustup: absent rustup, the file is inert | nothing; it is the truth about one run, and AC-012-5 compares it with what the leg intended |
| Who moves it | the framework, at an MSRV bump (`SUPPORT.md`) | the author, by editing the file; the generator, at the next template version | nobody; it changes when the environment changes, and the record shows that it did |
| The relation | **the pin is never below the MSRV; the MSRV may be below the pin.** A skeleton starts with the two equal because D-L2-4 chose that default, not because the two are defined equal | | may differ from both whenever an override is active or rustup is absent (§5.8) |

---

## 4. The corrected decision table (maintainer, 2026-09-07)

Every row is decided. "Corrections applied" lists what changed in the specification relative to the 2026-09-06 recommendation; "Specified in" is where the decision becomes requirements and tests.

### 4.1 L-2 — toolchain

| # | Decision | Disposition | Corrections applied | Specified in |
|---|---|---|---|---|
| **D-L2-0** | Contract version to target | **DECIDED 2026-09-06** (PR #63): C-4 1.3.0 provisional; compatibility assessment required | assessment corrected and kept provisional (§8) | §8 |
| **D-L2-1** | Mechanism | **DECIDED 2026-09-06**: `rust-toolchain.toml` **and** `rust-version`, with `verified_with` | — | §5 |
| **D-L2-2** | What the stable legs prove | **APPROVED.** Retain explicit job-level selection, no implicit installation, runtime identity checks, and the generated-artifact evidence from #64. Specify only the remaining generated-project proof | §5.9 lists only AC-012-4…7's remainder; nothing in `ci.yml` is re-specified | §0.3, §5.9 |
| **D-L2-3** | The seal's toolchain-shaping variables | **APPROVED WITH CORRECTIONS.** Keep `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS`, `RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`; remove `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT`; force `RUSTUP_AUTO_INSTALL=0` | applied to **every** tool invocation (§5.3 table), not only the five checks; the evidence mechanism no longer equates `rustc -vV` on `PATH` with Cargo's effective compiler (§5.3, with controls); release, commit, host recorded separately; only sanitized metadata and override *presence*; the seal is stated not to be a sandbox; "no provisioning" is distinguished from "no network", and the offline contract is preserved and tested (SR-012-5) | §5.2, §5.3 |
| **D-L2-4** | Whether the skeleton pins | **APPROVED.** The generator MSRV is the skeleton's default exact pin **and** its `rust-version` — a chosen default, not a universal equality rule | §3 and FR-012-1/2 no longer say "equals" | §5.1 |
| **D-L2-5** | Source of a starter's pin | **APPROVED WITH CORRECTIONS.** Read the MSRV from the framework manifest and the pin from the framework toolchain file **independently**; accept a supported exact pin ≥ MSRV; reject malformed, unsupported, and below-floor pins with named reasons; specify channel aliases and custom toolchains; derive the Docker builder version from the pin without downloading another compiler | the reversed inequality and every pin-equals-MSRV claim removed; refusal reasons named (FR-012-1); aliases refused, never resolved (FR-012-1); FR-012-9 rewritten | §5.1 |
| **D-L2-6** | Machines without rustup | **APPROVED.** Genuinely bare toolchains proceed with a notice when the effective compiler meets the MSRV and the tools are present; hiding `rustup` from `PATH` does not prove `cargo`/`rustc` are not proxies | proxy detection added (FR-012-7c) with the two controls that distinguish a bare toolchain from a hidden proxy | §5.3, §5.8 |
| **D-L2-7** | `doctor` and the pin | **APPROVED.** Reporting only; nothing changed or installed; the installed-toolchain report reconciled with the prohibition on unsafe listing probes | `doctor` never lists; it answers the operator's question with proxy probes under the seal (FR-012-11) | §5.7 |
| **D-L2-8** | Minimum rustup | **APPROVED.** rustup **1.28.1** is the minimum supported version for this policy, citing the release that introduced `RUSTUP_AUTO_INSTALL=0`; unsupported or unparseable versions are refused before any potentially installing proxy runs; a support-policy floor, not a claim about older releases | citation in §2.2; refusal ordering in FR-012-7a; `support-policy.md` change in §6.7 | §2.2, §5.3, §6.7 |
| **D-L2-9** | Record reader rule | **APPROVED WITH CORRECTIONS.** Explicit record-version dispatch: legacy records accepted without inventing evidence, known versions validated strictly, unsupported future versions refused by name before any file is modified | no claim that existing binaries gain the new behaviour; the old-reader/new-record incompatibility documented (FR-012-5c); "not published" is not "nobody has generated from source" (§8) | §5.2, §8 |
| **D-L2-10** | Legacy pin-less trees | **APPROVED.** Usable; no upgrade command this phase; no silent pin/MSRV insertion during `generate auth`; resource/migration operations do not become builds and manufacture no `verified_with`; earlier evidence retained explicitly as historical, never as proof of the modified tree | the toolchain template group is gated on the record (FR-012-10b), so a re-render of `Cargo.toml` cannot add `rust-version` to a legacy tree; the historical-evidence rule is mechanical (FR-012-5d) | §5.5 |
| **Selection rule** *(new)* | Resolution across staging and placement | **REQUIRED.** Existing-project verification must not silently lose the original project's effective selection when copied elsewhere; rustup's real precedence, including proximity, is respected; controls added | §5.4 with four controls; the scratch copy moves beside the project | §5.4 |

### 4.2 L-1 — TLS

| # | Decision | Disposition | Corrections applied | Specified in |
|---|---|---|---|---|
| **D-L1-1** | How the leg trusts its CA | **APPROVED WITH CONTROLS.** Absolute `SSL_CERT_FILE` to an ephemeral CA for the Linux proof; inherited `SSL_CERT_DIR` cleared; trust cases isolated in fresh processes/clients; trusted-CA success, well-formed unrelated-CA rejection, and the unset-CA control required; no OS trust-store discovery claimed; per-adapter `ca_file` deferred explicitly with an owner and target | SR-012-10…12 | §9.2, §9.9 |
| **D-L1-2** | Database TLS | **APPROVED AS A SEPARATE WORK ITEM.** Kept out of L-1's three-service wording; scoped and estimated in this plan as WI-012-5 (PostgreSQL and MySQL through SQLx and SeaORM, certificate and hostname verification); not postponed automatically; any deferral needs an explicit disposition | WI-012-5 in §11 with scope, estimate, and the consequence of deferral | §11 |
| **D-L1-3** | HTTPS OTLP receiver | **APPROVED SUBJECT TO PACKAGE REVIEW.** The crate's loopback receiver extended with `tokio-rustls` as a test-only dependency; ring-only provider preserved | package review recorded in §9.5 | §9.5 |
| **D-L1-4** | mTLS | **APPROVED.** Outside this proof; the exclusion is stated | §9.9 | §9.9 |
| **D-L1-5** | Certificate generation | **APPROVED.** The runner's OpenSSL CLI, version recorded; short-lived CA and leaves with correct SAN/EKU; private keys protected, never committed or uploaded | §9.3 | §9.3 |
| **D-L1-6** | macOS/Windows root stores | **APPROVED.** Linux file-CA evidence satisfies this bounded closure; OS trust-store discovery and macOS/Windows TLS behaviour stay explicitly unproven | §9.9 | §9.9 |
| **D-L1-7** | TLS version | **APPROVED WITH CORRECTION.** TLS 1.3 preferred; TLS 1.2 the minimum permitted baseline; the actual observation point identified per connection; a 1.2 negotiation explained; never inferred from crate features | §9.6 | §9.6 |

---

## 5. Specification — the toolchain declaration, the record, and the sealed resolution

### 5.1 The declaration (FR-012-1 … FR-012-3, FR-012-9)

| # | Requirement |
|---|---|
| **FR-012-1** | **Every generated tree carries `rust-toolchain.toml`** at its root: `channel = "<pin>"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`. **Starter**: `<pin>` is read at generation from `<framework-path>/rust-toolchain.toml` `[toolchain].channel`, and `<msrv>` from `<framework-path>/Cargo.toml` `[workspace.package].rust-version`, **each parsed on its own**; the two are then compared. Accepted: an **exact release** `X.Y.Z` with `X.Y.Z ≥ <msrv>`. Refused, before anything is staged, as a validation failure of the framework checkout (exit 3, C-1's `framework_directory` rule family, §6.3) with a named reason: `toolchain_pin_malformed` (not `X.Y.Z`; the file unreadable or missing the key), `toolchain_pin_unsupported` (a channel alias — `stable`, `beta`, `nightly`, a dated `nightly-YYYY-MM-DD` — a custom toolchain name, or a `path`: none of these is resolved to a version, silently or otherwise), `toolchain_pin_below_msrv` (`X.Y.Z < <msrv>`), `msrv_unreadable` (the manifest key absent or not `X.Y.Z`). A checkout that is refused is the framework's inconsistency, and the message says which file. **Skeleton**: `<pin>` and `<msrv>` are both the generator's `CARGO_PKG_RUST_VERSION` (D-L2-4). The file is rendered, digested, and listed in the record like every generated file; the template version moves to **8** (C-4 snapshot policy). |
| **FR-012-2** | **Every generated `Cargo.toml` carries `rust-version = "<msrv>"`** — the MSRV, not the pin. `edition = "2024"` is unchanged. |
| **FR-012-3** | The generated `README.md` states the pin and the MSRV as two numbers, why the pin exists (L-2's consequence: a tree verified on one compiler may fail `clippy -D warnings` on another), how to change it (edit the file, run `cargo clippy --all-targets -- -D warnings` and `cargo test`), that the pin is honoured only by rustup ≥ 1.28.1 and is inert without rustup while `rust-version` still refuses older compilers, and — for a legacy tree — the two files to add by hand (§5.5). |
| **FR-012-9** | The generated Dockerfile's builder tag is **derived from the pin** (`FROM docker.io/library/rust:<pin>-slim`), its comment names the file that now exists, and the builder stage sets `ENV RUSTUP_AUTO_INSTALL=0`. A pin whose image tag does not exist fails the image pull by name; a pin that outruns the image's installed toolchain fails the build by name; **neither downloads another compiler** inside the build. Snapshot test: the tag and the rendered channel are asserted equal. |

### 5.2 The record (FR-012-4, FR-012-5)

`.renvor/generated.toml` is written **after** verification and before the manifest, where the digests are written today (C-4 1.2.0). Version 2:

```toml
record_version = 2               # absent in every record written before this revision (FR-012-5b)
generator_version = "0.0.0"
template_version = "8"

[toolchain]
pinned = "1.94.0"                # the channel rendered into rust-toolchain.toml; "none" for a legacy tree (§5.5)
rust_version = "1.94.0"          # the rust-version rendered into Cargo.toml; "none" for a legacy tree

[verified_with]
operation = "new"                # new | auth — the operation whose five checks this table describes
manifest_digest = "sha256:…"     # over the sorted `[[file]]` entries as written at that verification (FR-012-5d)
rustc_release = "1.98.1"         # three fields, never one string (D-L2-3)
rustc_commit = "48a229cea"
rustc_host = "x86_64-unknown-linux-gnu"
cargo_release = "1.98.1"
cargo_commit = "…"
rustup = "1.29.0"                # or "absent"
proxy = true                     # the resolved rustc is a rustup proxy (FR-012-7c)
selected_by = "environment"      # environment | directory_override | toolchain_file | default | no_rustup | unknown
rustc_override = false           # RUSTC in the sealed environment, or Cargo's `build.rustc` (presence only)
wrapper = false                  # RUSTC_WRAPPER or RUSTC_WORKSPACE_WRAPPER present
rustflags = false                # RUSTFLAGS present
rustdocflags = false             # RUSTDOCFLAGS present
```

| # | Requirement |
|---|---|
| **FR-012-4** | `[verified_with]` is **measured, never derived**: the sealed evidence probe (§5.3, FR-012-7d) runs inside the staged (or scratch) tree with the same sealed environment the five checks receive, after they pass; the values are what that probe reports. A record written before verification, from the generator's own process environment, or from any cache (`.rustc_info.json`, a previous record) is a test failure. |
| **FR-012-5a** | **Which operations write it.** `renvor new` (real run; a dry run writes nothing) and `renvor generate auth` (its scratch verification) — the two operations that run the five checks — write `[verified_with]` with their `operation`. `renvor generate resource` and `renvor generate migration` run no build; they **leave `[verified_with]` byte-identical** and rewrite only the `[[file]]`/`[[resource]]` entries they own. |
| **FR-012-5b** | **Reader dispatch (D-L2-9).** A reader reads `record_version` first: **absent** → the record is a legacy (version 1) record: accepted; `[toolchain]` and `[verified_with]` are reported as *unknown*, never filled in; **2** → the whole document is validated strictly (`deny_unknown_fields` within the version); **greater than the reader knows** → refused by name — `record_unsupported`, `details.record_version`, `details.supported = 2` — **before any file is planned or modified** (exit code: U-1 in the task plan; recommended 3). |
| **FR-012-5c** | **The incompatibility the other way is documented, not claimed away.** A `renvor` built from source at or before `7281e4f` reads a version-2 record through `#[serde(deny_unknown_fields)]` and fails with serde's unknown-field error inside the existing read failure — a generic parse error, **not** `record_unsupported`, because those binaries do not have this rule. Nothing has been published, but people may have generated projects from source; the README of a new project and C-4 1.3.0's revision text both say: *a project generated at template version 8 or later is not readable by a generator built before this revision; rebuild the generator, not the project.* |
| **FR-012-5d** | **Evidence freshness (D-L2-10).** `manifest_digest` is SHA-256 over the sorted `path\0digest\n` lines of the `[[file]]` entries as written at the verification. `renvor check` recomputes it over the current `[[file]]` entries: equal → `verified_with` describes this tree; different → `renvor check` prints `verified_with: historical — <n> generate operations since; not proof of the current tree`, and `--output json` carries `data.verified_with.historical = true`. Earlier evidence is **retained**, never deleted and never re-dated. |
| **FR-012-5e** | The record's digest stays outside the snapshot's pinned set (its path is pinned); `[toolchain]` values are rendered and therefore part of the snapshot; `[verified_with]` values differ by machine and are not. `renvor check` prints both tables; `renvor new --output json` and `renvor generate … --output json` carry them under `data.toolchain` and `data.verified_with` (additive under C-2, §8). |

### 5.3 The sealed environment, the resolution probe, and the evidence probe (FR-012-6 … FR-012-8, SR-012-1 … SR-012-5)

**What "sealed" means after D-L2-3.** The allow-list `PASSED_THROUGH` keeps `PATH`, `HOME`, the locale and temp variables, `CARGO_HOME`, `RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`, `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS`, the `CARGO_*` build and network variables, `SSL_CERT_FILE`, `SSL_CERT_DIR`, the proxy variables (credential stripped), and the Windows process variables; it **loses** `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT`; and it **forces** `RUSTUP_AUTO_INSTALL=0` whether or not the caller set it. *The seal is not a sandbox*: a wrapper, a `RUSTC` override, `RUSTFLAGS`, and every build script of every dependency still run with the operator's rights inside the "sealed" step. What the seal guarantees is narrower and stated exactly — no secret of the operator's shell reaches the child (C-5 1.1.0), no toolchain is provisioned (SR-012-1), and what ran is recorded (FR-012-4).

| # | Requirement |
|---|---|
| **FR-012-6** | The sealed environment sets `RUSTUP_AUTO_INSTALL=0` **unconditionally** and omits `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` for every child it spawns — the five checks, the resolution probe, the evidence probe, `rustfmt`, and `doctor`'s probes. The three existing seal tests are unchanged; a fourth asserts the forced value and the two absences. |
| **FR-012-7a** | **Order of the preflight, before anything is staged and before any proxy runs in a pinned directory:** (1) `rustup --version` if `rustup` is on `PATH`: an unparseable version, or one **below 1.28.1**, is `tool_missing`, exit 5, `details.tool = "rustup >= 1.28.1"`, and no proxy is invoked (asserted by a test whose `PATH` has no `cargo`); (2) only then the resolution probe. If `rustup` is not on `PATH`, step (1) is skipped and FR-012-7c decides whether the proxies are nevertheless rustup's. |
| **FR-012-7b** | **The resolution probe** is `rustc -vV` and `cargo -vV`, run inside the directory whose selection is being measured (§5.4), under the seal, followed by `rustup show active-toolchain` when rustup is available. rustup's "is not installed" text is the refusal **`tool_missing`, exit 5**, `details.tool = "rustup toolchain <channel>"`, `details.remedy = "rustup toolchain install <channel> --component rustfmt --component clippy --profile minimal"`, before any check runs and before anything is placed. A resolved compiler older than `rust_version` is the same refusal with `details.tool = "rustc >= <msrv>"` (cargo would refuse later; the generator refuses first, by name). A toolchain that resolves but lacks `rustfmt` or `clippy` (`rustfmt --version`, `cargo clippy --version` under the same seal) is `tool_missing` naming the component and the `rustup component add` remedy. `selected_by` is taken from `rustup show active-toolchain`'s attribution text (the strings for the rustup versions named in `support-policy.md` are pinned by a test; unknown text records `unknown`, never a guess); the path rustup prints is not recorded. |
| **FR-012-7c** | **Proxy detection (D-L2-6).** Hiding `rustup` from `PATH` proves nothing about `rustc` and `cargo`, which may still be rustup's proxies (hard links to the rustup binary that read `RUSTUP_HOME` themselves). The probe therefore runs the resolved `rustc` once more with `RUSTUP_TOOLCHAIN=<a name that cannot be installed>` (the identity script's pattern, `RUSTUP_AUTO_INSTALL=0`, dist variables absent): a proxy fails with "is not installed"; a bare compiler ignores the variable and prints its version. The result is `proxy = true/false`. `selected_by = "no_rustup"` is recorded **only** when `proxy = false`; a proxy with `rustup` off `PATH` is attributed by the file/override/default it actually used, and the notice says so. |
| **FR-012-7d** | **The evidence probe (D-L2-3): Cargo's effective compiler, not `PATH`'s.** After the five checks pass, the seal renders a minimal probe crate inside the staged tree (`.renvor/probe/`: its own `[workspace]` table so it is its own root; no dependencies; a `build.rs` that reads the `RUSTC`, `RUSTC_WRAPPER`, and `RUSTC_WORKSPACE_WRAPPER` Cargo hands to build scripts, runs that `RUSTC -vV`, and writes the output to `OUT_DIR`; a `main.rs` that prints it) and runs `cargo run --quiet` on it under the same sealed environment and target directory. What it prints is what Cargo compiled the project with — after `build.rustc` in any Cargo configuration the sealed `CARGO_HOME` carries, after a `RUSTC` override, and independent of which `rustc` `PATH` resolves. The probe directory is removed before the manifest is taken and never placed. Cargo's own identity is `cargo -vV` from the seal. Cargo's `.rustc_info.json` is **not** read: it is a cache. **Controls**: (i) `RUSTC` in the sealed environment pointing at another installed toolchain's `rustc` binary while `PATH` resolves the pin — the record names the `RUSTC` one and `rustc_override = true`; (ii) a `[build] rustc = …` in the sealed `CARGO_HOME`'s `config.toml` — the same; (iii) a pass-through `RUSTC_WRAPPER` — the underlying identity unchanged, `wrapper = true`; (iv) the trivial case — `PATH`'s `rustc -vV` and the probe agree. |
| **FR-012-7e** | **Sanitized metadata only.** Each probe line is parsed under a strict grammar before it enters the record or an operator stream: `release` `^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$`, `commit-hash` `^[0-9a-f]{7,40}$` or `unknown`, `host` `^[A-Za-z0-9_.-]{1,64}$`. Output that does not match is `project_verification_failed` with `details.reason = compiler_identity_unreadable`, redacted per C-5 1.1.0. Override **presence** is recorded as booleans; **no** flag value, wrapper path, proxy credential, or raw child output enters the record, the JSON, or a human stream. |
| **FR-012-8** | When the resolved compiler is not the pin (an override is active, or rustup is absent), the run **proceeds** — the operator or CI chose it — and prints one line to stderr: `verifying with rustc <release> (<selected_by>); the project pins <channel>` (for a legacy tree: `the project pins nothing`). The record carries both. Nothing is silent; nothing is refused for being different. |
| **SR-012-1** | **No toolchain provisioning, ever, during generation or verification**: FR-012-6 plus FR-012-7a. Control: a test pins an absent channel and points every download at an unroutable address (`RUSTUP_DIST_SERVER=http://127.0.0.1:9` in the **test's** process environment, which the seal no longer forwards — so the control also proves the seal does not need it): the result is `tool_missing`, the probe completes in bounded time, and the toolchain directory under `RUSTUP_HOME` afterwards contains nothing new. |
| **SR-012-2** | **No silent fallback**: the compiler that ran is always in the record and in `--output json`; a difference from the pin is always printed (FR-012-8); a missing toolchain is never resolved to "whatever the default is". |
| **SR-012-3** | `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS` stay in the seal — the operator's own trust, the intended uses being build caches and cross-compilation — and their **presence** is recorded (§5.2). The residual (a wrapper runs inside the sealed step with the operator's rights) is a ledger row in `phase-012-limitations.md`, not a refusal: refusing wrappers would break every cache-using machine. |
| **SR-012-4** | The generator never runs a rustup command that lists, installs, updates, or sets a default; it runs proxies (`rustc`, `cargo`, `rustfmt`, `cargo clippy`), `rustup --version`, and `rustup show active-toolchain` only, all under FR-012-6. |
| **SR-012-5** | **No provisioning is not no network.** `RUSTUP_AUTO_INSTALL=0` stops rustup from installing a toolchain; it does not stop Cargo from fetching crates. Dependency downloads are governed by FR-006 (the seeded lockfile) and by `CARGO_NET_OFFLINE`, exactly as today, and that contract is **preserved and tested**: `tests/offline.rs` keeps its existing case (an empty `CARGO_HOME`, the lockfile closure fetched, `CARGO_NET_OFFLINE=true`, a starter generated) and gains two: the same with the pin installed → pass, `verified_with` names the pin; the same with the pin **absent** → `tool_missing` before any check, with no toolchain download and no crate fetch attempted. |

**Every tool invocation, and what D-L2-3 requires of each** (the sites are the nine `Command::new` calls in `crates/renvor-cli/src` at `7281e4f`):

| Site | Today | Required |
|---|---|---|
| `generate/verify.rs` `in_staging_with` — the five checks for `renvor new` and for `generate auth`'s scratch copy | sealed (allow-list) | the seal of FR-012-6 plus FR-012-7a/b/c before the checks and FR-012-7d after them |
| `commands/generate.rs` `rustfmt` — `generate resource` | **inherits the process environment** | run under the seal of FR-012-6 (a `rustfmt` that is a rustup proxy in a pinned directory can install today) |
| `commands/doctor.rs` `probe` — `--version` of every tool | inherits | run under the seal of FR-012-6; the pin probes of §5.7 |
| the preflight resolution probe (new) | — | FR-012-7a/b/c |
| `commands/dev.rs` `cargo test` — the operator's development loop in the operator's project | inherits, by design (the application needs its `RENVOR_*` environment) | **not sealed** — but `RUSTUP_AUTO_INSTALL=0` forced and the two install-server variables removed, so `renvor dev` never provisions a toolchain either |
| `commands/routes.rs` — the `cargo` warm build for the relay | inherits | as `dev` |
| `commands/relay.rs` — the application binary; `commands/docker.rs` — `docker` | not toolchain invocations | unchanged |
| the generated Dockerfile's builder stage | image environment | `ENV RUSTUP_AUTO_INSTALL=0` (FR-012-9) |

### 5.4 Selection across staging and placement (the additional selection rule)

rustup's documented order, quoted from its user guide (`doc/user-guide/src/overrides.md`): *"1. A toolchain override shorthand used on the command-line, such as `cargo +beta`. 2. The `RUSTUP_TOOLCHAIN` environment variable. 3. A directory override, set with the `rustup override` command. 4. The `rust-toolchain.toml` file. 5. The default toolchain. The toolchain is chosen in the order listed above, using the first one that is specified. There is one exception though: directory overrides and the `rust-toolchain.toml` file are also preferred by their proximity to the current directory. That is, these two override methods are discovered by walking up the directory tree toward the filesystem root, and a `rust-toolchain.toml` file that is closer to the current directory will be preferred over a directory override that is further away."*

| # | Requirement |
|---|---|
| **FR-012-12** | **`renvor new`**: staging is created inside the destination's parent (C-5), so the staged tree and the placed tree share every ancestor; the resolution measured in staging is the resolution at the destination, by construction. The probe runs in the staging directory. |
| **FR-012-13** | **`renvor generate auth`**: the scratch copy moves from the system temporary directory to a sibling of the project — `<parent>/.renvor-staging-<pid>-…`, C-5's residue naming, removed on completion — so the copy shares the project's ancestors and therefore its directory overrides and ancestor toolchain files. The resolution probe runs **first in the project directory**, then in the scratch copy; the two identities must be equal, or the run is `project_verification_failed` with `details.reason = toolchain_resolution_diverged` and nothing is written. The record's `selected_by` is the project directory's attribution. |
| **FR-012-14** | **`renvor generate resource`**'s `rustfmt` runs in the project directory itself. `renvor doctor` reports for the current directory. |
| **Controls** | **C-sel-1** — pin `X` in the project, `rustup override set Y` on the **parent**: the project's file is closer, so `X`, `toolchain_file`, no notice. **C-sel-2** — the override set on the **project directory itself** with pin `X`: rustup's order puts the override first, so `Y`, `directory_override`, the FR-012-8 notice. **C-sel-3** — a legacy pin-less project under an ancestor with `rust-toolchain.toml` `Z`: `generate auth` resolves `Z` in the project directory and in the sibling scratch copy alike (positive); the pre-change behaviour — a copy under the system temporary directory — resolves the default instead, and the divergence check fails it (negative control, kept as a test of the check itself). **C-sel-4** — `RUSTUP_TOOLCHAIN=Y` with pin `X`: `Y`, `environment` (this is AC-012-6). Each control needs two installed toolchains and is skipped with a `SKIPPED:` line locally and required under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` in CI (the census pattern). |

### 5.5 Legacy trees and `renvor generate` (FR-012-10, D-L2-10)

| # | Requirement |
|---|---|
| **FR-012-10a** | A project generated at template version 7 or earlier has no pin and a record without `record_version`. `renvor generate` (resource, migration, or auth) into such a tree **does not refuse** on that account: it verifies with whatever resolves (recording `verified_with` as for any run, `pinned = "none"`), prints the FR-012-8 line with `the project pins nothing`, and states once that no `renvor generate toolchain` action exists and that the README of a new project shows the two files to add by hand. No upgrade command is added this phase; L-11 ("nothing upgrades a project with it") stays the row that names the gap. |
| **FR-012-10b** | **No silent insertion.** The toolchain declaration is a template **group** (`toolchain`: `rust-toolchain.toml`, and the `rust-version` line of `Cargo.toml`) that renders when `renvor new` runs or when the record's `[toolchain]` declares a pin — and **not** when the record is legacy. So `generate auth`'s re-render of `Cargo.toml` on a legacy tree renders it without `rust-version`, and plans no `rust-toolchain.toml`; a control asserts both. |
| **FR-012-10c** | Verification scope is unchanged: `generate resource` and `generate migration` run `rustfmt` or nothing; they never build, and FR-012-5a forbids them to write `verified_with`. FR-012-5d makes the earlier evidence historical the moment the manifest changes. |

### 5.6 `renvor check`

`renvor check` reads the record through FR-012-5b, prints `[toolchain]` and `[verified_with]` (or *unknown* for a legacy record), applies FR-012-5d, and reads `rust-toolchain.toml` if present to report whether the file's channel still equals `[toolchain].pinned` (an author edit is reported, never refused).

### 5.7 `renvor doctor` (FR-012-11, D-L2-7)

`doctor` reports and changes nothing. In a directory with a project (`renvor.toml`, or `rust-toolchain.toml`, or both):

| Reported | How, without listing |
|---|---|
| the pin from `rust-toolchain.toml` and, from the record, `pinned`, `rust_version`, and `verified_with` (or *unknown*) | read |
| rustup version, and whether it is at or above the 1.28.1 floor | `rustup --version`, parsed; below the floor, the pin probes below are **not run** and the row says `not probed: rustup <v> is below the supported floor` |
| what resolves **here** and why | `rustc -vV`, `cargo -vV`, `rustup show active-toolchain`, under the seal of FR-012-6 |
| whether the pin is installed with both components | `rustc +<pin> -vV`, `rustfmt +<pin> --version`, `cargo +<pin> clippy --version` under the seal — rustup ≥ 1.28.1 with `RUSTUP_AUTO_INSTALL=0` answers "is not installed" by name and downloads nothing; the three lines report present/absent |
| whether the resolved compiler is a proxy | FR-012-7c |
| orphaned staging directories | as today |

`doctor` **never** runs `rustup toolchain list` or any listing, installing, updating, or default-setting command (SR-012-4); this is the reconciliation D-L2-7 asked for — the operator's question ("is the pin usable here, and what will run?") is answered by proxy probes that cannot install, and the "installed toolchains" table of the 2026-09-06 draft is withdrawn. Outside a project, the section is omitted and the JSON carries `toolchain: null`. Exit codes are unchanged (0; 5 for a required tool absent).

### 5.8 What the operator sees — the explicit behaviour table

Nothing in this table falls back silently, downloads, or refuses for a difference the operator chose.

| Situation | Selects the compiler | `renvor new` / `generate` | Exit | Recorded (`selected_by`, `pinned`, `rustc_release`) | Operator sees |
|---|---|---|---|---|---|
| Pin present and installed, no override (the clean machine) | the file | verifies with the pin | 0 | `toolchain_file`, pin, pin | nothing extra |
| `RUSTUP_TOOLCHAIN=Y` (also how `cargo +Y` reaches children; the CI legs) | the environment | verifies with `Y` | 0 | `environment`, pin, `Y` | `verifying with rustc Y (environment); the project pins X` |
| `rustup override set Y` on the project directory | the directory override (rustup's order) | verifies with `Y` | 0 | `directory_override`, pin, `Y` | the same line, `(directory override)` |
| `rustup override set Y` on an ancestor, pin in the project | the file (proximity) | verifies with the pin | 0 | `toolchain_file`, pin, pin | nothing extra |
| Pin not installed, rustup ≥ 1.28.1 | nothing — the probe fails by name | **refused before any check**, nothing staged | 5 `tool_missing`, `details.tool = "rustup toolchain X"`, the install command | no record | the refusal and the exact `rustup toolchain install …` line; **no download** |
| rustup below 1.28.1, or unparseable | — | **refused before the probe** | 5 `tool_missing`, `details.tool = "rustup >= 1.28.1"` | none | the rustup version and the floor; no proxy ran |
| Installed pin lacks `rustfmt` or `clippy` | the file | refused at the probe | 5 `tool_missing` naming the component | none | the `rustup component add` remedy |
| `rustup` off `PATH`, but `rustc`/`cargo` are still proxies | whatever the proxies resolve (file, override, default) | verifies; attributed correctly | 0 | the real attribution, `proxy = true` | the FR-012-8 line if the result is not the pin |
| **Genuinely bare** toolchain (a distribution compiler, an image with a bare toolchain), `rustc ≥ MSRV`, `rustfmt` and `clippy` present | `PATH` | verifies; the pin file is inert; `rust-version` still refuses older compilers | 0 | `no_rustup`, pin, what ran; `proxy = false` | `verifying with rustc Z (no rustup); the project pins X, which cannot be selected here` |
| Bare toolchain, `rustc < MSRV`, or a tool absent | `PATH` | refused at the probe | 5 `tool_missing` | none | the refusal; the README's install pointer |
| `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS` set | as above, then the override/wrapper/flags act inside Cargo | verifies (SR-012-3) | 0 | as above, plus the presence booleans | nothing extra (the record says it) |
| Docker build of the generated project | the image's toolchain | not a generator path; `ENV RUSTUP_AUTO_INSTALL=0` in the builder | — | — | a pin the image lacks fails the build by name |
| `renvor generate` into a legacy tree (template ≤ 7) | whatever resolves | proceeds; `pinned = "none"`; no pin inserted | 0 | as the row above | `the project pins nothing` |
| A record with `record_version` newer than the reader | — | refused before any plan | U-1 (`record_unsupported`) | — | the versions, and "rebuild the generator" |
| `renvor doctor` in a pinned directory | — | reports (§5.7) | 0 / 5 as today | — | the table |

### 5.9 The stable compatibility leg — the remainder

AC-012-1 … AC-012-3 are met at `7281e4f` (§0.3) and are **not re-specified**. What D-L2-2 leaves to this phase:

| # | Requirement |
|---|---|
| **AC-012-4** | Step 1 of `contracts/verification-sequence.md` records the compiler that runs the sequence — `rustc -vV` release, commit, host and `cargo -vV`, read from inside the checkout — in `cargo xtask verify`'s own output, so a local leg and a CI leg are recorded the same way. `verification-sequence.md` 2.4.0 (§6.5). |
| **AC-012-5** | The census (`starter_matrix.rs`) asserts, for every row on every leg, that the placed record's `[verified_with].rustc_release` and `rustc_commit` equal the gate's own `rustc -vV` — the compiler that verified a generated tree is the compiler the leg claims. On the stable leg this proves the generated pin did **not** win (the job-level variable did); on the MSRV leg it proves the pin and the leg agree. |
| **AC-012-6** | The unit-level control of §5.4 C-sel-4 and C-sel-1/2. |
| **AC-012-7** (remainder) | `SUPPORT.md` ("only the stable job moves") and `rust-toolchain.toml`'s comment ("CI additionally runs current stable") each gain a dated sentence: *true since `7281e4f` (2026-09-06); between 2026-08-11 and that commit the stable contexts compiled the pinned MSRV — `phase-011-evidence.md` §14.* No sentence is deleted. |

---

## 6. Contract-change proposals (drafted here, applied by the implementation batch, never silently)

These are the proposed revision texts. Each becomes a contract edit only in the implementation pull request that carries the behaviour, with the version confirmed by §8's assessment at that moment. Nothing in `contracts/` changes in this planning pull request.

### 6.1 C-4 `template-contract.md` → 1.3.0 (provisional)

Status line addition: *1.3.0 (Phase 012, L-2): every generated tree declares its toolchain — `rust-toolchain.toml` (an exact release, the framework checkout's channel for a starter, the generator's MSRV for a skeleton) and `rust-version` (the MSRV) — as a template group gated on the record; the provenance record gains `record_version = 2`, `[toolchain]`, and `[verified_with]` (measured after verification, never derived); readers dispatch on `record_version`; the generator now reads THREE files from the framework checkout it validates (`Cargo.toml`, `rust-toolchain.toml`) and one it copies (`Cargo.lock`); a checkout whose pin is malformed, an alias, or below its own MSRV is refused by name; the Dockerfile builder tag derives from the pin. A project rendered at template version 8 is not readable by a generator built before this revision.*

Body changes, section by section: **Verbatim files** — the sentence "reads **nothing** from the framework checkout … except two files it validates … and one it copies" becomes "three files it validates (`Cargo.toml`, `rust-toolchain.toml` — both parsed, neither evaluated) and one it copies"; **Starter sets** — a new group row `toolchain | at renvor new, or when the record declares a pin | rust-toolchain.toml, and the rust-version line of Cargo.toml`; **Generated-on-demand files** — `rustfmt` runs under the sealed environment of C-5 1.2.0; **Snapshot stability policy** — `[toolchain]` joins the pinned set, `[verified_with]` joins `Cargo.lock` outside it, the record's path stays pinned; **The provenance record** — the version-2 layout of §5.2, the reader rule of FR-012-5b/c, the freshness rule of FR-012-5d, and the write rule of FR-012-5a.

### 6.2 C-5 `generation-transaction.md` → 1.2.0 (provisional)

Status line addition: *1.2.0 (Phase 012, L-2): the sealed environment forces `RUSTUP_AUTO_INSTALL=0`, no longer passes `RUSTUP_DIST_SERVER` or `RUSTUP_UPDATE_ROOT`, refuses rustup below 1.28.1 and a pinned-but-absent toolchain by name before step 2, resolves and records the compiler Cargo actually uses (release, commit, host; override presence only), and applies the same seal to `rustfmt` at generation and to `doctor`'s probes; `generate auth` stages its scratch copy beside the project so the project's toolchain selection is preserved. The protocol, atomicity, and residue rules are unchanged; the seal is not a sandbox for trusted wrappers or build scripts.* Body: "What 'verify before placing' means" gains the resolution and evidence probes and the not-a-sandbox sentence; "Residue" gains the sibling scratch copy's name.

### 6.3 C-1 `command-surface.md` → 1.5.0 (provisional)

Additions, no exit code or stream rule changed: the `tool_missing` details of FR-012-7a/b (`details.tool` values `rustup >= 1.28.1`, `rustup toolchain <channel>`, `rustc >= <msrv>`, a component name; `details.remedy`); the `--framework-path` validation reasons of FR-012-1 (`toolchain_pin_malformed`, `toolchain_pin_unsupported`, `toolchain_pin_below_msrv`, `msrv_unreadable`); `project_verification_failed` reasons `compiler_identity_unreadable` and `toolchain_resolution_diverged`; the new refusal `record_unsupported` (exit U-1); the FR-012-8 stderr notice (one line, stderr, never stdout); `renvor check`'s two tables and the `historical` marker; `renvor doctor`'s toolchain section.

### 6.4 C-2 `json-output.md` — additive, no `schemaVersion` bump

`data.toolchain`, `data.verified_with` (with `historical`), and `data.doctor.toolchain` are additive fields; `record_unsupported` joins the error-code registry as an addition. C-2's own rule: adding is additive, removing is breaking. **The TOML record's `record_version` is a different axis** — the format version of a file the generator owns, governed by C-4 — and is not a `schemaVersion` change.

### 6.5 `verification-sequence.md` → 2.4.0 (provisional)

Step 1 records the compiler (AC-012-4); step 4's census carries the AC-012-5 assertion; the census count moves with the new rows (the `tls` row of §9.7, the seeded no-auth row of WI-012-1, and the controls of §5.4 when they run under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1`).

### 6.6 C-C7 `capabilities-contract.md` → 1.1.1 (provisional)

One sentence naming the TLS leg (§9) as C-C7's proof of the acceptance half, with the head and run it was first proven on — added only when that run exists (AC-L1-6).

### 6.7 `support-policy.md` → 1.2.0 (provisional) and `SUPPORT.md`

A new row: *rustup: **1.28.1 or later** is required to generate or verify a pinned project (the release that introduced `RUSTUP_AUTO_INSTALL`); older or unparseable versions are refused by name; this is a support floor for the generator's guarantee, not a statement about how older releases behave.* Because a support promise changes, the row is carried by a **proposed** decision record (ADR-0038, §6.8) per `SUPPORT.md`'s own rule; the AC-012-7 dated sentence is added at the same time.

### 6.8 Decision records — proposed, not accepted

| ADR (next free numbers) | Subject | Why an ADR |
|---|---|---|
| **ADR-0038** (proposed) | The generated project's toolchain declaration, the record's `verified_with`, the seal's provisioning ban, and the rustup 1.28.1 floor | a lasting public and operational commitment (constitution XII); a support-policy change (`SUPPORT.md`) |
| **ADR-0039** (proposed) | The TLS proof leg: trust through `SSL_CERT_FILE`, the ephemeral CA, the test-only `tokio-rustls` receiver, and the explicit exclusions (mTLS, OS store discovery, per-adapter `ca_file` deferred) | a lasting verification commitment and a dependency decision (constitution III) |
| **ADR-0040** (proposed) | Documentation versioning and the framework → documentation artifact handoff (§10.6) | a lasting cross-repository commitment under `PLAN.md` §26.7/§26.8 |

This pull request proposes none of them; each is drafted in the batch that carries its behaviour, reviewed under the single-maintainer waiver pattern, and accepted only by the maintainer.

---

## 7. Cross-reference note

The 2026-09-06 draft numbered the explicit behaviour table §7 and the compatibility assessment §8; the carry-over plan cites "brief §5–§8". In this revision the behaviour table is §5.8 and the assessment keeps its number, §8, so those citations still land.

---

## 8. Compatibility assessment — corrected, and kept provisional

D-L2-0's condition: a minor version is not forced if the change requires a major revision under the contract's own versioning. The 2026-09-06 assessment called every change additive; that was too quick. Assessed against the proposed texts of §6, with the contracts' own precedent (C-1 1.2.0–1.4.0 added flags, refusals, and classification rows as minors; C-4 1.1.0–1.2.0 added file groups, the record, and the snapshot policy as minors; none removed or contradicted a rule).

| Item | What changes | Kind | Reasoning |
|---|---|---|---|
| **C-4's read bound** | "reads nothing … except two files it validates and one it copies" → three validated, one copied | **a bound widens** — not purely additive | The generator reads (parses, never evaluates) one more file from a checkout it already validates. Precedent: 1.1.0 widened the read set from zero to three files as a minor. The widening is stated in the revision text rather than absorbed |
| **New refusals of previously accepted input** | a `--framework-path` checkout pinned to an alias, a malformed channel, or a channel below its own MSRV is now refused (FR-012-1) | **a narrowing** of accepted input for starters | Before, the generator never read the file, so such a checkout was accepted. The framework's own checkout always carries an exact pin; a checkout that does not is outside what C-1's `--framework-path` rule validates ("the Renvor workspace"). Recorded as a narrowing; recommended minor because no supported input is refused |
| **Legacy projects** | template ≤ 7 trees keep working through every `generate` action (FR-012-10); no pin is inserted | additive | the D-L2-10 rule is what keeps this a minor: refusing pin-less trees would force 2.0.0 |
| **Old reader / new record** | a generator built from source before this revision cannot read a version-2 record (FR-012-5c) | **breaking for unpublished readers** | Nothing has been published, so no released reader exists; but "not published" is not "nobody has generated from source" — the maintainer's own machine and the cheat sheet have. The contract's status text says it is "public from the first release that ships it", so the promise has no released consumer yet. **Recommended**: 1.3.0 with the incompatibility stated in the revision text and in every new project's README; **the alternative the maintainer may take**: 2.0.0, if source-built readers are inside the promise. This is the item that keeps the number provisional |
| **Verification scope and evidence freshness** | resource/migration operations write no `verified_with`; the freshness marker (FR-012-5d) | additive | no operation gains or loses a build |
| **JSON additions** | `data.toolchain`, `data.verified_with`, a new error code | additive under C-2's own rule | no `schemaVersion` bump |
| **TOML `record_version`** | a new top-level key; readers dispatch on it | a format version change of a generator-owned file | governed by C-4, not C-2; the axis the previous assessment blurred |
| **The seal (C-5)** | two variables dropped, one forced, probes added, scratch copy relocated | a narrowing of the pass-through set | the dropped pair configures installs, which the seal now forbids; nothing that verification needs is lost |
| **C-1** | new details, reasons, one new refusal, a stderr line | additive | no exit code or stream rule changes |

**Verdict (provisional).** C-4 **1.3.0**, C-5 **1.2.0**, C-1 **1.5.0**, `verification-sequence.md` **2.4.0**, C-C7 **1.1.1**, `support-policy.md` **1.2.0** are defensible under each contract's precedent **if** the maintainer accepts that source-built readers are outside the compatibility promise. If not, C-4 is **2.0.0** and C-1 moves with it. The numbers are confirmed when the revision texts of §6 are diffed against this table in the implementation pull request — not before, and not by calling every change additive.

---

## 9. TLS — the L-1 proof leg

The carry-over plan's AC-L1-1 … AC-L1-8 and its test list stand; the decisions of §4.2 and the maintainer's acceptance list below refine them.

### 9.1 What the leg is

A CI job `tls` in `ci.yml` (`ubuntu-latest`, **matrix `toolchain: [1.94.0, stable]`**, the job-level selection and the identity step of #64 reused unchanged) that generates an ephemeral CA and per-service certificates at run time, starts **Valkey with a TLS port**, **Mailpit with a certificate** (one container for implicit TLS, one for required STARTTLS, one plaintext-only), and the crate's **HTTPS OTLP receiver** (in-process, D-L1-3), and runs the positive, negative, control, and generated-starter cases below. Runs on every pull request; whether it becomes a required check is a repository-settings change the maintainer takes separately.

### 9.2 Trust (D-L1-1) — SR-012-10 … SR-012-12

| # | Requirement |
|---|---|
| **SR-012-10** | The CA reaches every client through **`SSL_CERT_FILE` set to an absolute path** (rustls-native-certs: "the `SSL_CERT_FILE` environment variable is checked first; if set, certificates are loaded from the path specified by that variable … if it's not set, then the platform-specific certificate source is used"). The job **unsets `SSL_CERT_DIR`** before every case (`env -u SSL_CERT_DIR`), so an inherited directory cannot supply or shadow a trust anchor. A test refuses to run under a relative `SSL_CERT_FILE`. |
| **SR-012-11** | **Trust cases are isolated in fresh processes and fresh clients.** Each trust case is a separate `cargo test … -- <case>` invocation with its own environment: (a) *trusted* — `SSL_CERT_FILE=<ca.pem>`: every positive case succeeds; (b) *unrelated* — `SSL_CERT_FILE=<other-ca.pem>`, a well-formed CA that signed nothing in the run: every positive case **fails closed** with the contract's category; (c) *unset* — `env -u SSL_CERT_FILE`: every positive case fails closed. Case (c) proves the trust came from the file. **It does not prove OS trust-store discovery works**, and no sentence claims it (D-L1-6). |
| **SR-012-12** | Per-adapter `ca_file` settings (`[cache] ca_file`, `[mail] ca_file`, `[otlp] ca_file`) are **deferred**: owner `renvor-cache`, `renvor-mail`, `renvor-observability` with `configuration-contract.md` (C-C11); target a decision record in Phase 013 planning; consequence stated in the generated README and in `phase-012-limitations.md`: an operator with a private CA sets `SSL_CERT_FILE` for the whole process. |

### 9.3 Certificates (D-L1-5)

The job prints `openssl version` first (ubuntu-latest's OpenSSL, recorded in the evidence). Generated into `$RUNNER_TEMP/tls` with `umask 077`: `ca` (basicConstraints CA:TRUE, keyUsage keyCertSign,cRLSign, 1-day validity), one leaf per service (`SAN = DNS:localhost, IP:127.0.0.1`, EKU serverAuth, 1-day validity), and the negative materials: `other-ca` (a second, unrelated CA), `mismatch` (SAN `DNS:other.invalid`, signed by the trusted CA), `expired` (signed by the trusted CA with `openssl ca -startdate/-enddate` in the past — available on OpenSSL 3.0). Private keys are `0600`, never printed, never committed, and **never uploaded**: the job has no artifact step, and a final step asserts that no `*.key` exists outside `$RUNNER_TEMP/tls` before deleting the directory.

### 9.4 Services

| Service | Positive endpoint | Negative endpoints | Delivery check (must be **absent** on negatives) |
|---|---|---|---|
| Valkey 9.1.1 | `--tls-port 6380 --port 0 --tls-cert-file … --tls-key-file … --tls-ca-cert-file … --tls-auth-clients no` (no client certificates: D-L1-4) | separate containers on 6381 (`mismatch` leaf), 6382 (`expired` leaf), and the plaintext instance on 6379 answering a `ValkeyEndpoint::tls` client | each negative asserts the adapter reported failure **before** any command, and that the target instance's key count (`DBSIZE` through the leg's admin client) is unchanged |
| Mailpit 1.29.1 | implicit TLS: `MP_SMTP_TLS_CERT/KEY` + `MP_SMTP_REQUIRE_TLS=true` (port 1465→1025); required STARTTLS: `MP_SMTP_TLS_CERT/KEY` + `MP_SMTP_REQUIRE_STARTTLS=true` (port 1587→1025) | `mismatch` and `expired` leaves on their own containers; a **plaintext-only** Mailpit (no certificate) for the *missing STARTTLS* case under `Security::StartTls` and the *plaintext peer* case under `Security::ImplicitTls` | each container's `/api/v1/messages` count: the positive cases show the message; every negative shows **zero** delivered |
| OTLP receiver | the crate's loopback receiver with `tokio-rustls` `ServerConfig` and the `otlp` leaf, `ring` provider named explicitly | the same receiver started with `mismatch`, `expired`, an `other-ca`-signed leaf, and a plaintext `TcpListener` at an `https://` endpoint | the receiver's recorded-request list: the positive shows the export; every negative shows **nothing received** and the exporter's failure counter incremented |

The Valkey image's TLS support is verified at leg time (`valkey-server --tls-port` refuses when built without TLS); the leg fails there rather than silently running plaintext.

### 9.5 The HTTPS receiver dependency (D-L1-3) — package review

| Item | Finding (crates.io and the lockfile, 2026-09-07) |
|---|---|
| Crate, version | `tokio-rustls` **0.26.4** (published 2025-09-26; not yanked) — the version **already in `Cargo.lock`** as a transitive dependency of `hyper-rustls` 0.27.9 (and of the `redis`/`lettre` rustls paths), so no new package enters the graph |
| Licence | `MIT OR Apache-2.0` — both on `deny.toml`'s allow-list |
| MSRV | `rust-version = 1.71` ≤ 1.94.0 |
| Dependencies | `rustls ^0.23.27`, `tokio ^1.0`; dev-only `rcgen`, `webpki-roots`, `argh` (not pulled by a dev-dependency on the crate) |
| Features | `default`, `logging`, `tls12`, `ring`, `aws-lc-rs`/`aws_lc_rs`, `fips`, `early-data`, `brotli`, `zlib`. Declared as `default-features = false, features = ["ring", "tls12"]` in `renvor-observability`'s `[dev-dependencies]` — never `aws-lc-rs`, never `fips`; xtask step 7's single-provider check stays green |
| Advisories | RustSec lists one entry for the crate, `RUSTSEC-2020-0019` (affects versions before 0.13); none against 0.26.x |
| Verdict | fit for the test-only receiver; `cargo deny check` unchanged; the receiver builds its `ServerConfig` with `rustls::crypto::ring::default_provider()` explicitly, mirroring the exporter's client side (ADR-0036) |

### 9.6 TLS version (D-L1-7)

TLS 1.3 is preferred; TLS 1.2 is the minimum permitted baseline; **nothing is inferred from `tls12` being compiled in**. Observation points, per connection:

| Path | Observation point | Fallback |
|---|---|---|
| OTLP over HTTPS | the receiver: `tokio_rustls::server::TlsStream::get_ref().1.protocol_version()` after the handshake, asserted and printed per test | — |
| SMTP (implicit and STARTTLS) | `lettre` exposes no negotiated version to the caller; the observation point is the **loopback capture**: `tcpdump -i lo -w $RUNNER_TEMP/tls/capture.pcap` started before the cases, decoded after with `tshark` (`-Y 'tls.handshake.type == 2'`, fields `tcp.srcport`, `tls.handshake.extensions.supported_version`, `tls.handshake.version`): a ServerHello carrying `supported_versions = 0x0304` is 1.3; otherwise the `version` field (`0x0303` = 1.2). The decoded table is the evidence line; the pcap is deleted with the keys | U-3 in the task plan records the alternative (server-side logs), which neither Mailpit nor Valkey documents as reliable |
| Valkey | `redis` exposes no negotiated version; the same capture, filtered on the Valkey ports | as above |

A 1.2 result closes nothing dishonestly and hides nothing: the evidence names which side lacked 1.3 (the server's `tls-protocols`/Go defaults, or the client stack's configuration), and a follow-up row is opened if a stack cannot reach 1.3.

### 9.7 The generated starter case (AC-L1-5)

A new census row **`tls`** (`RENVOR_TEST_STARTER_ROWS=tls`, run only by the `tls` job): `renvor new … --capabilities cache,mail,observability --framework-path <checkout>`, then — as the row's recorded edit, not the generator's — `config/cache.toml` set to `tls = true` **without** `allow_insecure_loopback`, `config/mail.toml` to `security = "starttls"` (and a second boot with `"implicit_tls"`) without the opt-in, and `config/otlp.toml` to the `https://127.0.0.1:<port>/v1/traces` receiver; the binary boots against the TLS services under `SSL_CERT_FILE`, its own `tests/starter.rs` passes, and a grep asserts no `allow_insecure_loopback` remains. Negative control: the same row with `SSL_CERT_FILE` unset must fail Boot with the contract's category and never fall back to plaintext.

### 9.8 Acceptance for L-1's closure — the complete list

L-1 and 010/L-1 close only when **every** item is recorded in Phase 012's evidence against a named head and run, for **both** toolchains of the `tls` job, and the validator has confirmed it; no waiver is granted here.

| # | Item |
|---|---|
| **AC-L1-1** … **AC-L1-8** | as the carry-over plan states them (job, four positive handshakes, negative controls each with a positive control in the same run, root-store control, the generated starter row, templates and README re-read, ledger closure with the measurement, dependency gates unchanged) |
| **AC-012-20** | Valkey TLS, SMTP implicit TLS, SMTP required STARTTLS, and HTTPS OTLP each observed positively (a `set`/`get`; a `send` + `verify()` twice; an export the receiver records) |
| **AC-012-21** | For **each applicable path**: unknown issuer, hostname mismatch, expired certificate, and a plaintext peer answering the TLS endpoint each fail closed with the contract's category — `CacheError::Unavailable`/`CacheBootError`, `MailError::Unavailable`/`Refused`, the exporter's counted failure |
| **AC-012-22** | **Missing STARTTLS specifically**: a relay that offers no STARTTLS under `Security::StartTls` is refused (`Tls::Required`, never `Opportunistic`) |
| **AC-012-23** | Reachable positive controls in every run; contractual failures (the category, not a panic or a timeout); **no plaintext downgrade** (the plaintext-only Mailpit and the plaintext Valkey port receive no command and no message in any negative case); **no successful application delivery** on any negative case (§9.4's delivery checks) |
| **AC-012-24** | The generated starter row of §9.7, both boots, positive and the unset-CA negative |
| **AC-012-25** | Both toolchains, named head and run identifiers, identity-step output quoted; the OpenSSL version; the negotiated TLS version per connection from the observation point of §9.6 |
| **AC-012-26** | The three trust cases of SR-012-11 recorded per positive case |

### 9.9 Stated exclusions

- **mTLS** (D-L1-4): outside this proof; no client-certificate setting appears in any contract; Valkey runs with `--tls-auth-clients no`; the closing measurement says so.
- **OS trust-store discovery** and **macOS/Windows TLS behaviour** (D-L1-6): unproven and stated; the platform jobs run no services and this leg is Linux-only.
- **Per-adapter CA files** (SR-012-12): deferred with owner and target.
- **Database TLS** (D-L1-2): WI-012-5, §11 — not L-1.

---

## 10. Streams S1–S6 — the documentation programme

### 10.1 What exists today (inspected before proposing anything)

`renvor-rs/renvor-docs` at `f4782ef` (2026-09-03): Docusaurus 3.10.2, Node 22, local search (`@easyops-cn/docusaurus-search-local`), `onBrokenLinks: 'throw'`, an explicit sidebar, CI `docs` (frozen install, image-input guard, `npm audit`, build, `lychee --require-https`) and `container` (build, Trivy, non-root read-only run), `publish-image.yml` (GHCR, immutable tag, SBOM, keyless provenance and SBOM attestations, anonymous-pull check), Dependabot; **no branch protection or ruleset** (`main` unprotected as of 2026-09-07). Eleven pages, all importing one hand-edited stamp (`docs/_stamp.mdx`: `renvor` 0.0.0 · framework source **`c57b4fb`** · MSRV 1.94.0 · pre-release):

| Page | Bytes | State against `main` at `7281e4f` |
|---|---|---|
| `intro` | 2.7 k | **stale**: "Phases 002 through 009"; "generated projects declare no Renvor dependency" (false since Phase 011's `--framework-path` starters) |
| `cli` | 16.4 k | **stale**: no `generate`, `--auth`, `--capabilities`, `--framework-path`, `--overwrite-unchanged`; exit codes and streams still correct |
| `persistence`, `database-portability`, `backup-restore` | 8.2 k, 11.0 k, 6.8 k | Phase 006–008 content; accurate; lack the Phase 011 generated-project sections |
| `authentication` | 5.4 k | Phase 009; lacks the auth starter and `generate auth` |
| `api-reference` | 2.5 k | the facade table lacks the five capability crates and the testkit |
| `verification` | 4.7 k | **stale**: "63 required four-row census pairs" (87 at `7281e4f`); the stable-leg history of §0.4 absent |
| `support-policy`, `governance` | 5.6 k, 2.6 k | accurate; `support-policy` will need the rustup floor and the dated CI sentence |
| *absent entirely* | — | installation and toolchain; `renvor new` guides; architecture and request lifecycle; REST/errors/validation/OpenAPI/pagination/versioning; configuration, secrets, containers, local HTTPS; the five capabilities; the testing kit; migrations as a guide; deployment and hardening; upgrade/compatibility/deprecation/security policies; examples; limitations; CLI JSON schemas; feature flags |

`renvor-rs/renvor` carries the rustdoc inputs, `examples/README.md` (pointing at the three facade example targets `minimal`, `providers`, `configuration` under `crates/renvor/examples/`), the four `trycmd` help snapshots (`tests/cmd/help*.trycmd`, `exit-codes.trycmd`), the JSON envelope fixtures (`tests/json/`), and no JSON Schema for the envelope. `renvor-rs/renvor-infra` carries the manifests and overlays for `renvor-docs` (staging and production) and `renvor-site`; deployment is GitOps by digest. The out-of-repository cheat sheet (§1–§22, executed on `ab701d2`) is the only end-to-end tutorial that exists; it stays local and untracked, and nothing here copies it.

**Consequence**: no existing page is replaced; every existing page is refreshed to the artifact SHA under S3; the missing topics are new pages.

### 10.2 S1 — versioned, searchable documentation (FR-012-D1 … D4)

| # | Requirement |
|---|---|
| **FR-012-D1** | **Versioning** is enabled in Docusaurus with the current set as the sole entry, labelled **`pre-release (snapshot <sha7>)`**, and the version selector visible with that one entry; no numbered version is cut until Phase 013 tags a release candidate (`PLAN.md` §26.7: one version per published minor release; none is published). The label's SHA is generated from the handoff artifact (FR-012-D14), never typed. |
| **FR-012-D2** | **Search** stays local (already true); the index covers every page and the generated references. |
| **FR-012-D3** | The stamp (`_stamp.mdx`) is **generated** from the artifact manifest (framework SHA, `renvor` version, MSRV, the rustc identities the artifact was built with, the date) — no longer hand-edited. |
| **FR-012-D4** | `renvor-docs` `main` gains branch protection with `docs` and `container` required (`PLAN.md` §26.1, §26.10) — **a repository-settings change that needs the maintainer's separate authorization** (task plan §5); the plan records the gap until then. |

### 10.3 S2 — the API-only quickstart and the authenticated application example (FR-012-D5 … D8)

| # | Requirement |
|---|---|
| **FR-012-D5** | **Quickstart (API-only)**: `renvor new` of a database starter with the example domain (`--framework-path <checkout>`, because nothing is published), the container profile, migrations, the CRUD routes, `renvor routes`/`openapi`, the generated test, and a clean stop — every command copy-pastable, every command **executed verbatim by CI** from a clean environment (FR-012-D18). No hand-written source: the quickstart is what the generator produces. |
| **FR-012-D6** | **Authenticated application example**: a committed, buildable project under `examples/authenticated-api/` in the framework repository, generated by `renvor new … --auth session --capabilities mail,cache --framework-path` and then extended by hand with an owned resource (the shape the cheat sheet §17–§20 proved), with relative path dependencies, a README that records the exact generation command and the hand edits, and its own `tests/`. Exercised by CI (FR-012-D19). Whether the committed tree or a CI-time regeneration is the source of truth is **U-4**. |
| **FR-012-D7** | Both examples state the prerelease condition on their first line: no crate is published; the framework checkout is the dependency. |
| **FR-012-D8** | Each example's page links the CI run that last exercised it (FR-012-D20). |

### 10.4 S3 — every `PLAN.md` §18 v1 topic (FR-012-D9)

**FR-012-D9**: the documentation set covers each §18 v1 bullet with at least one page whose sources are the contracts and code at the artifact SHA, whose claims link to evidence, and whose limitations are visible:

| §18 v1 topic | Page(s) | Existing / new | Sources at the SHA |
|---|---|---|---|
| installation and toolchain support | `installation` | new | `SUPPORT.md`, `support-policy.md`, §5 of this brief (the pin, the MSRV, the rustup floor, bare toolchains) |
| `renvor new` interactive and non-interactive guides | `renvor-new`, `renvor-generate` | new | C-1, C-4, C-5, the wizard parity tests |
| architecture and request lifecycle | `architecture` | new | `PLAN.md` §7, `lifecycle-contract.md`, `http-runtime.md`, `http-routing.md` |
| REST, errors, validation, OpenAPI, pagination, versioning | `rest`, `errors`, `validation`, `openapi`, `pagination` | new | `problem-details.md`, `error-taxonomy.md`, `validation.md` (with its Phase 012-targeted keyword gaps visible), `openapi.md`, `collection-reads.md`, `api-stability.md` |
| direct SQLx and SeaORM guides for PostgreSQL and MySQL | `persistence`, `database-portability`, `backup-restore`, `migrations` | refresh ×3, new ×1 | `database-portability.md`, ADR-0016…0023 |
| auth starter, policies, session/token choices, deployment hardening | `authentication`, `auth-starter`, `hardening` | refresh, new, new (S4) | Phase 009 contracts and ADR-0024…0030, `generate auth` |
| configuration, secrets, local HTTPS, containers, migrations, testing, observability, deployment | `configuration`, `secrets`, `containers`, `testing-kit`, `capabilities/{cache,jobs,mail,storage,observability}`, `deployment` (S4) | new | `configuration-contract.md`, `capabilities-contract.md`, `jobs-contract.md`, `observability-contract.md`, `renvor-testkit` |
| CLI reference with exit codes and JSON schemas | `cli`, `reference/cli/*` (generated), `reference/json-schema` | refresh, generated (S5) | C-1, C-2, the help snapshots, the envelope schema (U-5) |
| crate API documentation and feature flags | `api-reference`, `feature-flags` | refresh, new | rustdoc (S5), the crates' `Cargo.toml` features, xtask step 7's isolation rows |
| upgrade, compatibility, deprecation, and security policies | `policies` | new | `support-policy.md`, `api-stability.md`, `SECURITY.md`, `dependency-advisory-policy.md`, `RELEASING.md` (as policy, not as a release) |
| tested examples for API-only and authenticated applications | `quickstart`, `examples/authenticated-api` | new (S2) | FR-012-D5/D6 |
| *(cross-cutting)* limitations | `limitations` | generated (S6) | the ledgers at the SHA (FR-012-D21) |

### 10.5 S4 — deployment and hardening guidance (FR-012-D10 … D12)

| # | Requirement |
|---|---|
| **FR-012-D10** | `deployment`: the generated container profile (the pinned builder, `RUSTUP_AUTO_INSTALL=0`, distroless runtime), configuration and secrets outside the image, migrations at Boot, health and readiness, graceful shutdown, the trusted-proxy and host-validation settings, observability export, backup and restore (existing page), rollback by digest. **Deploying Renvor's own sites is not this page and remains a separate authorization (§10.8).** |
| **FR-012-D11** | `hardening`: TLS to backends (written from the §9 leg's measured behaviour, after it exists — the page cannot claim what the leg has not shown, and says so until then), the double opt-in for plaintext, session and CSRF settings, abuse controls, secret handling, the ASVS/NIST references the constitution names — as verification guidance, never as certification. |
| **FR-012-D12** | Every hardening claim links to the test or contract that proves it; a claim without a proof is labelled *guidance, unproven* (the pattern `database-portability` already uses). |

### 10.6 S5 — references generated from an immutable framework artifact (FR-012-D13 … D17)

| # | Requirement |
|---|---|
| **FR-012-D13** | **The framework produces the artifact.** A workflow `docs-artifact.yml` in `renvor-rs/renvor` builds, at one commit, `renvor-docs-inputs-<sha>.tar.zst` containing: all-feature rustdoc (`cargo doc --workspace --all-features --no-deps`); the CLI help tree captured from the **built binary** (`renvor --help`, every subcommand, matching the `trycmd` snapshots that test it); the JSON envelope schema (U-5) and the exit-code table; the contracts, the limitation ledgers, and the waiver ledger **at that SHA**; and a `manifest.json` (SHA, tree, `renvor` version, MSRV, the rustc identities of the build, date). `SHA256SUMS` and a keyless build-provenance attestation over the archive (the pattern `release-dry-run.yml`'s `attest` job already uses). |
| **FR-012-D14** | **The documentation repository consumes it and nothing else.** A workflow `import-framework-artifact.yml` in `renvor-docs` (`workflow_dispatch` with the framework run id, SHA, and expected digest) downloads the artifact, verifies the checksum and the attestation (`gh attestation verify … --repo renvor-rs/renvor`), unpacks rustdoc under `static/api/<sha7>/`, regenerates `docs/reference/cli/*.mdx`, `docs/reference/json-schema.mdx`, `docs/limitations.mdx`, `docs/_stamp.mdx`, and the version label, and opens a pull request. **No human copies framework content** (`PLAN.md` §26.8); the synchronisation is this recorded, versioned, automated step. |
| **FR-012-D15** | **Immutability before a tag.** No release tag exists and none may be created in this phase. The artifact is addressed by **commit SHA and digest** and attested; that satisfies "traceable to the framework revision it describes" (§26.7) but **not** "a signed release tag or the published crate". Recorded as an explicit assumption for the maintainer (**U-7**): until Phase 013 tags, the docs are stamped to a commit-addressed, attested artifact and say so on every page; the same workflows take a tag input when one exists. |
| **FR-012-D16** | The API reference page keeps the docs.rs pointer as a *future* destination and links the embedded rustdoc at the SHA; feature flags are listed per crate from the manifests in the artifact. |
| **FR-012-D17** | The CLI reference is generated, not written: a page per command from the captured help, the exit-code table from C-1, the envelope from C-2 with the schema; a docs-CI test fails when the generated pages differ from the artifact they claim. |

### 10.7 S6 — clean environments, CI-tested examples, evidence links, visible limitations, prerelease wording (FR-012-D18 … D23)

| # | Requirement |
|---|---|
| **FR-012-D18** | **Clean environment**: a framework CI job `docs-examples` (matrix both toolchains, the identity step reused) runs in a fresh container that has **only** what the `installation` page tells a reader to install (rustup ≥ 1.28.1 with the pinned channel and the two components, Docker for the services), with an empty `CARGO_HOME` seeded exactly as FR-006 states, and executes the quickstart's commands verbatim from the page's fenced blocks (extracted by a script, so the page and the run cannot drift). |
| **FR-012-D19** | The same job builds and tests `examples/authenticated-api` against the four-row services. |
| **FR-012-D20** | Both runs are per pull request in the framework repository, and their run identifiers are what the example pages link (FR-012-D8). |
| **FR-012-D21** | **Limitations are visible**: `docs/limitations.mdx` is generated from the ledgers in the artifact (every open row of Phase 011 and the retained earlier rows, with owner and target), and the intro links it above the fold. |
| **FR-012-D22** | **Claims link to evidence**: a docs-CI check (`scripts/check-evidence-links.mjs`) requires every line beginning `Evidence:` to link a permalink (`/blob/<40-hex>/` or an Actions run/job URL) and every such link to resolve (lychee already runs); a relative or branch-addressed evidence link fails the build. |
| **FR-012-D23** | **Prerelease wording** (`PLAN.md` §26.6) is a CI test, not a review habit: `scripts/check-prerelease-claims.mjs` fails when a page lacks the stamp, shows `cargo install renvor` or `cargo add renvor` outside an admonition that says the crate is unpublished, or contains the phrases the §26.6 list forbids (a maintained list in the script, with a positive and a negative control page). |

### 10.8 Cross-repository ownership and the handoff

| Owns | `renvor-rs/renvor` | `renvor-rs/renvor-docs` | `renvor-rs/renvor-infra` |
|---|---|---|---|
| Source of truth for | code, contracts, ledgers, rustdoc inputs, CLI help, the examples, `docs-artifact.yml`, `docs-examples` | prose, navigation, versioning, search, the generated reference pages **as imported**, `import-framework-artifact.yml`, the three CI checks of §10.7, the image | manifests, overlays, Flux, promotion by digest |
| Must never contain | website source | framework source copied by hand (§26.8) | application source, secrets |
| Handoff | produces `renvor-docs-inputs-<sha>.tar.zst` + `SHA256SUMS` + attestation | consumes by run id + SHA + digest; verifies; regenerates; opens a PR | consumes the published image digest |
| Deployment | — | publishes the image on `main` (existing workflow) | **deploys — a separate authorization**, never part of this phase's tasks |

### 10.9 Acceptance for S1–S6 (the `PLAN.md` Phase 012 acceptance, made measurable)

| `PLAN.md` acceptance | Measured by |
|---|---|
| documentation builds without broken links | `renvor-docs` CI `docs` (build with `onBrokenLinks: 'throw'`, lychee) green on the head that stamps the artifact |
| commands run from clean environments | FR-012-D18's job green, both toolchains, run ids recorded |
| examples are exercised in continuous integration | FR-012-D19/D20 |
| claims link to evidence | FR-012-D22's check green; the evidence index in the artifact |
| all current limitations are visible | FR-012-D21's page generated from the ledgers at the SHA, and the row count asserted against the ledgers |
| the set is prepared in `renvor-docs`, served from an immutable artifact, prerelease stated | FR-012-D13…D15, D23; "served" waits for the separate deployment authorization |

---

## 11. Work items

### WI-012-1 — a seeded starter without auth is refused by its own `rustfmt` *(not started)*

- **What**: `renvor new … --database postgres --example-domain --seed-data --capabilities storage --framework-path <checkout> --yes` on `ab701d2` is refused with `project_verification_failed` at the pre-placement `cargo fmt --check` on `src/seed.rs`; both ORMs, both toolchains; the same command without `--seed-data`, or with `--auth session`, succeeds. No census row renders seeds without auth (the L-16 shape).
- **Evidence**: `defect-1-repro.json`, the four `defect-1-control-*.json`, the five `D-ctl-*.json`, `D-controls.log` under the retained directory. Template: `crates/renvor-cli/templates/starter/src_seed.rs.j2`.
- **Owner**: `renvor-cli`. **Acceptance**: (a) the seed template renders `rustfmt`-stable output for every auth value on both ORMs; (b) a census row renders `--seed-data` without auth and passes on both legs; (c) negative control: with the template change reverted the row fails at the formatting check with the same reason; (d) seeded-with-auth snapshots byte-identical or moved with the template version.

### WI-012-2 — a starter's `/metrics` renders only the jobs families *(not started)*

- **What**: on a `cache,jobs,mail,storage,observability` starter after one operation of each kind, `/metrics` shows `process_start_time_seconds` and six `renvor_jobs_*` families, nothing from `renvor_cache_*`, `renvor_mail_*`, `renvor_storage_*`: only `src_capabilities_jobs.rs.j2` takes the `Registry`.
- **Evidence**: `A2-g13m-metrics.txt`. Templates: `src_capabilities_{cache,mail,storage}.rs.j2`.
- **Owner**: `renvor-cli`. **Acceptance**: (a) the three providers are constructed with the same `Registry`; (b) a census row asserts each family after its operation and — negative control — the absence of an unselected capability's families; (c) the generated README's observability section lists the families rendered.

### WI-012-3 — `hostile.rs::a_hostile_argv0_does_not_reach_the_terminal_raw` failed once with `ETXTBSY` *(not started; cause a hypothesis)*

- **What**: run `34017828678`, job `verify (1.94.0)` (`101444736622`) on `2f74962` (a documentation-only change) panicked at `expect("the planted binary runs")` with `Os { code: 26, kind: ExecutableFileBusy }`; green on three local runs (macOS) and on every other CI run before and since. The copy-then-exec race with a concurrent fork holding the copy's write descriptor is a **hypothesis from the error and the test's shape**, not a finding.
- **Evidence**: `ci-pr63-2f74962-verify-1.94.0-hostile-etxtbsy.log`.
- **Owner**: the `renvor-cli` test harness (`crates/renvor-cli/tests/hostile.rs`), not the generator. **Acceptance**: (a) the cause is **established on Linux before any change** — the hypothesis confirmed (a looped run of the test binary beside a concurrent spawner, or a trace of which descriptor is open at the exec) or refuted and replaced; (b) the harness change matches the confirmed cause; (c) a stated number of consecutive green runs on `ubuntu-latest` on both legs **without a re-run**; (d) negative control: a planted `argv[0]` carrying a raw `ESC` still fails the test. **No blanket retry is pre-authorised**: a bounded retry on `ETXTBSY` is acceptable only if (a) confirms that exact race, and only around the exec, never around the assertion.

### WI-012-4 — the `stable` CI legs compiled with the repository pin *(complete — §0.2, §0.3)*

Remaining under this number: nothing. AC-012-4…7's remainder is the L-2 batch's (§5.9).

### WI-012-5 — database TLS: PostgreSQL and MySQL through SQLx and SeaORM, with certificate and hostname verification *(new, D-L1-2; not started)*

- **What**: the persistence adapters enable SQLx's `tls-rustls-ring-native-roots` on both engines, and no test has completed a TLS session to either database; the four census rows connect in plaintext. A production-shaped starter therefore has an unproven database transport, exactly as L-1 had for the three services.
- **Scope**: PostgreSQL 17.11 with `ssl=on` and a `hostssl` rule; MySQL 8.4.11 with a certificate and `--require_secure_transport=ON`; the same ephemeral CA as §9; **certificate and hostname verification** on both engines through both adapters — SQLx `PgSslMode::VerifyFull` with the root certificate and `MySqlSslMode::VerifyIdentity` with the CA, and the same through SeaORM's `ConnectOptions`; the four rows positively; the negative cases (unknown issuer, hostname mismatch, a plaintext-only server under a verifying mode) per engine per adapter, each failing closed with `DatabaseError`'s category and never downgrading; the trust-source control of SR-012-11; the `verify-full` starter boot. **First task**: measure how the CA and the mode reach the driver through `renvor_database::ConnectionString` today (URL parameters `sslmode`/`sslrootcert`, `ssl-mode`/`ssl-ca`), because 010/L-15 (credentials inside the URL) already has a proposed-ADR target for the same type — the two must be designed together (**U-9**).
- **Estimate**: two batches, both gate legs each; roughly the size of the §9 leg (the certificate script is shared).
- **Disposition rule**: this item is **in the Phase 012 plan** (task plan §2, B3). If the maintainer defers it, the deferral is an explicit disposition that opens a new limitation row with owner (`renvor-sqlx`, `renvor-seaorm`), target (the next phase), and consequence ("the database transport of a generated starter is compiled for TLS and never exercised"); it is not postponed by omission.

---

## 12. Inventory — every other row in the tree that names Phase 012

These rows target Phase 012 in the ledgers but were not in the maintainer's Phase 012 instruction of 2026-09-07. Each needs a **disposition** (in this phase with a task, or explicitly re-targeted with owner and consequence) before the phase closes; none is decided here.

| Row | Where | What | Proposed disposition (decision needed) |
|---|---|---|---|
| L-4 | `phase-011-limitations.md` | resource generator knows five field types | re-target (Phase 013 planning) unless the docs examples need more |
| L-5 | same | capabilities chosen at `renvor new` only | re-target; documented in `renvor-generate` |
| L-10 | same | no database-backed starter row on Windows | re-target; the platform jobs stay service-less |
| L-11 | same | nothing upgrades a project with the record | **stays** — D-L2-10 confirms no upgrade command this phase; the row's consequence is restated in FR-012-10 |
| L-13 | same | `renvor-auth` in every database starter's graph | re-target (an adapter feature split) |
| L-14 | same | the rename-failure branch reached by no test | re-target unless a cross-device fixture is cheap in the `tls` job's runner |
| L-15 | same | reserved SQL identifiers from a curated list | **in this phase** if the four-row services are already running for B3: a live `SELECT 1 AS <word>` control is small |
| L-16 | same | starter Rust not passed through `rustfmt` | **in this phase** with WI-012-1 (the same template class) |
| 010/L-5, 010/L-6 | same | symlinks inside a storage root; two worker processes on one queue | re-target with reasons stated |
| 010/L-15 | same | database credentials inside the URL | **decide with WI-012-5** (U-9): the same type carries the TLS mode |
| 009/L-1 (password reset revokes nothing), 009/L-2, L-3, L-5, L-12, L-13, L-14, L-16, L-19, L-20, L-23 | `phase-009-limitations.md` | the eleven Phase 009 rows retained "for Phase 012" | 009/L-1 is the most consequential open security finding in the tree and is **not** in this plan's streams; it needs the maintainer's explicit disposition (a batch here, or a re-target with the consequence stated) |
| `contracts/validation.md` `pattern`, `allOf`/`anyOf`/`oneOf`/`not`/`if` | the validation contract | keywords refused rather than mis-validated, targeted at Phase 012 | re-target (no docs stream depends on them); the `validation` page shows them as refused |
| Phase 005 findings 2 and 10 (transferred as L-16/L-17 of that phase: `compat::compare` recursion depth; property tests over schema keyword values) | `phase-005-evidence.md`, `waivers.md` | | re-target with reasons |
| "The local licence gate is narrower than the CI licence gate" | `deferred-verification-work.md` | `cargo deny` passed where dependency-review failed (MIT-0) | **in this phase** with the `tokio-rustls` dev-dependency review (B2): assert locally what dependency-review asserts |
| W-001…W-024 | `waivers.md` | the single-maintainer review waivers | unchanged by this plan; each implementation batch cites the pattern |

---

## 13. What this brief does not do

It implements nothing; edits no contract, template, test, or workflow; proposes decision records without accepting them; closes, narrows, waives, or reinterprets neither L-1 nor L-2 nor any row of §12; does not merge, tag, release, publish, or deploy; does not touch the cheat sheet; and does not authorise the repository-settings change of FR-012-D4 or the deployment of §10.8. The maintainer's approval of this specification — the planning checkpoint — precedes the first implementation batch.

## 14. How this revision was checked

- `origin/main` was fetched on 2026-09-07 and read at `7281e4f`; every file, function, template, workflow step, contract section, and test named above was located there before it was named. Line numbers are omitted so the references survive edits.
- WI-012-4's history was read from the pull request's commit list and each commit's check-runs through the GitHub API; the run identifiers are quoted from those responses.
- The rustup changelog was read from `rust-lang/rustup` `CHANGELOG.md` (entries 1.28.0, 1.28.1, 1.28.2, 1.29.0) and the override precedence from `doc/user-guide/src/overrides.md`, both through the GitHub API; the quotations are verbatim.
- `tokio-rustls` 0.26.4's licence, MSRV, features, and dependencies were read from crates.io; its advisory list from the RustSec database; its presence in `Cargo.lock` from `main`.
- Mailpit's `MP_SMTP_TLS_CERT`/`MP_SMTP_TLS_KEY`/`MP_SMTP_REQUIRE_STARTTLS`/`MP_SMTP_REQUIRE_TLS` options were read from its runtime-options documentation; the rustls-native-certs `SSL_CERT_FILE` rule from that crate's README; Valkey's TLS options are stated from its documented server flags and are marked for verification at leg time.
- The documentation repository's tree, workflows, configuration, pages, and protection state were read through the GitHub API on 2026-09-07.
- No secret, credential, token, or cookie appears in this file, in the task plan, or in the retained evidence.
