# Phase 012 — Specification and decision brief

**Companion to**: [`phase-012-security-carryover.md`](phase-012-security-carryover.md) (the L-1/L-2 carry-over plan; this brief supersedes its proposals where the two differ) · [`phase-011-limitations.md`](phase-011-limitations.md) · [`phase-011-evidence.md`](phase-011-evidence.md) · `PLAN.md` §"Phase 012 — REST documentation and production examples" · `CONSTITUTION.md` · contracts C-1 (`command-surface.md` 1.4.0), C-4 (`template-contract.md` 1.2.0), C-5 (`generation-transaction.md` 1.1.0)
**Drafted**: 2026-09-06, against `main` at `bc979e83ca1bc35559d2f713ebc6686a08d9df85` (tree `1fb19224619c1aa1ddf73896dcbce10a0e5301d7`; the squash of PR #63, parent `ab701d2`), by the maintainer's session, on branch `docs/phase-012-decision-brief`
**Status**: **BRIEF — decisions requested.** Two decisions are taken and recorded here (D-L2-0 on the contract version, D-L2-1 on the mechanism); every other choice is a *Proposal* with a recommendation, its trade-offs, and acceptance criteria, presented together so that the maintainer decides once. This file implements nothing, changes no contract, template, test, or workflow, closes nothing, and grants no waiver. **L-1 and L-2 stay open.** Nothing is tagged, released, published, or deployed. The cheat sheet stays local and untracked.
**Working copy**: `specs/012-rest-documentation-and-production-examples/spec.md` under the gitignored `specs/` tree is the same text; this tracked file is the clone-visible mirror and the authority if the two differ.
**Identifiers**: requirement, acceptance, decision, and work-item identifiers are brief-local (`FR-012-n`, `SR-012-n`, `AC-012-n`, `D-L1-n`, `D-L2-n`, `WI-012-n`). `D-L1-1…7` and `D-L2-0…7` keep the numbers the carry-over plan gave them; `D-L2-8…10` are new.

---

## 0. What is decided, what is asked, and what was found

| Kind | Items | Where |
|---|---|---|
| **Decided** (maintainer, 2026-09-06) | **D-L2-0** — C-4's provisional target is 1.3.0, contract front matter reconciled (PR #63). **D-L2-1** — the mechanism: a generated `rust-toolchain.toml` **and** `rust-version` in `Cargo.toml`, with `verified_with` recording the compiler actually used. *Mechanism only*: selection, override handling, operation without rustup, and compatibility were left to be specified together | §4, §5 |
| **Specified here, for decision** | The three toolchain versions and where each lives (§3); the declaration, the record, the sealed resolution step, and the operator-facing behaviour for overrides, absent toolchains, and machines without rustup (§5, §7); the stable compatibility leg (§6); the contract compatibility assessment for C-4 1.3.0 (§8) | §3–§8 |
| **Requested** | **D-L2-2 … D-L2-10** (toolchain) and **D-L1-1 … D-L1-7** (TLS), each with a recommendation, trade-offs, and acceptance criteria | §9 |
| **Found while preparing this brief** | **F-1** the repository's own "stable" CI legs have compiled with the pinned 1.94.0, not with stable, since the pin landed (§2.1). **F-2** rustup 1.29.0 downloads a pinned-but-absent toolchain from a *listing* command unless told not to (§2.2). Neither is fixed here | §2, WI-012-4 |
| **Scoped work items** | **WI-012-1** seeded no-auth starter refused by its own `rustfmt`; **WI-012-2** starter `/metrics` renders only the jobs families; **WI-012-3** `hostile.rs` `ETXTBSY` investigation (cause a hypothesis); **WI-012-4** the stable CI legs (F-1) | §10 |

Recommended order once decided: **WI-012-4 first** — until the stable legs really run stable, no "verified on both toolchains" claim about a generated tree can be made from CI, and AC-012-6 below cannot be measured.

---

## 1. Phase 012 scope map

`PLAN.md` states Phase 012 as: *versioned documentation site; API-only quickstart; authenticated application example; all Section 18 v1 documentation; deployment and hardening guides; searchable CLI/API references*, accepted when *documentation builds without broken links; commands run from clean environments; examples are exercised in continuous integration; claims link to evidence; all current limitations are visible*, with the documentation set prepared in `renvor-rs/renvor-docs` and served at `docs.renvor.dev` from an immutable framework artifact, prerelease status stated.

| Stream | Content | Status in this brief |
|---|---|---|
| S1–S6 | Documentation site, quickstart, authenticated example, Section 18 v1 documentation, deployment and hardening guides, CLI/API references | **Not specified here.** They need their own specification sections once the decisions below are taken, because each depends on them: a deployment guide must state the toolchain policy (§5), the examples "exercised in continuous integration" must run on both toolchains for real (§6, WI-012-4), and "commands run from clean environments" is exactly what the sealed resolution step (§5.3) proves |
| **S7** | Security carry-over: **L-2** (this brief §3–§9.1) and **L-1** (§9.2) | Specified / decisions requested |
| **S8** | Correctness triage carried from the cheat-sheet execution of 2026-09-06 and from this brief's own findings | Work items §10 |

Out of scope for this brief, as for the carry-over plan: any implementation; an S3 storage adapter; token issuance; GraphQL; frontend flags; the package ecosystem; a merge of anything; publication.

---

## 2. Findings that change the picture since the carry-over plan

### 2.1 F-1 — the repository's "stable" CI legs compile with the pin, not with stable

**What the repository does.** `rust-toolchain.toml` at the root has pinned `channel = "1.94.0"` since `98a4e2c` (2026-08-11, "build: add Rust 2024 workspace with a fixed 1.94.0 MSRV floor"). `.github/workflows/ci.yml` installs the matrix toolchain with `dtolnay/rust-toolchain@6c977a6…`, whose action runs `rustup toolchain install <toolchain>` and `rustup default <toolchain>` and sets no `RUSTUP_TOOLCHAIN` (read from the action's `action.yml` at that commit), and then runs plain `cargo xtask verify` in the checkout. rustup's precedence puts a directory's `rust-toolchain.toml` **above** the default toolchain — rustup says so itself in the probe's `overridden by` attribution (§2.2), in the CI logs below, and in any checkout on this machine, where the default is stable and `rustup show active-toolchain` prints `1.94.0-aarch64-apple-darwin (overridden by '…/rust-toolchain.toml')` — so in the checkout the proxies resolve to 1.94.0 on every leg.

**What the logs show** (extracted through the GitHub API on 2026-09-06 and retained out of repository as `renvor-blog-api-cheatsheet-evidence/2026-09-06-ab701d2/logs/ci-stable-legs-run-the-pin.log`):

| Job (run) | After `rustup default stable` | During the gate |
|---|---|---|
| `verify (stable)`, run `34025561472` on `3742ec1`, job `101465783525` | `info: note that the toolchain '1.94.0-x86_64-unknown-linux-gnu' is currently in use (overridden by '/home/runner/work/renvor/renvor/rust-toolchain.toml')` | inside the `cargo xtask verify` step: `info: syncing channel updates for 1.94.0-x86_64-unknown-linux-gnu` / `info: latest update on 2026-03-05 for version 1.94.0 (4a4ef493e 2026-03-02)` — rustup **installed 1.94.0 on the stable runner** because the proxy resolved to it, then ran the gate with it |
| `verify (stable)`, run `34018208859` on `5acb492`, job `101445807521` | the same note | the same install |
| `platform (macos-latest, stable)`, run `34025561472`, job `101465783728` | the same note for `1.94.0-aarch64-apple-darwin` | the same install |
| *Control*: `docs`, run `34025561477`, job `101465783662` — `docs.yml` gives the action `toolchain: "1.94.0"` **by design** | the same note (expected: the input and the file agree) | 1.94.0 installed and used, as intended. Included so the note is not mistaken for proof by itself: the proof for the stable legs is the *install of 1.94.0 on a runner that was given `stable`* |

**What it means.** The required check `verify (stable)` and the `platform (…, stable)` checks have verified the pinned MSRV a second time on every pull request since 2026-08-11 (`docs` runs 1.94.0 on purpose and is unaffected). They did not verify current stable. `SUPPORT.md`'s "only the stable job moves" and `rust-toolchain.toml`'s own comment ("CI additionally runs current stable") describe an intent the workflow does not carry out. The **local** legs recorded in the evidence records were real: the maintainer runs `cargo +stable xtask verify`, and `+stable` sets `RUSTUP_TOOLCHAIN`, which beats the file (`phase-011-evidence.md` §10, §13: "rustc 1.97.1" in leg B). CI's lines in those records — "verify (stable) (39m19s)" and the like — described a check that ran 1.94.0.

**What it does not mean.** Nothing about the code is known to be wrong on stable: every local stable leg was green. The gap is in what CI *proved*, not in what the tree does. Nothing is fixed here: a workflow edit is outside a planning record. → **WI-012-4**, and the stable-leg specification in §6 exists because of this finding.

### 2.2 F-2 — rustup installs a pinned-but-absent toolchain from a listing command

Measured 2026-09-06 on macOS with rustup 1.29.0 in a throw-away directory (retained as `…/logs/rustup-auto-install-probe.log`):

| Probe | Result |
|---|---|
| `rust-toolchain.toml` with `channel = "stable"`; `rustc -vV` | `release: 1.97.1` — the file selects |
| same file, `RUSTUP_TOOLCHAIN=1.94.0 rustc -vV` | `release: 1.94.0` — the environment variable wins over the file |
| same file, `rustc +1.94.0 -vV` | `release: 1.94.0` — the `+toolchain` argument wins over the file |
| `channel = "1.93.0"` (not installed), `RUSTUP_AUTO_INSTALL=0 cargo --version` (dist server pointed at a closed loopback port, `http://127.0.0.1:9`, so any download attempt is refused at once) | `error: toolchain '1.93.0-aarch64-apple-darwin' is not installed` / `help: run 'rustup toolchain install' to install it` — a named refusal, no download |
| same directory, plain `rustup toolchain list` (the probe's oversight) | `info: syncing channel updates for 1.93.0-aarch64-apple-darwin` … `info: downloading 6 components` — **a listing command downloaded and installed a toolchain** because the directory's pin named an absent one. Uninstalled afterwards |

Reading: with rustup 1.28.1 and later, auto-install is the default and `RUSTUP_AUTO_INSTALL=0` turns it off; with 1.28.0 it was off; before 1.28.0 it was always on. **Any** proxy or rustup invocation that resolves the active toolchain in a pinned directory can download. A generator that renders a pin and then runs *anything* through rustup in that directory must set `RUSTUP_AUTO_INSTALL=0` itself, or it will download without asking — the "unexpected download" this brief must exclude (SR-012-1). The exact rustup version threshold is to be confirmed against rustup's changelog before implementation (D-L2-8).

### 2.3 Carried from the plan, unchanged

The generated Dockerfile's comment names a `rust-toolchain.toml` the project does not contain, and its builder image `rust:1.94.0-slim` is pinned independently of anything the tree declares (AC-L2-7 of the plan → FR-012-9 here). The record `.renvor/generated.toml` has no toolchain field. `renvor doctor` probes `cargo` against the generator's own `CARGO_PKG_RUST_VERSION` only.

---

## 3. Three versions, distinguished

The row L-2 and the decisions under it conflate three things that must be named apart. Each has one declaration, one enforcer, one record, and one owner.

| | **Minimum supported Rust version (MSRV)** | **Default pinned toolchain** | **Compiler actually used** |
|---|---|---|---|
| What it is | The oldest compiler the project promises to build with | The compiler rustup selects when nothing overrides it | The compiler that ran a specific verification |
| Declared in a generated project | `rust-version = "1.94.0"` in `Cargo.toml` (**new**, D-L2-1) | `rust-toolchain.toml`: `channel`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"` (**new**, D-L2-1) | not declared — **measured** and written to `.renvor/generated.toml` as `[verified_with]` (**new**, D-L2-1) |
| Value | the framework's own MSRV, `1.94.0` (`Cargo.toml` `[workspace.package] rust-version`), for a starter because its path dependencies need it; the generator's `CARGO_PKG_RUST_VERSION` for a skeleton | the framework checkout's `rust-toolchain.toml` channel for a starter (D-L2-5); the generator's MSRV for a skeleton (D-L2-4) | `rustc -vV`'s `release`, `commit-hash`, `host`, plus `cargo --version`, read inside the sealed environment from inside the staged project, after the five checks |
| Enforced by | cargo: an older compiler is refused ("package requires rustc 1.94.0 or newer"), a newer one runs; `--ignore-rust-version` bypasses it explicitly. With resolver 3 (edition 2024) the resolver also prefers dependency versions compatible with it — inert for a starter because its lockfile is seeded from the framework's (FR-006) | rustup, and only rustup: absent rustup, the file is inert | nothing enforces it; it is the truth about one run, and the assertion of AC-012-6 compares it with what the caller intended |
| Who moves it | the framework, at an MSRV bump (`SUPPORT.md` fixed-floor policy) | the author, by editing the file; the generator, at the next template version | nobody; it changes when the environment changes, and the record shows that it did |
| May legitimately differ from the others when | never below the pin; the pin may exceed it | an operator or CI sets `RUSTUP_TOOLCHAIN`, `+toolchain`, or a directory override (§7) — this is how the stable leg works | whenever an override is active, or rustup is absent |

The three coincide on a clean machine with rustup and the pin installed. Every other case in §7 is one where they differ, and the rule of this brief is that a difference is **visible** (a notice and the record) and never **silent**.

---

## 4. D-L2-1 — decided (mechanism)

Verbatim (maintainer, 2026-09-06): *"choose both a generated rust-toolchain.toml and rust-version in Cargo.toml, with verified_with recording the compiler actually used. This approves the mechanism only. Toolchain selection, override handling, operation without rustup, and compatibility remain to be specified together."*

What it fixes: the declaration is two files plus one record; `verified_with` is a record of what ran, never a pin. What it leaves open, and where this brief takes it up: selection (§5.1, D-L2-4, D-L2-5), override handling (§7, D-L2-2, D-L2-3), operation without rustup (§7, D-L2-6, D-L2-8), compatibility (§8, D-L2-9, D-L2-10).

---

## 5. Specification — the toolchain declaration, the record, and the sealed resolution step

### 5.1 The declaration (FR-012-1 … FR-012-3, FR-012-9)

| # | Requirement |
|---|---|
| **FR-012-1** | Every generated tree carries `rust-toolchain.toml` at its root: `channel = "<pin>"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`. For a starter, `<pin>` is read at generation from `<framework-path>/rust-toolchain.toml` (D-L2-5) and must equal the framework's `rust-version`; a checkout whose two values differ is refused with a named reason before anything is staged (the framework, not the starter, is inconsistent). For a skeleton, `<pin>` is the generator's `CARGO_PKG_RUST_VERSION` (D-L2-4). The file is rendered, digested, and listed in the record like every generated file; the template version moves (C-4 snapshot policy). |
| **FR-012-2** | Every generated `Cargo.toml` carries `rust-version = "<msrv>"`, where `<msrv>` equals the pin at generation. `edition = "2024"` is unchanged. |
| **FR-012-3** | The generated `README.md` states the pin, why it exists (L-2's consequence: a tree verified on one compiler may fail `clippy -D warnings` on another), how to change it (edit the file, run `cargo clippy --all-targets -- -D warnings` and `cargo test`), and what happens without rustup (§7). |
| **FR-012-9** | The generated Dockerfile's builder tag and the pin name the same channel from the same source, its comment names the file that now exists, and the builder stage sets `ENV RUSTUP_AUTO_INSTALL=0` so a pin that outruns the image fails the build loudly instead of downloading inside it (the plan's AC-L2-7). |

### 5.2 The record (FR-012-4 … FR-012-5)

`.renvor/generated.toml` gains two tables, written **after** verification and before the manifest, exactly where the digests are written today (C-4 1.2.0's first rule):

```toml
record_version = 2            # new: readers older than this refuse with a named reason (D-L2-9)

[toolchain]
pinned = "1.94.0"             # the channel rendered into rust-toolchain.toml
rust_version = "1.94.0"       # the rust-version rendered into Cargo.toml

[verified_with]
rustc = "1.97.1 (8bab26f4f 2026-07-14)"   # `rustc -vV` release and commit, measured in the seal, from inside the staged tree
host = "aarch64-apple-darwin"
cargo = "1.97.1 (…)"
rustup = "1.29.0"             # or "absent"
selected_by = "environment"   # environment | argument | directory_override | toolchain_file | default | no_rustup
```

| # | Requirement |
|---|---|
| **FR-012-4** | `[verified_with]` is measured, never derived: the seal runs `rustc -vV` and `cargo --version` **inside the staged project directory** with the same sealed environment the five checks receive, after the checks pass. A record written before verification, or from the generator's own process environment, is the Phase 011 regression in the other direction and is a test failure. `selected_by` is taken from the environment when `RUSTUP_TOOLCHAIN` is set (`environment`; a `+toolchain` argument reaches the child as the same variable and is indistinguishable, so `argument` is recorded only when the generator itself was given one), from `rustup show active-toolchain`'s override text otherwise, and is `no_rustup` when no `rustup` is on `PATH`. Parsing rustup's text is the one fragile step; the test pins the exact strings for the rustup version `SUPPORT.md` names (D-L2-8). |
| **FR-012-5** | The record's digest stays outside the snapshot's pinned set (its path is pinned); the `[toolchain]` values are part of the snapshot (they are rendered), the `[verified_with]` values are not (they differ by machine, as `Cargo.lock` does). `renvor check` prints both tables; `renvor new --output json` and `renvor generate … --output json` carry them under `data.toolchain` and `data.verified_with`. |

### 5.3 The sealed resolution step (FR-012-6 … FR-012-8, SR-012-1 … SR-012-4)

Before the first of the five checks, the seal resolves which compiler will run, and refuses when the answer is "none" or "one I would have to download":

| # | Requirement |
|---|---|
| **FR-012-6** | The sealed environment sets `RUSTUP_AUTO_INSTALL=0` **unconditionally** (forced, not passed through) for every child it spawns, including the resolution probe. |
| **FR-012-7** | The resolution probe is `rustc -vV` run inside the staged tree under the seal. Its failure with rustup's "is not installed" text is the refusal **`tool_missing`, exit 5**, `details.tool = "rustup toolchain <channel>"`, `details.remedy = "rustup toolchain install <channel> --component rustfmt --component clippy --profile minimal"`, before any check runs and before anything is placed; a compiler older than `rust_version` is the same refusal with `details.tool = "rustc >= <msrv>"` (cargo would refuse later with a compiler error; the generator refuses first with a name). A toolchain that resolves but lacks `rustfmt` or `clippy` is `tool_missing` naming the component — the framework's own `rust-toolchain.toml` comment records that this happens with `--profile minimal`. |
| **FR-012-8** | When the resolved compiler is not the pin (an override is active, or rustup is absent), the run **proceeds** — the operator or CI chose it — and prints one line to stderr: `verifying with rustc <release> (<selected_by>); the project pins <channel>`. The record carries both. Nothing is silent; nothing is refused for being different. (This is what keeps the stable leg meaningful, §6.) |
| **SR-012-1** | **No download, ever, during generation or verification**: FR-012-6, plus `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` removed from `PASSED_THROUGH` (they configure installs, which the seal now forbids) — D-L2-3. A control test points the dist server at an unroutable address and pins an absent channel: the result must be `tool_missing`, and the probe must complete in bounded time. |
| **SR-012-2** | **No silent fallback**: the compiler that ran is always in the record and in `--output json`; a difference from the pin is always printed (FR-012-8); a missing toolchain is never resolved to "whatever the default is". |
| **SR-012-3** | `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS` stay in the seal (D-L2-3, the operator's own trust), and their *presence* — never their values — is recorded in `[verified_with]` as `wrapper = true/false`, `rustflags = true/false`, so a verification made under a wrapper says so. |
| **SR-012-4** | The generator never runs a rustup command that lists, installs, updates, or sets a default; it runs proxies (`rustc`, `cargo`) and `rustup show active-toolchain` only, all under FR-012-6. |

### 5.4 Existing trees and `renvor generate` (FR-012-10, D-L2-10)

| # | Requirement |
|---|---|
| **FR-012-10** | A project generated at template version 7 or earlier has no pin and a record without `record_version`. `renvor generate` (resource, migration, or auth) into such a tree **does not refuse** on that account: it verifies with whatever resolves (recording `verified_with` as for any run, with `pinned = "none"`), prints the FR-012-8 line with `the project pins nothing`, and states once that `renvor generate toolchain` does not exist and the README of a new project shows the two files to add by hand. Whether a `renvor generate toolchain` action should exist is D-L2-10; L-11 ("nothing upgrades a project with it") already names the gap. |

---

## 6. The stable compatibility leg

**The requirement**: the `stable` legs of `verify` and `platform` must execute the framework **and** every generated tree on current stable, and the evidence must show that they did, so that a pin cannot be tested twice silently — which is what happens today (F-1) and would keep happening with generated pins unless the leg's own selection is explicit.

| # | Requirement |
|---|---|
| **AC-012-1** | `ci.yml` sets `RUSTUP_TOOLCHAIN: ${{ matrix.toolchain }}` at **job** level for `verify` and `platform` (`docs.yml` pins `1.94.0` by input and matches the file; it needs only AC-012-2), so every proxy in every step — the gate, the census, the generated trees' own `cargo` inside the seal (which passes `RUSTUP_TOOLCHAIN` through) — resolves to the leg's toolchain regardless of any `rust-toolchain.toml` in the checkout or in a staged tree. rustup's documented precedence, measured in §2.2, is what makes this sufficient. |
| **AC-012-2** | Every leg sets `RUSTUP_AUTO_INSTALL=0` at job level, so an absent toolchain fails the leg by name instead of downloading in the middle of a step (F-1's install lines are the symptom). |
| **AC-012-3** | A control step before the gate prints and asserts the identity: `rustc -vV` and `rustup show active-toolchain` run **inside the checkout**, and the `release` line must equal the matrix toolchain's `rustc +<toolchain> -vV` release; a mismatch fails the leg. The step's output is what the evidence record quotes for "toolchain" — not a local run. |
| **AC-012-4** | Step 1 of `contracts/verification-sequence.md` (the prerequisite probe) records the compiler that runs the sequence in `cargo xtask verify`'s own output, so a local leg and a CI leg are recorded the same way. |
| **AC-012-5** | The census (`starter_matrix.rs`) asserts, for every row on every leg, that the placed record's `[verified_with].rustc` equals the gate's own `rustc -vV` release — the compiler that verifies a generated tree is the compiler the leg claims. On the stable leg this is the proof that the generated pin did **not** win; on the MSRV leg it is the proof that it did not matter. |
| **AC-012-6** | A unit-level control in `verify.rs`: with a staged tree pinned to `X` and `RUSTUP_TOOLCHAIN=Y` in the sealed environment, the resolution probe reports `Y` and `selected_by = "environment"`; with the variable unset, it reports `X` and `toolchain_file`. Needs two installed toolchains; skipped with a `SKIPPED:` line and required under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` in CI (the census pattern). |
| **AC-012-7** | `SUPPORT.md`'s "only the stable job moves" and `rust-toolchain.toml`'s comment "CI additionally runs current stable" are true again, and `phase-011-evidence.md` gains an **erratum** paragraph (dated, appended, nothing rewritten) stating which CI checks between `98a4e2c` and the fix ran 1.94.0 under a `stable` name, and that the local legs recorded in its tables were genuine. |

---

## 7. Explicit behaviour: overrides, absent toolchains, and machines without rustup

Nothing in this table falls back silently, downloads, or refuses for a difference the operator chose.

| Situation | What selects the compiler | `renvor new` / `renvor generate` | Exit | Recorded (`selected_by`, `pinned`, `rustc`) | Operator sees |
|---|---|---|---|---|---|
| Pin present and installed, no override (the clean machine) | the file | verifies with the pin | 0 | `toolchain_file`, pin, pin | nothing extra |
| `RUSTUP_TOOLCHAIN=Y` in the environment (also how `cargo +Y` reaches children; the CI legs after AC-012-1) | the environment | verifies with `Y` | 0 | `environment`, pin, `Y` | `verifying with rustc Y (environment); the project pins X` |
| `rustup override set Y` in an ancestor directory | the directory override | verifies with `Y` | 0 | `directory_override`, pin, `Y` | the same line, `(directory override)` |
| Pin names a toolchain that is **not installed**, rustup ≥ the version D-L2-8 names | nothing — the probe fails by name | **refused before any check**, nothing staged | **5** `tool_missing`, `details.tool = "rustup toolchain X"`, the install command as the remedy | no record (nothing placed) | the refusal and the exact `rustup toolchain install …` line; **no download** |
| The same, rustup **older** than that version | rustup would install | **refused before the probe**: the generator reads `rustup --version` first and, below the threshold, refuses with `tool_missing`, `details.tool = "rustup >= <threshold>"` rather than run any proxy in the pinned tree | 5 | none | the refusal names the rustup version and the threshold; no download |
| Installed pin lacks `rustfmt` or `clippy` | the file | refused at the probe | 5 `tool_missing` naming the component and the `rustup component add` remedy | none | the refusal |
| **No rustup** on `PATH`, `rustc` ≥ MSRV (a distribution compiler, a Docker image with a bare toolchain) | `PATH` | verifies; the pin file is inert; `rust-version` still refuses older compilers | 0 | `no_rustup`, pin, what ran | `verifying with rustc Z (no rustup); the project pins X, which cannot be selected here` |
| No rustup, `rustc` < MSRV | `PATH` | refused at the probe | 5 `tool_missing`, `details.tool = "rustc >= <msrv>"` | none | the refusal; the README's install pointer |
| `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS` set | as above, then the wrapper/flags act inside the compiler | verifies (the operator's trust, D-L2-3) | 0 | as above plus `wrapper = true` / `rustflags = true` | nothing extra (the record says it) |
| Docker build of the generated project | the image's rustup with the image's installed toolchain | not a generator path; `ENV RUSTUP_AUTO_INSTALL=0` in the builder makes a pin the image lacks fail the build by name | — | — | the build error names the toolchain |
| `renvor generate` into a tree without a pin (template ≤ 7) | whatever resolves | proceeds, records `pinned = "none"` | 0 | as the row above | `the project pins nothing` (§5.4) |
| `renvor doctor` in a directory with a pin | — | reports the pin, the installed toolchains, whether the pin is installed with both components, the rustup version, and which compiler `rustc -vV` resolves to here — reporting only (D-L2-7) | 0 / 1 as today | — | the table |

---

## 8. Contract compatibility assessment — is C-4 1.3.0 (minor) the right number?

The maintainer's D-L2-0 condition: a minor version is not forced if the change requires a major revision. Assessed section by section against `template-contract.md` 1.2.0, with C-5 and C-1 beside it, using the contracts' own precedent (C-1's 1.2.0–1.4.0 added flags, refusals, and classification rows as minors; C-4's 1.1.0–1.2.0 added file groups, the record, and the snapshot policy as minors; none of them removed or contradicted a rule).

| C-4 section | Change | Kind |
|---|---|---|
| Delivery | none — the new file is embedded and rendered like every other | neutral |
| Versioning | none in rule; `TemplateSet::version` moves from 7 to 8 because the manifest changes (the policy's normal path) | neutral |
| Rendering environment / Bounds | none — one more small file inside the project root; no archive, no network; the generator still never downloads (SR-012-1 strengthens the bound) | neutral / additive |
| Verbatim files / Starter sets | two new rendered files in every set (`rust-toolchain.toml`) and one new key in an existing one (`rust-version`) | additive |
| Generated-on-demand files | `rustfmt` at generation now runs under the resolved toolchain of the target tree, under `RUSTUP_AUTO_INSTALL=0` — the same tool, an explicit selection | additive |
| Snapshot stability policy | `[toolchain]` joins the pinned set; `[verified_with]` joins `Cargo.lock` outside it; the record's path stays pinned | additive |
| Provenance record | `record_version`, `[toolchain]`, `[verified_with]` | **additive with one compatibility risk** — below |
| Output paths | none | neutral |

**The one risk**: `record.rs` derives `Record` with `#[serde(deny_unknown_fields)]`, so a reader older than the record refuses a record that carries the new tables. Today no reader has been published (C-1 and C-4 are "public from the first release that ships it; nothing has been published yet"; the package is `0.0.0`), so there is no installed base to break — which is why this stays a minor now and must not stay unstated: **D-L2-9** asks the maintainer to fix the reader rule (`record_version` plus "a reader refuses a newer record by name and accepts an older one") so the next change is not a major by accident.

**The other way a major would be forced**, and the specification avoids it: refusing to operate on a tree that has no pin. §5.4 keeps `renvor generate` working on template-7 trees (D-L2-10), and FR-012-8 keeps an override a proceed-with-notice rather than a refusal. If the maintainer decides either the other way — refuse pin-less trees, or refuse when the compiler differs from the pin — C-4 becomes **2.0.0**, and C-1 with it.

**Neighbouring contracts**: C-5 `generation-transaction.md` 1.1.0 → **1.2.0** (the seal forces `RUSTUP_AUTO_INSTALL=0`, drops two variables, records the compiler; the protocol, atomicity, and residue rules are unchanged). C-1 `command-surface.md` 1.4.0 → **1.5.0** (new `tool_missing` details, the stderr notice, `renvor check`/`doctor` output; no exit code changes). C-2 `json-output.md`: `data.toolchain` and `data.verified_with` are additive fields under the existing `schemaVersion` 2, following its rule that additions do not bump the wire version. `verification-sequence.md`: step 1 records the compiler (AC-012-4).

**Verdict (proposal)**: **C-4 1.3.0 is compatible** under the contract's own precedent, provided D-L2-9 and D-L2-10 are decided as recommended. The number is confirmed at the moment the C-4 revision text is written and its diff is checked against this table — not before.

---

## 9. All remaining decisions, with recommendations

Each row: the decision; the recommendation; the trade-off the maintainer accepts by taking it; the acceptance criteria that prove it. Nothing here is decided.

### 9.1 L-2 — toolchain (D-L2-2 … D-L2-10)

| # | Decision | Recommendation | Trade-off | Acceptance |
|---|---|---|---|---|
| **D-L2-2** | What the **stable** legs prove once trees are pinned | **Job-level `RUSTUP_TOOLCHAIN` and `RUSTUP_AUTO_INSTALL=0` on every leg, plus the identity control step** (§6). Not `cargo +stable xtask verify` alone: `+stable` sets the variable for that process tree only, and the `platform` and `docs` jobs run other commands | The stable legs will now run stable for the first time since 2026-08-11; a real stable-only failure (a new default lint) will surface and must be fixed rather than re-run — that is the point | AC-012-1 … AC-012-7 |
| **D-L2-3** | Which toolchain-shaping variables stay in `PASSED_THROUGH` | **Keep** `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS` (build caches are the intended use; the operator already holds that trust) and record their presence (SR-012-3). **Drop** `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` (install-only). **Force** `RUSTUP_AUTO_INSTALL=0` (FR-012-6). Keep `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN` | A wrapper still runs inside the "sealed" step; the residual stays a ledger row (AC-L2-10 of the plan) instead of a refusal, because refusing wrappers would break every cache-using machine | the three existing seal tests unchanged; a new one asserts the dropped pair is absent and the forced value present; SR-012-1's unroutable-dist-server control |
| **D-L2-4** | Whether the **skeleton** pins too | **Yes**, to the generator's MSRV (`CARGO_PKG_RUST_VERSION`, the constant `doctor` already reads), with the same components and profile | The dependency-free tree gains a rustup dependency it did not have; without rustup it is inert (§7) and `rust-version` still guards the floor | every skeleton variant's snapshot carries the file; `doctor`'s constant and the rendered channel are asserted equal |
| **D-L2-5** | Source of a **starter's** pin | The framework checkout's own `rust-toolchain.toml`, read at generation; refuse a checkout whose channel and `rust-version` disagree (FR-012-1) | A starter generated from a checkout pinned to a newer channel than the generator's MSRV carries that newer pin — correct, because its path dependencies are that checkout | a census control with a checkout copy whose channel is edited: the starter renders the edited channel; a copy whose channel and `rust-version` differ is refused by name |
| **D-L2-6** | Machines **without rustup** | **Proceed with a notice and the record** (§7 rows "No rustup"); `rust-version` is the only guard; the README says so; `doctor` reports rustup absent | A distribution compiler newer than the pin verifies a tree the pin did not choose; the record shows it, and the README explains it | a control that empties `PATH` of rustup (a directory with a bare `rustc`/`cargo` symlinked from an installed toolchain): `selected_by = "no_rustup"`, exit 0, the notice printed; the same with a `rustc` below the MSRV: `tool_missing` |
| **D-L2-7** | Whether `renvor doctor` reads a project's pin | **Yes, reporting only** — no new refusal (§7 last row) | `doctor` grows a directory-dependent section; it must say "no project here" outside one | a `doctor` test in a pinned tree and one outside |
| **D-L2-8** *(new)* | The **minimum rustup version** the guarantee needs, and what to do below it | Confirm the version that introduced `RUSTUP_AUTO_INSTALL` against rustup's changelog (expected 1.28.1); name it in `SUPPORT.md`; **below it, refuse to verify a pinned tree** (§7) rather than inspect `$RUSTUP_HOME/toolchains` by hand, because a listing-free filesystem inspection would still be racing rustup's own resolution the moment `cargo` runs | Operators on old rustup must update before generating; the message tells them exactly that | a test that stubs `rustup --version` below the threshold: `tool_missing`, no proxy invoked (asserted by a `PATH` that has no `cargo`) |
| **D-L2-9** *(new)* | The **record reader rule** (forward compatibility) | `record_version = 2`; a reader refuses a record whose version is **newer** than it knows, by name (`generation_conflict`? no — a new reason `record_unsupported`, exit 5); accepts an older one and treats absent tables as "unknown"; keeps `deny_unknown_fields` **within** a known version | One more reason code; a reader from the future is refused rather than guessed at | unit tests for older, equal, newer; C-1 and C-2 gain the reason |
| **D-L2-10** *(new)* | What `renvor generate` does in a tree **without a pin**, and whether a `renvor generate toolchain` action should exist | Proceed with the notice (§5.4) now; **do not** add the action in Phase 012 — it is the upgrade path L-11 names, and belongs with the upgrade design, not with this row | Template-7 projects stay pin-less until their author adds two files by hand, guided by the README of a new project | the §5.4 control: a template-7 tree, `generate resource`, exit 0, `pinned = "none"`, the notice |

### 9.2 L-1 — TLS to Valkey, SMTP, and the OTLP receiver (D-L1-1 … D-L1-7)

The carry-over plan's acceptance criteria AC-L1-1 … AC-L1-8 and its test list stand unchanged; the decisions are restated here with trade-offs so that they are taken with the toolchain ones.

| # | Decision | Recommendation | Trade-off | Acceptance |
|---|---|---|---|---|
| **D-L1-1** | How the TLS leg trusts its throw-away CA | **`SSL_CERT_FILE`** (honoured by `rustls-native-certs`, already passed through the seal; no configuration change) for the CI leg, **and** a per-adapter `ca_file` setting **deferred** to a decision record of its own: it is a public configuration change under the configuration contract, and the leg does not need it to prove the handshake | The proof covers the process-wide root store, not a per-adapter file; an operator who needs a private CA for one adapter only waits for the deferred setting | AC-L1-4's control (the same tests fail with the variable unset) |
| **D-L1-2** | Whether TLS to PostgreSQL/MySQL joins the leg | **A new row**, so L-1 closes on its own wording; the database TLS leg is scheduled behind it because SQLx's feature is the same on both engines and the CA plumbing is shared | The database path stays plaintext-proven for one more phase; stated as a new limitation row, visible | the new row exists before L-1 is marked closed |
| **D-L1-3** | The HTTPS OTLP receiver | **The crate's own loopback receiver with `tokio-rustls`** as a dev-dependency (package research per the constitution; the workspace already enables rustls with `ring` only — `aws-lc-rs` must not be pulled in, see the single-provider rule) | A test-only dependency through the package gate; the proof is not a collector image's configuration | AC-L1-2's HTTPS export; `cargo deny` unchanged; the provider list unchanged |
| **D-L1-4** | Client certificates (mTLS) | **Out of scope**; no contract names them | None now; a future row if a contract does | AC-L1-7's closing measurement states mTLS out of scope, and no client-certificate setting appears in the configuration contract |
| **D-L1-5** | Certificate generation in CI | **The runner's `openssl` CLI** — nothing joins the dependency graph; the certificates are generated at run time and never committed | The leg depends on the runner image's `openssl`; asserted by a version print at the top of the job | AC-L1-1; AC-L1-8 |
| **D-L1-6** | macOS/Windows root-store discovery | **Out of scope for closure**; recorded as a consequence in the closing measurement, because the platform jobs run no services | Root-store discovery on two platforms stays unproven and says so | AC-L1-7's measurement names it |
| **D-L1-7** | TLS 1.2 versus 1.3 | **Record what is observed**; restrict nothing without a decision record; the three stacks allow 1.2 | A 1.2 negotiation would close the row honestly rather than be hidden by a restriction nobody decided | AC-L1-2's per-test protocol version in the evidence |

---

## 10. Scoped work items

Each item is independent of the decisions above except where stated, has an owner proposal, reproduction, and acceptance criteria, and is **not started**.

### WI-012-1 — a seeded starter without auth is refused by its own `rustfmt`

- **What**: `renvor new … --database postgres --example-domain --seed-data --capabilities storage --framework-path <checkout> --yes` on `ab701d2` is refused with `project_verification_failed` at the pre-placement `cargo fmt --check` on `src/seed.rs`; both ORMs, both toolchains; the same command without `--seed-data`, or with `--auth session`, succeeds. No census row renders seeds without auth (the L-16 shape).
- **Evidence**: `renvor-blog-api-cheatsheet-evidence/2026-09-06-ab701d2/logs/defect-1-repro.json`, the four `defect-1-control-*.json`, the five second-round controls `D-ctl-seeded.json`, `D-ctl-unseeded.json`, `D-ctl-seeded-auth.json`, `D-ctl-seeded-sea.json`, `D-ctl-seeded-194.json`, and `D-controls.log`. Template: `crates/renvor-cli/templates/starter/src_seed.rs.j2`.
- **Owner** — *Proposal*: `renvor-cli` (starter templates; the census).
- **Acceptance**: (a) the seed template renders `rustfmt`-stable output for every auth value on both ORMs; (b) a census row renders `--seed-data` without auth and passes on both legs; (c) negative control: with the template change reverted the row fails at the formatting check with the same reason; (d) seeded-with-auth snapshots either byte-identical or moved with the template version.

### WI-012-2 — a starter's `/metrics` renders only the jobs families

- **What**: on a `cache,jobs,mail,storage,observability` starter after one operation of each kind, `/metrics` shows `process_start_time_seconds` and six `renvor_jobs_*` families, nothing from `renvor_cache_*`, `renvor_mail_*`, `renvor_storage_*`: only `src_capabilities_jobs.rs.j2` takes the `Registry`.
- **Evidence**: `…/logs/A2-g13m-metrics.txt`. Templates: `crates/renvor-cli/templates/starter/src_capabilities_{cache,mail,storage}.rs.j2`.
- **Owner** — *Proposal*: `renvor-cli` (starter templates); the capability crates are not implicated.
- **Acceptance**: (a) the three providers are constructed with the same `Registry`; (b) the generated starter test or a census row asserts each family after its operation and — negative control — the absence of an unselected capability's families; (c) the generated README's observability section lists the families rendered.

### WI-012-3 — `hostile.rs::a_hostile_argv0_does_not_reach_the_terminal_raw` failed once with `ETXTBSY`

- **What**: run `34017828678`, job `verify (1.94.0)` (`101444736622`) on `2f74962` panicked at the `expect("the planted binary runs")` with `Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }`; green on three local runs and on every other CI run. The copy-then-exec race with a concurrent fork holding the copy's write descriptor is a **hypothesis**, not a finding.
- **Evidence**: `…/logs/ci-pr63-2f74962-verify-1.94.0-hostile-etxtbsy.log`; the job URL above.
- **Owner** — *Proposal*: the `renvor-cli` test harness (`crates/renvor-cli/tests/hostile.rs`), not the generator.
- **Acceptance**: (a) the cause is established on Linux before any change (the hypothesis confirmed by a looped run beside a concurrent spawner or by tracing the open descriptor, or refuted); (b) the harness change matches the confirmed cause (for the hypothesis: flush and close the copy before exec and retry a bounded number of times on `ETXTBSY`, or serialise the planted-binary tests); (c) a stated number of consecutive green runs on `ubuntu-latest` on both legs without a re-run; (d) negative control: a planted `argv[0]` carrying a raw `ESC` still fails the test.

### WI-012-4 — the `stable` CI legs compile with the repository pin (F-1)

- **What**: §2.1. `verify (stable)` and `platform (…, stable)` have run 1.94.0 since `98a4e2c` (2026-08-11); `docs` runs 1.94.0 by its own input and is a control, not a finding. The required check named `verify (stable)` has been a second MSRV run.
- **Evidence**: `…/logs/ci-stable-legs-run-the-pin.log` (job log excerpts for `101465783525`, `101445807521`, `101465783728`, and the control `101465783662`); `dtolnay/rust-toolchain` `action.yml` at `6c977a6…` (no `RUSTUP_TOOLCHAIN`); the local reproduction: in any checkout, `rustup show active-toolchain` with the default set to stable prints `1.94.0-… (overridden by '…/rust-toolchain.toml')`.
- **Owner** — *Proposal*: the maintainer (workflows and `SUPPORT.md`), with `xtask` for AC-012-4.
- **Acceptance**: AC-012-1 … AC-012-4 and AC-012-7; the first green `verify (stable)` after the fix is recorded with its control-step output as the first CI proof of stable; a **negative control** is run once and recorded: the same workflow with the job-level variable removed must fail the identity step (proving the step, not the variable, is what guards).
- **Dependency**: none on D-L2-2 … D-L2-10 — this is the framework's own leg and should land **before** any generated pin, or the generated pins will inherit the same silent double test.

---

## 11. Sequencing, and what this brief does not do

Proposed order: **WI-012-4** → the L-2 batch (C-4 1.3.0, C-5 1.2.0, C-1 1.5.0 revision texts; templates; `record.rs`; `verify.rs`; `doctor`; the tests of §6 and §9.1; the census) → the L-1 leg (§9.2) → WI-012-1/2/3 as independent correctness batches → streams S1–S6, each with its own specification section that takes this brief as input. Every batch runs the full gate on its own commits; **no gate is run on this draft** (it changes no code).

This brief does not implement any of the above; does not close, narrow, waive, or reinterpret L-1 or L-2; does not edit their rows; does not revise a contract (the D-L2-0 correction was PR #63's, a metadata change); does not change a workflow; does not tag, release, publish, or deploy; does not touch the cheat sheet, which stays local and untracked.

---

## 12. How this brief was checked

- Every file, function, template, workflow step, and contract section named above was located on `main` at `bc979e8` before it was named; line numbers are omitted so the references survive edits.
- F-1: four job logs were fetched through the GitHub API on 2026-09-06 and the quoted lines saved verbatim (with terminal colour codes stripped) beside the cheat-sheet evidence; the action's `action.yml` at the pinned commit was read from GitHub; the pin's history was read with `git log -S'channel' -- rust-toolchain.toml`.
- F-2 and the precedence table: one throw-away directory on this machine (rustup 1.29.0, macOS aarch64); the toolchain the probe installed by accident was uninstalled the same minute and the incident is recorded in the probe log, not hidden.
- The cargo `rust-version` and resolver statements are cargo's documented behaviour and are marked for a control test rather than asserted as measured.
- The rustup version threshold for `RUSTUP_AUTO_INSTALL` is stated as expected (1.28.1) and marked for confirmation (D-L2-8).
- No secret, credential, token, or cookie appears in this file or in the retained evidence; the job logs quoted contain none.
