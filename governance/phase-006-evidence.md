# Phase 006 evidence — persistence, migrations, and the container profile

| Field | Value |
|---|---|
| **Phase** | 006 — persistence (SQLx, PostgreSQL and MySQL) |
| **Branch** | `feat/phase-006-persistence-sqlx` |
| **Closed under** | [W-014](waivers.md) — a *phase-level* waiver. Its records are accepted under **W-013** |
| **Date** | 2026-08-24 |
| **Decision records** | [ADR-0016](../decisions/0016-direct-sqlx-behind-transport-independent-persistence-ports.md), [ADR-0017](../decisions/0017-renvor-owned-unit-of-work-and-the-cancellation-guard.md), [ADR-0018](../decisions/0018-depend-on-sqlx-run-direct-for-migration-on-boot.md), [ADR-0019](../decisions/0019-generated-container-profile-and-the-cache-engine.md) |

> **No independent human review of this phase has occurred, and none is claimed.** Four reviews
> were commissioned and are **automated**, therefore **advisory**. See §6.

## 1. What this phase delivers

Two crates and a generator change.

`renvor-database` declares persistence ports that name no driver. `renvor-sqlx` implements them
against SQLx 0.9.0, behind `db-postgres` and `db-mysql`, neither of which is a default. `renvor new
--database <engine>` emits persistence sources and a reversible migration pair; `renvor new
--container` emits a local development Compose profile to run them against.

Four real engines were exercised at the boundary rather than mocked: **PostgreSQL 17.11 and 18.6,
MySQL 8.4.11 and 9.7.2**.

## 2. The two defects this phase found in its own shipped work

Both had passed the phase's tests and the phase's own review. They are recorded first because they
are the most useful thing in this document.

### 2a. A limitation resting on a wrong diagnosis

`MigrationPolicy::OnBoot` was **recorded and never applied**. `migrates_on_boot()` answered `true`
and the provider migrated nothing, so an operator could read that `true`, deploy, and run against
an un-migrated schema with the framework's own manifest telling them otherwise.

The limitation recorded at the time (L-6) gave the reason as *"sqlx's migration future is not
`Send`"*. That is **not** the obstacle. The obstacle is a higher-ranked region obligation that
survives the coercion to `dyn Future`; making the driver concrete does not help, and neither does
boxing an `async fn` wrapper. Four compile probes with verbatim compiler output are preserved in
the phase evidence. The wrongness mattered: it named a property nobody can change, so the
limitation read as unfixable when it was not.

Closed by [ADR-0018](../decisions/0018-depend-on-sqlx-run-direct-for-migration-on-boot.md).
**L-6 is withdrawn, not re-scoped.**

### 2b. A documented bound that was enforced by nothing

`MigrationSettings::lock_timeout` was stored, validated, and read by no code, while the module
documentation claimed that bounding the lock wait was what this wrapper added over sqlx. Both
engines wait forever by default — `GET_LOCK(?, -1)` and `pg_advisory_lock`, neither honouring a
statement timeout. The documentation was not merely aspirational; it was the opposite of true.

Renvor now takes the lock itself under that deadline and sets `Migrator::set_locking(false)`.

## 3. Verification

| Gate | Result |
|---|---|
| Workspace tests, Rust 1.94.0 (MSRV), `--all-features` | **1290 passed**, 0 failed |
| Workspace tests, Rust 1.97.1 (stable), `--all-features` | passed, 0 failed |
| Both, at **default parallelism** | and with `RENVOR_TEST_REQUIRE_DATABASE=1`, so a skipped database is a failure |
| `cargo xtask verify` | steps **1–10 pass**; step 11 is working-tree cleanliness |
| Real-database suites | PostgreSQL 17.11, 18.6; MySQL 8.4.11, 9.7.2 |
| Real-Docker container matrix | 5 rows, **77 assertions**, all passed (a manual gate; the security posture it checked is now also asserted in the Rust suite) |
| Feature isolation | 0 cross-driver, **with positive controls** |
| Licence and advisory | `cargo deny` clean; 0 `webpki-roots`, 0 `rsa` |
| Secret scan | history and working tree clean |

### The `--test-threads=1` dependency, removed

