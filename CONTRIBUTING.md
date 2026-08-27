# Contributing to Renvor

Thank you for considering a contribution.

**Renvor is pre-release and unpublished.** Phase 002 delivered a working
transport-independent kernel — lifecycle, provider resolution, layered configuration, health,
and failure injection. It has no transport yet, so it cannot serve a request. Every API is
explicitly unstable and expected to change once the first transport adapter exercises it.

## Before you start

Read [`GOVERNANCE.md`](GOVERNANCE.md) for who decides what, and
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) for the behaviour expected of everyone here.

## The one command

```sh
cargo xtask verify
```

That is the entire verification sequence — formatting, lint, tests, API documentation,
dependency and licence policy, secret scanning, documentation build, link checking, and
working-tree cleanliness. **CI runs this exact command.** Nothing in a workflow file
duplicates it, because duplicated steps are how local and automated verification silently
drift apart.

Run it before you open a pull request. If it passes locally it should pass in CI.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Every step ran and passed |
| `1` | A step ran and failed |
| `2` | A required tool, or the database environment, is missing — **no steps ran** |
| `3` | The working tree was dirty after an otherwise successful run |

**Exit code 2 is not a pass.** A check that cannot run is a failure, never a skip. If you
see it, the output names everything that is missing and what to do about each one.

### Tooling you need

`cargo xtask verify` probes for all of these and refuses to run without them:

| Tool | Install |
|---|---|
| rustfmt | `rustup component add rustfmt` |
| clippy | `rustup component add clippy` |
| cargo-deny | `cargo install cargo-deny --locked` |
| gitleaks | `brew install gitleaks` (or see the gitleaks project) |
| lychee | `cargo install lychee --locked` |
| node, npm | see `.nvmrc` |

The toolchain itself is pinned by `rust-toolchain.toml`; rustup will fetch it for you.

### Databases you need

`cargo xtask verify` runs the **four-row persistence census** — every persistence suite against
PostgreSQL and MySQL, through both the direct-SQLx and the SeaORM adapter. Those are the four rows
`PLAN.md` §10.1 makes first-class, and they are not optional, so the command **refuses to run**
without them:

| Variable | What it is |
|---|---|
| `RENVOR_TEST_POSTGRES_URL` | connection string for a PostgreSQL the suite may create and drop tables in |
| `RENVOR_TEST_MYSQL_URL` | connection string for a MySQL, on the same terms |
| `RENVOR_TEST_REQUIRE_DATABASE` | set to `1`. Turns a *skipped* real-database test into a failing one |

Any container runtime will do. CI pins `postgres:17.11-trixie` and `mysql:8.4.11`; matching those
locally means a portability difference fails on your machine rather than in review.

Use a throwaway database. The suite creates and drops its own tables, and the upgrade suite
deliberately migrates a schema from a previous release.

**Two properties of these URLs are not obvious, and each one costs a full gate run to discover.**

1. **The user needs `CREATE DATABASE` globally**, not just rights on the named database. The
   suites create a fresh database per test rather than sharing one. In the official MySQL image the
   `MYSQL_USER` account is granted rights on `MYSQL_DATABASE` **only**, so a URL using it fails
   every migration test with `ConnectFailed` — the create is refused, and the connection to the
   database that was never created is what actually reports. Use an account with global rights.
2. **The password must not be a substring of `renvor-sqlx`, `renvor-seaorm`, `postgres` or
   `mysql`.** `a_failed_boot_publishes_nothing_and_leaks_no_credential` extracts the real password
   from your URL and asserts it does not appear in a rendered startup diagnostic. That diagnostic
   legitimately names the adapter crate, so a password of `renvor` matches inside `renvor-seaorm`
   and the test reports *"the password reached a diagnostic"* when nothing leaked. The test is
   right to fail closed on a substring match; pick a password that shares no substring with the
   safe tokens.

**Why this is a refusal rather than a skip.** Until Phase 008 the census printed `ok — NOT RUN`
without these and the whole sequence still exited 0 — a check that did not run, reported as a check
that passed. That is the third time this repository has been bitten by that exact shape. The third
variable exists for the same reason one level down: without it the test harness skips every
real-database test and still prints `ok`.

**Nothing is started for you.** `cargo xtask verify` never launches a database: what it verifies is
the machine you actually have.

## Supported Rust versions and platforms

The minimum supported Rust version is **1.94.0** — a fixed floor, not a rolling offset
from stable. CI tests exactly two toolchains: the pinned `1.94.0` and the current stable
channel, resolved by CI at run time.

**Linux, macOS, and Windows are supported.** Six platform/toolchain contexts run on every
pull request, but **only `verify (1.94.0)` and `verify (stable)` are required by branch
protection** — the four `platform (…)` contexts are evidence, not gates. Do not describe all
six as required checks.

