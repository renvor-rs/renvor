---
description: "Contract — the ordered verification sequence `cargo xtask verify` runs"
version: "1.0.0"
status: "normative — enforced executably by `xtask`. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract: Verification Sequence

**Feature**: Phase 001 — governance foundation | **Satisfies**: FR-018, FR-019, FR-022, FR-023, FR-024, FR-025, FR-037, FR-055

One command, one behaviour, locally and in automation:

```
cargo xtask verify
```

CI invokes the same entry point. That is the point — duplicated shell steps in workflow files are how local and automated verification silently diverge, and divergence is how a skipped check gets reported as a pass.

## Steps

Executed in order. None is conditional. None is skipped.

| # | Step | Command | Toolchain required |
|---|---|---|---|
| 1 | Toolchain probe | — | Rust (pinned), and Node LTS when step 7 is in scope |
| 2 | Formatting | `cargo fmt --all --check` | Rust |
| 3 | Lint | `cargo clippy --all-targets --all-features -- -D warnings` | Rust |
| 4 | Tests | `cargo test --workspace --all-features` | Rust |
| 5 | API documentation | `cargo doc --workspace --no-deps` with warnings denied | Rust |
| 6 | Dependency and licence policy | `cargo deny check` | `cargo-deny` |
| 7 | Secret scan | `gitleaks git . --no-banner` (history) **and** `gitleaks dir . --no-banner` (working tree) | `gitleaks` |
| 8 | Documentation site | `npm ci && npm run build` in `docs/` | Node LTS |
| 9 | Link check | `lychee` over the built documentation output | `lychee` |
| 10 | Working-tree cleanliness | assert no untracked or modified files remain | Rust |

### Step 7 command note

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

Step 1 probes for every tool the sequence needs and exits non-zero if any is absent, naming what is missing and how to install it (FR-055). The observable contract:

```
$ cargo xtask verify
error: verification cannot run — required tooling is missing

  missing: lychee (link checking, step 9)
    install: cargo install lychee --locked

  missing: node (documentation site, step 8)
    install: see .nvmrc for the required version

no checks were run. verification did not pass.
```

The last line matters. A partial run that reports success is the failure mode this contract exists to prevent — an exit code of 0 must mean every step ran and every step passed.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Every step ran and passed |
| 1 | A step ran and failed |
| 2 | A required toolchain is missing; no steps ran |
| 3 | The working tree was dirty after a successful run (step 10) |

## Working-tree cleanliness

Step 10 enforces FR-024: after the full sequence, `git status --porcelain` must be empty. This is what proves the ignore rules are correct rather than merely present. Build output, documentation output, `node_modules/`, editor state, OS artefacts, and local environment files must all be ignored.

**Known starting condition**: the repository currently contains `.DS_Store`, `.idea/`, and `.playwright-mcp/`, and the existing `.gitignore` does not cover all of them. Step 10 fails until the ignore rules are corrected — correctly, because publishing editor and OS artefacts to a public repository is exactly what it should catch.

## Performance target

Under **10 minutes** on a clean checkout on `ubuntu-latest`. The sequence is a required check on every pull request; if it is slow, small changes become expensive and the gate starts attracting pressure to weaken it.

## Required checks in branch protection

These check names must be listed as required in the protection baseline, so the names are part of the contract:

- `verify (1.94.0)` — full sequence at the declared MSRV
- `verify (stable)` — full sequence at current stable
- `security` — `cargo deny`, dependency review, CodeQL, clippy SARIF upload
- `docs` — documentation build and link check

## Consumers

Contributors run it before pushing. CI runs it on every pull request. The Phase 001 evidence pack records a dated run of it. Later phases extend the step list but must not weaken the fail-closed rule or make any step conditional.
