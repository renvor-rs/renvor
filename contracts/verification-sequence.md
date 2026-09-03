---
description: "Contract — the ordered verification sequence `cargo xtask verify` runs"
version: "2.0.0"
status: "normative — enforced executably by `xtask`. 2.0.0 (2026-09-03) transfers the Docusaurus build and rendered-site link check to the now-live `renvor-rs/renvor-docs` repository, removes those two checks and their Node/npm/lychee prerequisites from this framework sequence, renumbers working-tree cleanliness to step 9, and makes rustdoc cover all features. This is a BEHAVIOUR change: the framework gate now owns only framework verification, while the independently required `docs` context keeps warning-denied all-feature rustdoc visible in branch protection. 1.2.1 (2026-08-26) corrected the exit-code table and fail-closed sample output. 1.2.0 (2026-08-26) made the four-row database environment mandatory. This version identifies the contract text, not a stability promise"
---

# Contract: Verification Sequence

**Feature**: Phase 001 — governance foundation | **Satisfies**: FR-018, FR-019, FR-022, FR-023, FR-024, FR-025, FR-037, FR-055

One command, one behaviour, locally and in automation:

```
cargo xtask verify
```

The two verification jobs invoke the same entry point. That is the point — a second copy of the
full sequence in workflow files would silently diverge, and divergence is how a skipped check gets
reported as a pass. The separately required `docs` context repeats step 5 alone so all-feature,
warning-denied rustdoc remains visible in branch protection.

## Steps

Executed in order. None is conditional. None is skipped.

| # | Step | Command | Toolchain required |
|---|---|---|---|
| 1 | Prerequisite probe | — | Rust (pinned), **and a PostgreSQL and MySQL the census can reach** |
| 2 | Formatting | `cargo fmt --all --check` | Rust |
| 3 | Lint | `cargo clippy --all-targets --all-features -- -D warnings` | Rust |
| 4 | Tests | `cargo test --workspace --all-features`, then the end-to-end route relay, then the four-row persistence census — every one of the 63 required (row, test) pairs must report in | Rust, both databases |
| 5 | API documentation | `cargo doc --workspace --all-features --no-deps` with warnings denied | Rust |
| 6 | Dependency and licence policy | `cargo deny check` | `cargo-deny` |
| 7 | Architecture invariants | crate DAG, transport and persistence isolation, per-driver adapter compiles, facade isolation, lean compile, publishable dependencies, required package metadata, instability wording, executable name — each with a control | Rust |
| 8 | Secret scan | `gitleaks git . --no-banner` (history) **and** `gitleaks dir . --no-banner` (working tree) | `gitleaks` |
| 9 | Working-tree cleanliness | assert no untracked or modified files remain | Rust |

### Documentation-site ownership changed on 2026-09-03

The earlier eleven-step sequence built the temporary Docusaurus site under `docs/` and checked its
rendered links. That source has moved to the public `renvor-rs/renvor-docs` repository, and
`https://docs.renvor.dev` now serves the replacement. Keeping the framework copy or continuing to
make a Rust gate install the site would create two sources of truth and couple crate verification to
another repository's release cadence.

The documentation repository now owns its frozen npm install, content controls, production build,
rendered-site link check, container verification, advisory scan, image construction, and
publication. The infrastructure repository owns the staging and production deployment manifests
and Flux reconciliation. This repository still owns Rust API documentation. Step 5 and the
separately required `docs` check both run warning-denied rustdoc with every feature enabled.

### Step 1 refuses a run without the database environment — added 2026-08-26

`cargo xtask verify` exits **2** — *"a required tool or the database environment is missing; no
steps ran"* — unless all three of `RENVOR_TEST_POSTGRES_URL`, `RENVOR_TEST_MYSQL_URL`, and
`RENVOR_TEST_REQUIRE_DATABASE` are set and non-empty.

**This corrects a step that contradicted this contract's own opening rule.** The table below says
*"Executed in order. None is conditional. None is skipped."* Step 4's four-row census was
conditional on `RENVOR_TEST_REQUIRE_DATABASE`: without it the census printed
`ok — NOT RUN` and **returned success**, so a full `cargo xtask verify` could exit **0** having
executed none of the row-suite pairs. The four rows of `PLAN.md` §10.1 are not optional, so their
evidence is not optional either.

The remedy is a refusal, not a smaller verification, and specifically **not** an automatic one:

- Missing prerequisites are reported in step 1, alongside missing tools, with the setup each needs.
- **Nothing is started for the operator.** A gate that provisions its own dependencies says nothing
  about the machine it ran on.
- The census keeps its own check as defence in depth. Reaching it with the environment incomplete
  now **fails** rather than reporting `ok`.
- CI is unaffected: the `verify` job already exports all three variables before invoking this
  command, and still requires all four rows.

Asserted by three tests in `xtask`, including the negative one this correction exists for — a
missing prerequisite cannot yield exit 0 or a success marker — and a positive control proving a
complete environment still lets the sequence proceed.

**Step 7 was missing from this table until 2026-08-20.** It has run in `xtask` since Phase 002.
The omission is recorded rather than quietly filled in, because a contract that under-describes
what the command does is the same defect class as a command that under-performs what the contract
promises.

**Rows 4 and 7 under-described the command until Phase 008.** Step 4 had grown two sub-steps — the
end-to-end relay, and the four-row persistence census — and step 7's list had never included
transport and persistence isolation or required package metadata, both of which it had been
running. Phase 008 added the per-driver adapter compile to step 7 and brought both rows into line
with what `xtask` actually does. Recorded for the same reason as the paragraph above: the gap
between the two is the defect, whichever side it opens on.

### There is no repository cross-reference step