The real-database suites shared one fixture table each and passed **only** under
`--test-threads=1`, with nothing in the code saying so. An ordinary `cargo test` failed with
fourteen unrelated-looking assertion errors — the shape of failure that gets dismissed as
flakiness. CI happened to pass the flag, so the dependency was invisible.

The requirement is now in the type: the fixture helper returns a guard that must be held. The
suite that drops `_sqlx_migrations` was additionally given its **own database**, because
`--test-threads=1` only serialises within one binary and cargo runs binaries in parallel.

One guard was initially written with a scope smaller than the thing it guarded — bound inside an
inner `block_on` and dropped when that block ended, leaving the rest of the test unprotected while
looking protected in the diff. That is recorded because it is the failure mode of this kind of fix.

## 4. Mutation testing

The `OnBoot` branch and its cleanup guards were mutation-tested: **7 of 10 mutants killed.**

The pass found more in the tests than in the code:

1. A boundedness test that measured elapsed time **after** the call returned — so an unbounded wait
   made it hang rather than fail.
2. A deadline chosen without arithmetic, sitting exactly on the `run_timeout + CLEANUP_TIMEOUT`
   boundary: it passed on PostgreSQL and failed on MySQL.
3. `set_locking(false)` could be flipped to `true` with the **entire** real-database suite still
   green, because both engines' migration locks are re-entrant within one session. Only a
   white-box assertion can catch it.
4. **A spurious kill, withdrawn.** An intermediate cancellation test appeared to kill the
   `close_on_drop` mutant; measurement showed it failed with or without the guard, so it was
   proving nothing. Recorded rather than quietly deleted.

The **three** surviving mutants — M-5, M-6 and M-9, all in the cleanup layer — are argued as
equivalent mutants from upstream source: the failure path discards the whole pool, and `PoolInner`'s
own `Drop` closes it, so per-connection cleanup cannot be distinguished from pool teardown by any
black-box observation. All three guards are retained anyway.

**This section said "8 of 10 … two surviving" until an advisory review compared it against the
evidence it cites.** The figure came from an intermediate run in which M-6 was killed by a test that
was later shown to be measuring nothing, and it was never updated when that kill was withdrawn. The
correction is recorded rather than applied silently: a phase record that rounds its own evidence in
its own favour is worse than one with a lower number.

## 5. What this phase deliberately did not do

- **No ORM.** `Orm` is a one-variant enum so a later phase adds a variant rather than turning a
  flag into a keyword.
- **No SQLite, no SeaORM.**
- **No cache capability.** The generated cache container is local infrastructure only. No
  `renvor-cache`, no client, no application API, no middleware — that is Phase 010, and the
  limitation is stated in the README, in `compose.yaml`, and in `renvor.toml` as
  `cache_wired_into_application = false`.
- **No image digest pinning.** Tags are pinned to a tested patch version; the reasoning against
  digests is in ADR-0019 and carried as L-10.
- **No published crate, tag, release, or deployment.**

## 6. The commissioned reviews

Six ran in total: requirements conformance, security and database threat, dependency and MSRV,
test quality, and two further audits of the container-render and credential surfaces. **All six
returned findings**; none is recorded as NOT PERFORMED.

They found **more than this phase's own testing did**, and the most serious finding was against the
*record* rather than the code.

### 6a. What they found, and what was done