The normative policy is [`contracts/support-policy.md`](contracts/support-policy.md);
[`SUPPORT.md`](SUPPORT.md) is the human-facing summary.

Do not use an API newer than the floor. `clippy.toml` sets `msrv = "1.94.0"` so clippy
will not suggest one.

## Pull requests

- **A pull request is required.** The default branch cannot be pushed to directly, by
  anyone, including administrators.
- All required checks must pass: `verify (1.94.0)`, `verify (stable)`, `security`, `docs`.
- Keep the change focused. A pull request that does one thing is reviewable; one that does
  five is not.
- Explain *why*, not only *what*. The diff already shows what changed.

**On approvals:** the project currently has a single maintainer, so pull requests are
merged without a second person's approving review. This gap is recorded openly as waiver
**W-001** in [`governance/waivers.md`](governance/waivers.md), with an absolute expiry
date and the full verification sequence plus every scanning gate as its compensating
controls. It is a documented exception, not an unexamined habit.

## Dependency updates

The dependency policy exists because an unreviewed dependency change is an unreviewed code
change with extra steps.

**The authoritative, machine-readable licence and dependency policy is
[`deny.toml`](deny.toml).** It is enforced by `cargo deny check` in verification step 6.
This document deliberately does **not** restate the allow-list — a prose copy drifts from
the enforced one, and reviewers then trust the wrong list. Read `deny.toml`.

Rules:

1. **Updates arrive as reviewable pull requests.** Dependabot covers the `cargo`,
   `github-actions`, and `npm` ecosystems. Every update is reviewed like any other change.
2. **Wildcard version requirements are denied.** A wildcard means "whatever resolves
   today", which is an unreviewed floating update by another name.
3. **No git or path dependencies in a publishable package.** `xtask` is exempt because it
   declares `publish = false`.
4. **crates.io only.** An unknown registry bypasses the immutability guarantee and the
   audit trail that comes with it.
5. **Adding a licence to the allow-list is a policy change**, reviewed like any other, not
   a convenience edit to get CI green.
6. **Lockfiles**: committed for applications, release tooling, automation, and the
   documentation site; not committed for reusable library crates.

### Security advisories

An advisory against a dependency is on a clock from the moment it is confirmed. The
authoritative policy is
[`governance/dependency-advisory-policy.md`](governance/dependency-advisory-policy.md);
this is a summary.

| Condition | Triage within | Remediate within |
| --- | --- | --- |
| Known active exploitation | 24 hours | Begin immediately; decision within 24 hours |
| Critical | 24 hours | 7 calendar days |
| High | 48 hours | 14 calendar days |
| Medium | 5 calendar days | 30 calendar days |
| Low | 10 calendar days | 90 days, or the next prerelease, whichever is first |

- **Severity is not the CVSS score alone.** Reachability, exploit maturity, known
  exploitation, and actual exposure all count, and the reasoning is recorded.
- **No upstream fix does not buy more time.** Remove, disable, replace, or isolate the
  dependency, or block the release.
- **Critical and High cannot be waived** for a public release.
- **Every advisory gets a dated record.** Adding an identifier to `deny.toml`'s ignore list
  without one is a policy violation, not a configuration choice.

This concerns advisories against **dependencies**. Reporting a vulnerability *in Renvor*
follows [`SECURITY.md`](SECURITY.md), which has its own and different timetable.

If a new dependency is genuinely needed, say in the pull request what it does, why a
smaller option or the standard library will not serve, and what its licence is.

## Licensing of your contribution

Renvor is licensed under **`MIT OR Apache-2.0`**, at the recipient's option.

> Unless you explicitly state otherwise, any contribution you intentionally submit for
> inclusion in this project, as defined in the Apache-2.0 licence, shall be dual licensed
> as **MIT OR Apache-2.0**, without any additional terms or conditions.

No separate contributor licence agreement is required. Submitting a pull request is your
agreement to the terms above.

## Code that Renvor generates for a user is *not* covered by this

**Project code generated for you by Renvor tooling carries no Renvor licensing
obligation.** It is yours outright, to license however you choose, including
commercially and including under a proprietary licence.

Generated output must not embed a Renvor licence header implying otherwise. If you find
generated output that does, that is a bug — please report it.

The dual licence above governs **Renvor's own source and documentation**, not the
applications people build with it.

## Security issues

Do not open a public issue. See [`SECURITY.md`](SECURITY.md) for the private reporting
path and response commitments.

## Decision records

Substantial architectural decisions are recorded in [`decisions/`](decisions/) using the
template at [`decisions/0000-template.md`](decisions/0000-template.md). A decision with no
rejected alternatives was not a decision — the template requires them, and so do reviewers.