Nothing in this sequence validates the repository's own Markdown references: relative links between `governance/`, `contracts/`, and
`decisions/`, `specs/`-shaped path references in tracked text, or same-repository `blob/` URLs.
That gap is real and is recorded, with the withdrawn implementation and the intended replacement,
in [`governance/deferred-verification-work.md`](../governance/deferred-verification-work.md).

It is named here rather than left silent, because a contract that lists only the checks that exist
tells a reader what runs but not what is unguarded.

### Step 6 scope note — added 2026-08-23

**Step 6 does not cover dev-only dependencies.** Measured on this workspace: `cargo deny list`
reports `schemars` (a runtime dependency) and reports neither `jsonschema`, `proptest`,
`fluent-uri`, `referencing`, nor `borrow-or-share`, every one of which is dev-only.

A `MIT-0` licence entered through that subgraph in Phase 005, **passed step 6**, and was caught by
GitHub's dependency-review action instead — which inspects the whole graph.

This is stated here rather than left to the wider reading, because *"Dependency and licence policy —
`cargo deny check`"* reads as covering everything, and for dev-dependencies it does not. A gate that
under-describes its own scope is the same defect class as a step this table omits.

The gap, what would close it, and why the cause is not yet established are recorded in
[`governance/deferred-verification-work.md`](../governance/deferred-verification-work.md).

### Step 8 command note

`gitleaks detect` **was removed in Gitleaks 8.x**. Version 8.30.1 exposes only `git`,
`dir`, and `stdin`. The earlier wording here named a command that no longer exists.

**Both scanners run; neither substitutes for the other.** `gitleaks git` walks commit
history and cannot see uncommitted working-tree content. `gitleaks dir` walks the
filesystem and cannot see content that was committed and later deleted. A secret that
exists in only one of those places is invisible to the other scanner.

Two further properties worth knowing, both verified empirically at T013:

- **`gitleaks dir` does not honour `.gitignore`.** It scans ignored paths too. Reported
  byte counts are *text* volume — binaries are skipped — so a small "scanned" figure does
  not mean a small file set was covered.
- **A `paths` allowlist entry excludes a file *before* scanning it**, producing
  `scanned ~0 bytes` for that file and hiding any real secret in it. Allowlists must be
  scoped by `regexes`/`regexTarget` instead, and every allowlist must be proven narrow by
  injecting a canary credential and confirming it is still detected.

## Fail-closed rule

**A check that cannot run is a failure, never a skip** (FR-023).

Step 1 probes for every **prerequisite** the sequence needs — each tool, and the database
environment the four-row census runs against — and exits non-zero if any is absent, naming what is
missing and what to do about each one (FR-055). The observable contract:

```
$ cargo xtask verify
error: verification cannot run — a required prerequisite is missing

  missing tool: cargo-deny (dependency and licence policy, step 6)
    install: cargo install cargo-deny --locked

  missing environment: RENVOR_TEST_POSTGRES_URL (the PostgreSQL half of the four-row census)
    setup: export RENVOR_TEST_POSTGRES_URL=postgres://user:pass@127.0.0.1:5432/renvor_test

The four rows of PLAN.md §10.1 are not optional, so neither is their evidence.
This is a refusal to verify, not a verification that covered less. Nothing here
starts a database for you: what runs is what you provided.

no checks were run. verification did not pass.
```

**One code for both conditions, deliberately.** A missing tool and a missing database environment
are the same fact — *the machine cannot run this sequence* — and both stop the run before any step
executes. Splitting them would imply a caller could usefully handle one and not the other, which is
not true of a gate whose entire contract is that it ran everything.

The last line matters. A partial run that reports success is the failure mode this contract exists to prevent — an exit code of 0 must mean every step ran and every step passed.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Every step ran and passed |
| 1 | A step ran and failed |
| 2 | A mandatory verification prerequisite is missing — a required tool, or the four-row database environment; no steps ran |
| 3 | The working tree was dirty after a successful run (step 9) |

## Working-tree cleanliness

Step 9 enforces FR-024: after the full sequence, `git status --porcelain` must be empty. This is what proves the ignore rules are correct rather than merely present. Build output, editor state, OS artefacts, and local environment files must all be ignored.

**Historical note, dated.** When this contract was first written the repository contained `.DS_Store`, `.idea/`, and `.playwright-mcp/` while the ignore rules did not cover all of them, and step 11 failed on that account — correctly, because publishing editor and OS artefacts to a public repository is exactly what it should catch.

**That was fixed. It is no longer true, and this contract said otherwise until 2026-08-21.** All three are covered by the tracked root ignore rules, verified with `git check-ignore -v`, with `README.md` as a negative control confirming the probe discriminates rather than reporting everything ignored. **The working-tree step passes at the current head.**

A contract that describes its own subject as currently failing, while the command it governs succeeds, is a false statement in a normative document — the same defect class as a step the table omits. It is corrected here and the correction is dated rather than the sentence being quietly deleted.

## Performance target

Under **10 minutes** on a clean checkout on `ubuntu-latest`. The sequence is a required check on every pull request; if it is slow, small changes become expensive and the gate starts attracting pressure to weaken it.

## Required checks in branch protection

These check names must be listed as required in the protection baseline, so the names are part of the contract:

- `verify (1.94.0)` — full sequence at the declared MSRV
- `verify (stable)` — full sequence at current stable
- `security` — `cargo deny` plus all-target, all-feature clippy with warnings denied
- `docs` — warning-denied, all-feature Rust API documentation

The pull-request-only `dependency-review` job and GitHub's default CodeQL analyses are separate
checks. They are valuable, but they are not hidden inside the required `security` context.

## Consumers

Contributors run it before pushing. CI runs it on every pull request. The Phase 001 evidence pack records a dated run of it. Later phases may change the step list only through a versioned contract change; they must not weaken the fail-closed rule or make any step conditional.