| # | Severity | Finding | Disposition |
|---|---|---|---|
| R1 | **Critical** | The conformance record claimed *"8/10 killed with the two survivors"*; the evidence it cites says **7 of 10** with **three**. The dropped survivor was M-6, the cancellation guard — the one with the most operational consequence | **Fixed.** Corrected in the conformance record and in W-013, both with a written correction rather than a silent edit. The figure had been taken from an intermediate run and never updated |
| T1 | **Critical** | **No automated gate set either database URL.** `support::url` returned `None`, every test returned early, and `libtest` swallows the printed `SKIPPED` for a passing test. The entire real-database suite — including all 30 migration-on-boot tests — reported `ok` in CI having connected to nothing. A revert of this phase's headline fix would have gone green | **Fixed.** CI now starts real PostgreSQL and MySQL, with the **engine pair in the toolchain matrix** so all four supported versions are covered. `RENVOR_TEST_REQUIRE_DATABASE` makes a missing URL a **failure**, so deleting the step breaks the build loudly instead of restoring the gap |
| R5/T2 | High | `a_migration_failure_names_no_credential…` injected its canary with `replace("devpassword", …)`, and `devpassword` appears nowhere in this repository. Worse, had it matched, the DSN would no longer authenticate and the test would assert about a *connect* error | **Fixed.** The canary is gone: the DSN stays valid, the failure is a real checksum refusal, and the secret asserted absent is the operator's actual password. A control asserts the error kind is `MigrationChecksumMismatch`, so a connect failure cannot satisfy it |
| R2 | High | FR-057 (loopback binding) had **no automated assertion** — the cited "five matrix rows" came from a shell script outside the repository, one of whose rows asserted over an empty set | **Fixed.** A real test on the rendered file, with a positive control and a mutation check |
| R3 | High | FR-061 claimed six security properties and asserted **one** | **Fixed.** All six asserted on the rendered file; `no-new-privileges` is counted against the service count so dropping it from one service fails. Mutation-checked |
| R4 | High | The headline test count was identical with and without a database | **Fixed** by T1, and the row rewritten so it no longer leads with the number |
| S1 | High | `renvor docker logs` prints the MySQL container's generated root password, and `root@'%'` authenticates over the published port. The claim that it is *"printed once and not recoverable"* is false — `docker compose logs` re-reads the log | **Fixed.** `MYSQL_ROOT_HOST: localhost`; `root@'%'` is never created, so the logged password grants nothing over TCP. Verified both directions; the application user is unaffected |
| S3 | Medium | Both health checks answered over the **unix socket**, and both entrypoints initialise with networking disabled — so `service_healthy` could release the application against a server not accepting TCP | **Fixed.** `-h 127.0.0.1` on both. The "checked in both directions" claim now covers the direction that occurs |
| S4 | Medium | The kernel's 30s provider deadline is shorter than the migration defaults (60s lock + 300s run), so both migration diagnostics are unreachable under defaults | **Fixed.** `SqlxProvider::required_boot_deadline()` returns the number, with three tests. The interaction is documented where an author will meet it rather than derived from three constants in two crates |
| S5 | Medium | `Database::check()` held a pooled connection and then acquired a second, so `max_connections(1)` — a supported choice — could never boot | **Fixed.** The probe runs on the connection already held |
| S2/R7 | Medium | The cache password is an argv element, visible to `docker inspect`; a comment claimed otherwise | **Claim corrected; exposure recorded as L-11.** The secret test was widened beyond `test:` lines to require every credential mention be a `${...}` reference |
| R6 | Medium | FR-063's *"structurally cannot"* is false — the binary shells out to `docker` | **Restated** as the behavioural claim, which is the one that is tested |
| S6 | Low | The generated `Dockerfile` used floating base tags while the same phase pins database images | **Partly fixed.** The builder is pinned to the MSRV compiler. `distroless` publishes no stable version tag, so the same reasoning that refuses digests applies, and the limitation is stated in the file |
| T5, R10 | Low | `table_exists` was used only negated; the `compile_guard` existed only under `db-postgres` | **Both fixed** — a positive control, and a second guard sharing one assertion helper |
| T4 | High | `sessions()` ended in `unwrap_or(0)`, so an unreadable session view made both the baseline and the reading zero and the leak assertion never ran | **Fixed.** It fails loudly |
| R9, T6 | Medium | The deadline tests asserted "bounded by something", not the **configured** bound | **Fixed.** Lower bounds added, so a deadline collapsed to zero fails |
| — | Low | Adjacent, pre-existing: `PoolSettings::connect_timeout` was validated and applied to nothing | **Fixed.** `sqlx` has no such option, so Renvor applies it to the opening connect |
| D1, D2 | Low | Packaging observations about `publish = false` | **No change.** The publishable set is asserted in CI against an expected list, so a change would be a visible edit |
| T9, R11 | Low | Pre-existing non-hermetic tests (a 1ms deadline; a test hashing the macOS login keychain) | **Recorded, not fixed** — outside this phase, and noted so the "passes twice" claim is read as a statement about machine state |

### 6b. Two further audits, and what they found that the first four did not

Two additional automated audits ran against the pre-fix tree. Both are **advisory**. Both found
things the four above missed, and one of them found the most valuable single finding of the phase.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| P1 | **High** | *"No flag can carry a password"* was a **comment, not a test.** A visible `--database-password` was caught only by the byte-exact `--help` snapshot — which fails as "the surface changed" and prints its own regeneration command, so the reflex fix accepts it. Added with `#[arg(long, hide = true)]`, the pattern five reserved flags already use, it passed **262 tests with zero failures** | **Fixed.** `no_flag_in_the_whole_surface_can_carry_a_credential` reflects over the whole `clap::Command` tree — `hide` removes an argument from the rendering, not from the command — and refuses any long name or alias containing `password`, `secret`, `credential`, `token`, or `apikey`. Verified against the peer's exact case: hidden, the snapshot passes and only this guard fires |
| P2 | **High** | The health-check **content** was asserted nowhere. Reverting `CacheEngine::healthcheck` to the plain `valkey-cli ping` — the form that exits 0 on a wrong password — kept all 27 tests green **and** the Docker matrix green, because the matrix's cache checks are positive-only | **Fixed.** `the_health_checks_are_the_forms_that_can_actually_fail`, mutation-checked in both directions: reverting the cache form fails, dropping `-h` from `pg_isready` fails |
| P3 | Medium | The engine assertion covered the **image** in both directions but the **environment block** in one. Inverting `container_is_postgres` would render `MYSQL_*` keys under a `library/postgres` image — a container that starts and serves nothing the application expects — and every test would pass | **Fixed.** Both directions asserted for both engines |
| P4 | Medium | `sqlx` was absent from `capabilities.rs`'s crate lists, so an accidental `renvor-sqlx` edge from the CLI would have left the suite green — and FR-063 rests on that closure | **Fixed.** `the_executable_reaches_no_database_driver`, with `renvor-database` as the positive control since it **is** a dependency |
| P5 | Medium | ADR-0019 repeated the *"structurally cannot"* over-claim, so it was not a one-document slip | **Fixed** in the ADR, with the correction recorded rather than the sentence quietly replaced |
| P6 | Low | The `${VAR:?}` assertion rendered PostgreSQL only, so the MySQL branch was never exercised; and a bare `${VAR}` — which Compose substitutes empty with only a warning — slipped past a `:-` check | **Fixed.** Both engines exercised; every reference must now carry `:?` |
| P7 | Low | The named-volume claim was asserted for the database and not the cache | **Fixed** |
| P8 | — | FR-062 audited **clean**. The peer went past `PROVENANCE.md` and rebuilt the v3 generator from `git archive 1a83149`, diffing all ten files byte-for-byte; and mutation-killed the compatibility gate by making `[container]` required | **No change.** Their qualification is adopted: v3 lived four commits on an unmerged branch, so the gate protects an intermediate no user ever had. Sound test, narrow value — recorded, not overstated |
| P9 | — | `#[serde(default)]` on `Option<ContainerTable>` is redundant; serde already treats a missing `Option` as `None` | **No change.** An equivalent mutant, recorded so nobody reads it as a dead gate |

The FR-057 and FR-061 findings in that batch were already closed by §6a before the audits reported;
their independent confirmation of the gap is recorded because two audits reaching the same
conclusion from different directions is worth more than one.

### 6c. Two CodeQL alerts: two code fixes attempted, then dismissed

CodeQL raised **two high-severity `rust/cleartext-logging` alerts** against this branch, both
attributed to `crates/renvor-sqlx/tests/provider.rs` — the macro invocation lines, since CodeQL
attributes expanded code to its call site. The operation is:

```rust
let provider = provider(ConnectionString::new(format!(
    "postgres://user:{}@db.internal:5432/app",
    support::CREDENTIAL_CANARY
)));
let rendered = format!("{provider:?}");
assert!(!rendered.contains(support::CREDENTIAL_CANARY));
```

The alert says *"This operation writes secret to a log file."* Four facts, each checked rather than
asserted:

1. **It writes to no log.** The test contains zero `println!`, `eprintln!`, `tracing::` or `log::`
   calls. `format!` returns a local `String`.
2. **The file is not shipped.** The published `renvor-sqlx` crate is `src/*.rs`, two licences and a
   README — thirteen files, confirmed against the built `.crate` archive. `tests/` is excluded by
   the `include` list.
3. **The value is not a credential.** It is a hardcoded canary chosen to be unmistakable.
4. **The flagged operation is the verification of the property CodeQL is worried about.** The next
   line asserts the canary is *absent* from the rendered output. The test exists to prove that a
   provider's `Debug` cannot carry a DSN.

So it is a false positive on unshipped test code, where the flagged line is the proof rather than
the leak.

**Both were dismissed as false positives on 2026-08-24, on the maintainer's explicit
instruction — and only after two attempts to fix the code instead.**

That sequence is the record, so it is written down rather than summarised as a dismissal:

1. **A correction.** This section first said CodeQL "is not among the required status checks", on
   the strength of `required_status_checks.contexts` listing only `verify (1.94.0)`,
   `verify (stable)`, `security` and `docs`. That was wrong. GitHub's **code-scanning merge
   protection** is a separate mechanism that does not appear in that list, and it blocked the merge.
2. **First attempt — remove the planted credential.** The test was rewritten from "assert one
   canary string is absent" to a **structural** assertion: `SqlxProvider`'s `Debug` declares four
   fields, and the rendering must carry those four and no fifth. Mutation-checked both ways — a
   fifth field carrying the DSN fails, and so does a harmless one. Strictly stronger than what it
   replaced. **The alert persisted.**
3. **Second attempt — remove the URL shape.** The rewrite had kept
   `postgres://host:5432/name`. It was replaced with a plain marker. **The alert persisted.**
4. **The trigger identified.** CodeQL classifies the location as `["test"]` and still calls the
   literal `zz-connection-marker-zz` a secret, so it keys on the `ConnectionString` **type name**
   rather than on any value. The only untried code-level move was renaming a public type to satisfy
   a scanner heuristic, which was rejected.

Nothing was bypassed to achieve this. `--admin` and `--auto` were both offered by `gh` at the
refused merge and both were declined; dismissal is the path GitHub provides for a false positive,
it is reversible, and every required check passed on its own merits before it was used.

**The two rewrites are kept.** They were undertaken to clear the alert and did not, but both tests
are better than the ones they replaced — and the second rewrite exposed a third test,
`a_seed_report_carries_no_credential`, that built its statement from `CREDENTIAL_CANARY.len()` (the
integer 22) and therefore asserted the absence of a string that never entered the seed, the
database, or the report. That assertion could not fail under any mutation. It is now non-vacuous
and mutation-checked.

The precedent for handling a scanner false positive in this repository is `.gitleaks.toml`, whose
policy requires every allowlist entry to be **narrow and individually justified**, naming the rule,
the exact location, why the match is not a secret, and what would make the entry removable. The
paragraphs above are that justification; the entry itself is the maintainer's to make.

**Automated review is not independent review.** Constitution §Development and Phase Workflow #4 and
`PLAN.md` §6.1 step 10 require a review by a qualified person other than the author. No such review
has occurred. The reviews recorded below were performed by automated agents, are **advisory**, and
do not satisfy that requirement. A review that returned nothing is recorded as **NOT PERFORMED**,
never as a pass.

## 7. Limitations

Carried forward with owners and removal conditions in
`specs/006-persistence-sqlx/evidence/fr-conformance.md`: **L-5, L-7, L-8, L-9, L-10, L-11**.
**L-6 is withdrawn.**


**Erratum (2026-09-06).** The CI context `verify (stable)` named in this record — and every `platform (…, stable)` context — compiled with the pinned **1.94.0**, not with current stable, from `98a4e2c` (2026-08-11) until the fix in pull request #64; only three runs (pull request #63's) were inspected directly, the window is inferred from configuration history, and every locally recorded `cargo +stable xtask verify` leg was genuine. See `phase-011-evidence.md` §14. This note is appended; nothing above it is edited.
