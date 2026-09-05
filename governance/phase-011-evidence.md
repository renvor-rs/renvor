# Phase 011 — Evidence

**Phase**: 011 — Generators for the auth starter and the five capabilities, the resource and
migration generators, and the testing kit
**State**: **at the review/merge-authority checkpoint — implemented, unmerged, not closed.** No
publication, tag, release, or deployment; no waiver created or granted; every decision record this
phase touches stays `proposed`. Closure of W-023 and W-024 is recorded in `waivers.md` only after
the validation review in `phase-011-review-record.md` and only against the head and tree named in
§10 below.
**Base**: `4f383005851809802fb91cc4cc97972689b1c58b` (origin/main, after PR #60), tree
`e77cb1b6c9fc4100a502b9c48fb9a3385c3716eb`
**Branch**: `feat/phase-011-generators-testing-kit`
**Closure head**: `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0` (tree `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9`) — the last implementation commit; the checkpoint head adds the closure records and the ledger parser's closed-row rule (§10)
**Working record**: `specs/011-generators-auth-starter-testing-kit/` — spec, clarifications, plan,
data model, threat model, package research, and one evidence file per batch; **gitignored**,
`git ls-files specs` = 0. This file and its companions (`phase-011-limitations.md`,
`phase-011-mutation-ledger.md`, `phase-011-dependency-inventory.md`, `phase-011-review-record.md`)
are the clone-visible mirror.

## 1. What this phase added

**The generator honours the two governed choices it could not honour before (W-023, W-024).**
`renvor new` asks for — and `--auth none|session` and `--capabilities <list>|none` accept — the auth
starter and the five capabilities, through one configuration model (`ProjectConfiguration::resolve`)
that the wizard and the flags both feed. Every choice is persisted canonically in `renvor.toml`
(`[auth]`, `[capabilities]`, `[framework]`) and read back by `renvor check`. Unsupported values fail
explicitly and name why (`api`/`full`: `unsupported_value`, `reason = no_token_issuance_route` —
Phase 009 ships refresh, not issuance, so no mode is invented to fill the list); unsupported
combinations fail before any write (`session` without a database or without `mail`; `jobs`
without a database; a selection needing the framework without `--framework-path`; `--container-cache
none` beside `cache`). Nothing inert is ever recorded.

**A framework-backed starter.** With `--framework-path`, the generated project depends on the
workspace crates by path, seeds its resolution from the framework's lockfile, and is generated with
the auth starter (`renvor-auth` + `renvor-auth-http`: registration, login, `/auth/me`, deny-by-default
authorization inside the operation, CSRF-protected logout, verification and recovery with
indistinguishable responses) and each selected capability wired through the facade's `capability-*`
features and the adapter features (`cache` → Valkey, `jobs` → the four-row job store with its
embedded migrations, `mail` → SMTP, `storage` → a `cap-std` filesystem root, `observability` → JSON
logs, bounded metrics, OTLP/HTTP traces). The start-up order is real: the database provider is
available before `AuthEndpoints` or any pool-dependent route is constructed (`routes_deferred` +
`configured_at_boot`, this phase). Unselected capabilities appear nowhere in the tree
(`assert_recorded`).

**Every generated starter proves itself.** Generation verifies the staged project in a sealed
environment (`fmt --check`, `clippy --all-targets -D warnings`, `build`, `test`, and a route-dump
smoke that the binary must answer) before anything reaches the destination, and the placed project
carries `tests/starter.rs` + `tests/support/mod.rs`: start the binary on a free port against the
real services, migrate, seed, register, log in, authorize, refuse, log out, confirm the verification
mail from the sink, exercise each capability, interrupt with a real `SIGINT`, and assert the clean
exit. Users come from `renvor_testkit::factory`; the loopback client is `renvor_testkit::client`.

**Generators for a placed project.** `renvor generate migration <name>` (a UTC-versioned reversible
pair; a rerun reuses the pair), `--import auth|jobs` (the engine's embedded set into the project's one
directory — 010/L-7), `renvor generate resource <Name> field:type…` (module, migration pair, test,
marker edits, `rustfmt` at generation), and `renvor generate auth` (the auth starter into a placed
starter, refusing where `new` would). Every one plans against `.renvor/generated.toml` (generator
version, template version, SHA-256 per file): absent → written, byte-identical → no-op, untouched
since generation → regenerated, changed by the author → `generation_conflict` naming the paths and
writing nothing; commits go through a temporary sibling and a rename, record last.

**The testing kit.** `factory` (deterministic `Sequence`, `Factory`, `UserFactory`, `ItemFactory`),
`app::TestApplication` (boots an `ApplicationBuilder` with the caller's providers and dispatches
requests through the registry without a socket — `renvor-auth-http`'s own suite now runs on it),
and `client` (a blocking loopback client over `minreq`, for tests that spawn a binary).

**Framework changes this needed**: embedded migration sets (`renvor_auth::migrations`,
`renvor_jobs::migrations`, counted by tests); `routes_deferred`/`DeferredEndpoints` (503 until set)
and `HttpServerProvider::configured_at_boot`; `boot_deadline_for` on both persistence providers;
`UserRepository::mark_email_verified` (a Phase 009 defect: the flag never became true — RED first,
proven on all four rows); `from_forwarded` reads every parameter (010/L-17); `renvor-mail` and
`renvor-storage` warning-free without their adapter feature (no `allow(dead_code)`, no lint
weakened); the jobs README's migration count pinned by a test.

**Contracts**: `command-surface.md` 1.2.0, `json-output.md` (`generation_conflict`, 21 codes),
`project-manifest.md` 1.1.0, `template-contract.md` 1.1.0 (verbatim files, starter sets, snapshot
policy, provenance record), `generation-transaction.md` (the sealed verification environment),
`verification-sequence.md` 2.3.0 (census 86; step 4's general run with
`RENVOR_TEST_STARTER_ROWS=none`; `cargo xtask census` for the rows).

## 2. W-023 and W-024 — every removal-plan control, and the test that executes it

| Control | W-023 (the auth starter) | W-024 (the five capabilities) |
|---|---|---|
| (1) unsupported inputs fail explicitly, never recorded as inert | `api_and_full_are_unsupported_values_not_reservations` (code, `supported`, prose, and the machine-readable `reason` — M-A1, M-A2); `every_governed_choice_of_principle_seven_is_classified` now classifies the auth starter as `Honoured(--auth)`; the `RESERVED` table no longer names `--auth` (pinned by its own test) | `an_unknown_capability_is_refused_naming_the_five`, `a_duplicate_capability_is_refused_and_none_with_a_name_is_refused` (M-A3, M-A4); the governed-choice test classifies capabilities as `Honoured(--capabilities)` |
| (2) wizard/flag parity, validated `renvor.toml` persistence, real wiring, compile and start | parity: `parity::a_wizard_run_and_a_flag_run_for_a_starter_produce_byte_identical_projects` (a real pty, `y` to confirm); persistence: `a_valid_starter_selection_resolves_and_records_everything`, `the_equivalent_command_carries_every_answered_choice`, the `manifest-v7-*` snapshots, `renvor check` reads `[auth]`; wiring and start: every `*_with_everything_generates_and_proves_itself` row and `authentication_with_only_its_mail_generates_and_proves_itself` (`assert_recorded`: `src/auth.rs` present, `renvor-auth` in `Cargo.toml`, `renvor-auth-http` reachable in the lock closure exactly when chosen, the auth migrations applied, register → login → `/auth/me` → 401/403/204 → logout) | the same parity and persistence tests (`[capabilities]`); `the_cache_capability_with_containers_wires_the_container_cache` (M-A10); wiring and start: the five lean rows (`the_cache_alone…`, `mail_alone…`, `storage_alone…`, `observability_alone…`, `a_starter_without_a_database…`) and the four full rows, each `assert_recorded` for presence of the selected and absence of the unselected — in `renvor.toml`, in `Cargo.toml`, on disk, and in the **lock closure walked from the manifest's runtime dependencies** (`lock_closure`, FR-024) |
| (3) the four-row and capability combinations, censused | `sqlx_postgres_…`, `sqlx_mysql_…`, `seaorm_postgres_…`, `seaorm_mysql_…` `_with_everything_generates_and_proves_itself` — auth on each row, the authenticated flow end to end; 19 `renvor-cli` rows in `ROW_EVIDENCE` (§4) | the same four rows carry `jobs` with its migrations applied on each engine; `cache` against Valkey 9.1.1, `mail` against Mailpit 1.29.1 (the verification mail read from its API), `storage` against an isolated root, `observability` against a local OTLP receiver and `/metrics`; censused (§4) |
| (4) no tag, release, deployment, or publication while active | unchanged: nothing published; the `renvor` facade still says so | unchanged |

Everything above is executable; the three negative controls in §4 prove the census notices a row
that stops reporting. The validation review of this table is in `phase-011-review-record.md`.

## 3. The covering matrix

| Row | Database / ORM | Auth | Capabilities | Domain | Proves |
|---|---|---|---|---|---|
| `pgsqlx` | PostgreSQL / SQLx | session | all five | example + seed | the full flow on the first row |
| `mysqlx` | MySQL / SQLx | session | all five | example + seed | the same on MySQL (the InnoDB lock-first trap, Phase 010, stays fixed) |
| `pgsea` | PostgreSQL / SeaORM | session | all five | example + seed | the SeaORM entity/repository layout |
| `mysea` | MySQL / SeaORM | session | all five | example + seed | the same on MySQL |
| `authonly` | PostgreSQL / SQLx | session | mail only | none | auth without the example domain |
| `cacheonly` | none | none | cache | none | Valkey wiring with no database |
| `mailonly` | none | none | mail | none | SMTP wiring alone |
| `storageonly` | none | none | storage | none | the filesystem root alone |
| `observeonly` | none | none | observability | none | logs, metrics, and an OTLP export at shutdown |
| `nodb` | none | none | none | none | the framework-backed starter with nothing selected |
| refusals | — | — | — | — | `every_invalid_combination_is_refused_before_any_write` |
| determinism | PostgreSQL / SQLx | session | all five | example | dry-run == real (`a_dry_run_of_a_starter_matches_the_real_run_and_writes_nothing`), byte-identical twice and a rerun a no-op (`a_starter_generated_twice_is_byte_identical_and_a_rerun_changes_nothing`), failure after verification leaves the destination absent |
| `ressqlx`, `ressea` | PostgreSQL / SQLx, SeaORM | session | mail | example | `renvor generate resource` into a placed starter, proven live |
| `authadded`, `authrefused` | PostgreSQL / SQLx | added later | mail | example | `renvor generate auth` into a placed starter; refused where `new` refuses |
| parity | PostgreSQL / SQLx | session | all five | example | the wizard through a real terminal equals the flags, byte for byte |

Every row runs in `crates/renvor-cli/tests/starter_matrix.rs` (`parity.rs` for the last), against
rv-postgres 17.11, rv-mysql 8.4.11, rv-valkey 9.1.1, and rv-mailpit 1.29.1, with
`CARGO_INCREMENTAL=0` and a shared absolute `RENVOR_TEST_TARGET_DIR`.

## 4. The census on the checkpoint head, and its negative controls

**The census** (`cargo xtask census`, step 4 of the verification sequence run over every four-row
suite with the starter rows enabled), on head `d8e3a445363da965f40470b12100082d02c68254`, tree
`ea7e3a52d0db8198b14508041e1622a327780317`, 2026-09-05 06:46:58–06:58:19, `CARGO_INCREMENTAL=0`,
every `RENVOR_TEST_*` variable set (both `REQUIRE` flags and the Mailpit API included, so a skipped
live proof fails rather than passes): **`[4/9] tests (four-row persistence census): ok — all 86
required suites reported in`** — `renvor-auth-http` 0m11s, `renvor-cli` 10m09s (the eighteen
`starter_matrix` rows — ten covering rows, the refusals, three determinism proofs, four generator
rows — and the parity row: nineteen of the 86), `renvor-seaorm` 0m31s, `renvor-sqlx` 0m28s; exit 0. The
only tracked files modified at the time were three governance/README documents (no build input);
the code was exactly the committed tree. Log: `census-final.log` (scratch, quoted here).

**Re-run on the closure head** `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0` (tree
`d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9`, = `d8e3a44` + the publication-order fix + the
manifest-comment pin test), 07:36:43–07:48:36, the same environment: **all 86 required suites
reported in**, exit 0 — `renvor-cli` 10m57s, `renvor-seaorm` 0m22s, `renvor-sqlx` 0m20s
(`census-h1.log`). The three negative controls are cited from `d8e3a44`: the delta to the closure
head is a workflow line, `RELEASING.md`, and one unit test, none of which the census or the row
table reads.

**The three negative controls** (FR-032), run by `census-controls.sh` (scratch): the storage row's
`row!(…)` invocation is renamed, then deleted, then gated behind `#[cfg(feature = "never-set")]`;
after each edit the census runs and the file is restored from git (the trap guarantees it), and the
census must fail naming that row. Run twice — on `3df5589` (10m46s / 9m51s / 9m54s) and, after
the corrections, on the checkpoint head `d8e3a44` (2026-09-05 06:58:49–07:29:20) — all three fired
each time:

| Control | Census outcome on `d8e3a44` | `renvor-cli` group |
|---|---|---|
| rename | exit 1 — `[4/9] tests (four-row persistence census): FAILED — row renvor-cli::storage_alone_generates_and_proves_itself did not report in. Expected the line test storage_alone_generates_and_proves_itself ... ok from starter_matrix …` | 10m13s |
| delete | exit 1 — the same line | 9m42s |
| cfg-gate | exit 1 — the same line | 9m56s |

The census reports the *first* row that did not report, so a control run proves the mechanism,
not the other rows; the other rows' proof is the full census above on the same head. The matrix
file was restored from git after each control (`git status` clean for it; the trap guarantees it).
Logs: `census-control-{rename,delete,cfggate}.log` (scratch, quoted here).

A first attempt at the rename control (on `9dd5bc4`) edited a `fn` that does not exist — the rows
are a macro — so its census was effectively unmodified, and it is what caught the full-row
regression in §5.

## 5. Defects found, and by what

- **The full rows regressed and the census control caught it.** After the generated tests were
  switched to the testkit factories (`afab721`), only the generator rows were re-run; the four full
  rows failed to compile their own test (`let session: Vec<…>` shadowing the new `session()` helper,
  E0618 ×2). The first census control — a no-op rename, so an unmodified census — reported it.
  Fixed in `63ab6f8` (one binding renamed, two lines reflowed for `rustfmt`), re-proven on all four
  rows (223 s, 18 passed). Recorded in `phase-011-limitations.md` §Closed in this phase and in the
  memory that governs template edits.
- **`email_verified` never became true** (Phase 009): `confirm_verification` consumed the token and
  wrote nothing to the user row. `UserRepository::mark_email_verified` added, RED first, proven on
  all four rows, mutation M-B-01.
- **The verification mail was never sent by the starter** until `/auth/verification/resend` was
  wired; the OTLP batch was lost at drop until the telemetry is shut down explicitly; `/metrics` was
  empty until a start-time gauge existed; the seeds raced the first request until `SeedProvider`.
- **Two entry findings** (Phase 010's review): the jobs README said four pairs (five shipped) — now
  a test reads both; `renvor-mail`/`renvor-storage` warned without their feature — the variant and
  its arms are gated on the feature that constructs them; xtask step 7 compiles each port crate
  featureless under `-D warnings` with a control that must fire.
- **M-A1 survived**: the auth refusal's machine-readable `reason` was unpinned; pinned.
- **The lock-closure walk (FR-024, added after the first validation pass) found `renvor-auth` in
  a mail-only starter's graph** — reachable through the persistence adapters, which implement
  its repositories, in every database-backed starter whether or not auth was chosen. The
  capability crates are absent when unselected, as required; the auth domain crate is not, and
  that is recorded as limitation L-13 and asserted exactly (`renvor-auth-http` follows the
  choice; `renvor-auth` follows the database).
- **The first gate leg on the closure records' commit (`83faf1e`) failed step 4** on three
  file-set tests never run by hand during the phase — `container::each_selection_generates_exactly_its_file_set`
  and two in `seaorm.rs` — each expecting a generated tree without `.renvor/generated.toml`, the
  provenance record every project has carried since batch F. The expectations were corrected
  (`9dd8d13`), every `renvor-cli` test binary was run with the rows off (446 passed), and both legs
  were run again on that head (§10). The lesson is the same as the template one: a change to what
  the generator emits is proven by every binary that pins the emission, not by the ones the author
  remembers.
- **The first gate leg on `9dd8d13` reached step 8 and failed the history secret scan**: one
  `generic-api-key` finding in the starter test template at commit `9dd5bc4` — the made-up
  password of the example user the generated test registered to prove authorization refusal, a
  JSON fixture that authenticates nobody. Batch F had already replaced every fixed user with
  factory-drawn values (`afab721`), so the working tree was clean and the history was not.
  Allowlisted as FP-005 in `.gitleaks.toml` by the fixture line's address and key, not by path or
  commit and without the value; verified by a canary beside a copy of the line still being
  reported. The history scan is clean; both legs re-run (§10).
- **PR #62's Windows platform legs failed twice, on the `nodb` row and then on a unit test.**
  First (`03a3e8d`): on Windows `std::fs::canonicalize` answers with a verbatim path
  (`\\?\D:\…`), the generator rendered it into the starter's `Cargo.toml` as a path dependency,
  and cargo refused it (`invalid path url`) in staging — a defect no macOS or Linux run can see.
  Fixed in `f95ab6b` by removing the prefix at the canonicalisation site (`without_verbatim_prefix`,
  pure text, RED first, tested on every platform; M-F11). Second (`f95ab6b`): the model's own
  test compared the recorded path with the raw canonical form, prefix included; the expectation
  now strips it too (`2df9f81`). Third (`2df9f81`): generation and the generated test's own run
  passed on Windows up to the last assertion — a clean exit after the interrupt — which the Windows
  stop path can never satisfy, because a test there cannot send its child a SIGINT and the template
  ended the process with `kill()`. `stop()` now returns `None` where no interrupt can be sent and the
  generated test prints `SKIPPED` for that assertion instead of faking a status (limitation L-10).
  Both local legs were run again on each of these heads.
- **The second validation pass found a release-order regression** (`publication_order_is_topological`):
  the testkit's optional `renvor-http` edge put it before `renvor-openapi` in the publication list.
  Fixed in `8c09835` (`release-dry-run.yml`, `RELEASING.md`).
- **Feature-gated rustdoc links** (`EntropySource`, `ApplicationBuilder`) resolved at the crate root
  because of the outer `///` on the modules; qualified. The gate's step 9 does not run
  `--all-features` (a known gap); `RUSTDOCFLAGS=-D warnings cargo doc -p renvor-testkit --all-features
  --no-deps` was run by hand after the fix (06:05, green) and again by the second validation pass
  (07:07, green).

- **Three defects the correction round's own proofs surfaced (2026-09-05), none of them a
  Codex finding.** (1) The SeaORM repository template's `create` signature was split across lines
  with a conditional `owner` parameter; without auth `rustfmt` joins it, and no earlier row had
  rendered SeaORM without auth beside the example domain — the new `authaddedmysql` row did, and
  generation refused its own output at the format check. (2) The generated `config.rs` test's
  `assert_eq!(defaults["local_domain"].as_str(), Some("<name>.test"))` crossed rustfmt's call width
  at a 14-character project name; every earlier row's name was at most 11 characters (limitation
  L-16). (3) The generated `reset()` dropped only the tables the project had selected, so an
  upgraded project booting **against the ledger its pre-auth run left** — the exact scenario the
  review's first finding described, now step 6 of both auth-added rows — met `rv_auth_*` tables
  left in the shared test database by an earlier auth row and failed its first auth migration;
  a manual reproduction against a fresh database booted and applied the eleven pending migrations
  forward, which is what isolated the cause to the shared database. The reset now drops every table
  the framework's own sets create, selected or not. Each was found by a row, fixed in the template,
  and re-proven by the row.

## 6. Testing discipline

Every batch: the failing test first (quoted in the batch evidence), the minimal change, the green
run, a positive control, then the mutations in `phase-011-mutation-ledger.md`. The four-row suites
are not process-safe, so no two four-row runs overlap; the template rows are re-run for every
guard the edit touches (§5, first bullet, is the case that wrote that rule down).

## 7. Documentation

`crates/renvor-cli/README.md` (`generate`), `crates/renvor-testkit/README.md` and crate docs
(`factory`, `app`, `client`), every generated `README.md` (what is production-capable and what is
not, the environment keys, how to run the generated test against real services), the contracts in
§1, `verification-sequence.md` 2.3.0, and `README.md` (Phase 011's state). ADRs: none accepted;
none created — the decisions this phase took are recorded in the contracts and in
`phase-011-dependency-inventory.md`, and the two API changes it declined (010/L-15's
`ConnectionString` constructor; a capability generator) are named in `phase-011-limitations.md`
as Phase 012 work behind their own records.

## 8. Limitations

`phase-011-limitations.md`: 13 retained rows of this phase; 010/L-7, 010/L-17, 009/L-15 closed with
the measurement; 010/L-14 closed only after the validation review; 010/L-1, L-5, L-6, L-15 retained
with a disposition; eleven Phase 009 rows inventoried.

## 8a. Task ledger at the checkpoint, stated as it actually is

The hey-daddy ledger for this phase (statuses set by the implementer and the validation agent;
nobody marks `complete` but the final authority): **#101** batch A (model), **#102** batch B
(templates), **#103** batch C (matrix, census, controls), **#106** batch F (generators, factories,
harness, dispositions) — `coding_done` after the corrections of 2026-09-05, awaiting the second
validation pass; **#104** batch D (the two entry findings) — `validated`; **#105** batch E (the
W-023/W-024 closure records) — written against the checkpoint head after the second pass, then
`coding_done`. Nothing in the ledger is `complete`.

## 9. What this phase did not do

- No S3 adapter, no TLS handshake in a generated project, no `renvor generate capability`, no
  upgrade command over the recorded template metadata, no two-process jobs test (010/L-6), no
  `ConnectionString` API change (010/L-15), no Windows run of a database-backed row, no work on the
  ten retained Phase 009 auth rows — each with an owner and a target in `phase-011-limitations.md`.
- No ADR accepted, no waiver created or granted, no independent human review (none is claimed),
  nothing published.

## 10. Verification and closure binding

**Sequence.** Both legs were run one after the other on this machine, `CARGO_INCREMENTAL=0`, real
services (rv-postgres 17.11, rv-mysql 8.4.11, rv-valkey 9.1.1, rv-mailpit 1.29.1), every
`RENVOR_TEST_*` variable set including both `REQUIRE` flags, `RENVOR_TEST_STARTER_ROWS` unset for
the census, a clean tracked tree (step 9 asserts it; a first attempt on `e06ae4b` reached step 9
green through 8 and failed only that step, because this record was being edited in the tree while
the leg ran — recorded, not hidden). Both legs were green on `5cb4b25` (08:45–09:28) before the Windows
corrections moved the head, and again on `f95ab6b` and `2df9f81` after each; the table records the
final pair, on the head below.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `84a8d2e7b2d6ae2d3333a3ee1c2ac90df87fec57` | `84a8d2e7b2d6ae2d3333a3ee1c2ac90df87fec57` |
| Tree | `faddc2185bc42bcc4b51957b0f4373cbd32f6cb5` | `faddc2185bc42bcc4b51957b0f4373cbd32f6cb5` |
| Steps | 9/9 ok | 9/9 ok |
| Exit | 0 | 0 |
| Tests | 2171 passed, 0 failed, 5 ignored (145 `test result` lines) | 2171 passed, 0 failed, 5 ignored (145 `test result` lines) |
| Census | 86/86 rows reported in (`renvor-cli` 9m 56s) | 86/86 rows reported in (`renvor-cli` 10m 04s) |
| Elapsed | 11:35:44–11:53:53 (step 4 general run 6m 53s) | 11:53:53–12:12:31 (step 4 general run 7m 10s) |

Logs: `gate-1.94.0.log`, `gate-stable.log` (scratch, quoted here).

**Heads, stated as they are.**

| Head | Tree | What it is |
|---|---|---|
| `4f383005851809802fb91cc4cc97972689b1c58b` | `e77cb1b6c9fc4100a502b9c48fb9a3385c3716eb` | base (origin/main) |
| `d8e3a445363da965f40470b12100082d02c68254` | `ea7e3a52d0db8198b14508041e1622a327780317` | the second validation pass's head: census 86/86, the three controls fired |
| `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0` | `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9` | **the closure head** W-023, W-024, and 010/L-14 are bound to: `d8e3a44` + the publication-order fix + the manifest-comment pin; census 86/86 re-run here |
| `84a8d2e7b2d6ae2d3333a3ee1c2ac90df87fec57` | `faddc2185bc42bcc4b51957b0f4373cbd32f6cb5` | **the gate head**: both legs green; = the closure head + the closure records, two file-set test corrections, the FP-005 allowlist entry, this record's earlier sections, and the three Windows corrections PR #62's platform legs forced — `without_verbatim_prefix` (Windows' verbatim canonical path refused by cargo in a path dependency; a pure text function tested on every platform, M-F11), its test expectation, and the platform-honest stop path of the generated tests (`67449a5`, L-10). `git diff 5eff451 84a8d2e --stat -- crates/` names two test files, `config/model.rs`, and three test templates; no generator logic other than the path text changed |
| the tip of `feat/phase-011-generators-testing-kit` | — | **the checkpoint (pull-request) head**: the gate head + this section, then the CI record — documents only; named, with the pull request's number, in the CI paragraph below once it exists |

**Pull request.** [#62](https://github.com/renvor-rs/renvor/pull/62), `feat/phase-011-generators-testing-kit` → `main`, opened 2026-09-05 from `03a3e8d`; the Windows fix and this record were pushed onto it. **Kept unmerged**; nothing tagged, released, published, or deployed.

**CI.** On `84a8d2e7b2d6ae2d3333a3ee1c2ac90df87fec57` every required check passed — 13 passed, 1 skipped by design (`attest rehearsal artifacts`, release-only): verify (1.94.0) (40m8s), verify (stable) (40m25s), platform (macos-latest, 1.94.0) (10m37s), platform (macos-latest, stable) (11m8s), platform (windows-latest, 1.94.0) (18m58s), platform (windows-latest, stable) (16m6s), docs (1m48s), security (2m38s), dependency-review (8s), Analyze (rust) (7m59s), Analyze (actions) (49s), CodeQL (3s), package and verify without publishing (2m9s). The `verify` jobs run the same nine steps against real PostgreSQL, MySQL, Valkey, and Mailpit containers with `RENVOR_TEST_STARTER_ROWS=none` for the general run and the census for the rows; the platform jobs run the `nodb` starter row. Three earlier runs on this pull request failed only their Windows platform legs, each on a defect recorded in §5 and corrected before this head (`03a3e8d`: the verbatim canonical path; `f95ab6b`: the model's own expectation of it; `2df9f81`: the clean-exit assertion after an interrupt Windows cannot send). The commit that adds this paragraph is documents-only; its own run is the pull request's final state.

**What is not claimed.** No independent human review; no merge, tag, release, publication, or
deployment; W-023 and W-024 are closed by the ledger entries that cite the closure head, not by
this paragraph; the phase is complete only when the final authority says so.

## 11. The correction round (2026-09-05) — what changed after the Codex review, and how it is bound

**What it answered.** The seventeen Native Codex findings of `phase-011-review-record.md` §3a,
each reproduced before it was fixed, each fixed at the root with a regression test that failed
first (§review §3a names every test and every red run). The Standards and Specification axes did
not reach the correcting session (§review §3b, §3c).

**What it changed in the product.** The apply engine commits a generation as one change or none
and digests the two marked files without their block; the record carries each generated resource;
`generate auth` keeps applied migrations byte-identical, adds the owner column by a forward
migration, renders every recorded resource again with its guards, verifies the merged tree in a
scratch copy, and writes the resolved lockfile; migration versions are allocated past the
directory and an import cannot collide with a version another migration holds; a name beside
`--import`, and a resource or field that is a bare SQL keyword, are refused; `renvor check`
validates the auth table's values; the record is written after verification; the wizard asks the
capabilities with a multi-select; the kernel shares its state map after Boot and the test
application attaches it, one run id, and a chosen peer to every request; the generated starter
requires the session for storage reads, refuses a mail notice from any peer but loopback, checks
the session numbers at Validate, announces the address it bound, probes the cache under a key of
its own, and resets every table the framework's sets create. Contracts: `command-surface.md`
1.3.0, `template-contract.md` 1.2.0. No dependency was added (`phase-011-dependency-inventory.md`
§4a). Three limitations were added (L-14, L-15, L-16).

**The census, extended.** `ROW_EVIDENCE` went from 86 to **87**: the auth-added proof runs on
MySQL with SeaORM as well (`the_auth_starter_added_to_a_mysql_seaorm_starter_proves_itself`),
because the forward owner migration is engine-specific SQL. Both auth-added rows now: generate a
resource before auth; run the pre-auth resource test (the ledger holds the item and resource
migrations by checksum); make `generate auth` refuse a line the user added outside the markers of
`src/routes.rs`; add auth and assert the applied migration untouched, the owner migration
planned, the resource regenerated with its guards, the lockfile an `edit`; `cargo build --locked`;
**boot the upgraded project against the ledger the pre-auth run left** and read an HTTP answer;
then run the regenerated starter and resource tests. The resource rows refuse `Order`.

**Rows re-run on this machine before the gates** (real services, `RENVOR_TEST_REQUIRE_DATABASE=1`,
`RENVOR_TEST_REQUIRE_CAPABILITIES=1`, logs `row-<name>.log`):

| Row | What it proves for the round | Result | Elapsed |
|---|---|---|---|
| `nodb` | port `0` and the announced address; `Shared` on a starter with no database | green | 15:29:38–15:30:27 |
| `cacheonly` | the per-request cache key under eight concurrent probes | green | 15:30:27–15:30:58 |
| `storageonly` | the storage routes without auth (unchanged behaviour) | green | 15:30:58–15:31:27 |
| `mailonly` | the loopback guard: the generated module's forged-peer unit test, then the notice over loopback | green | 15:31:27–15:31:57 |
| `ressqlx` | the record's `[[resource]]`; `Order` refused as a reserved identifier | green | 15:35:01–15:35:45 |
| `authadded` (PostgreSQL, SQLx) | the seven-step upgrade: resource before auth, the pre-auth ledger, the marker conflict, the untouched item migration and the owner column forward, the resource regenerated with its guards, `Cargo.lock` as an `edit`, `cargo build --locked`, boot against the existing ledger, both regenerated tests | green | 15:43:32–15:45:18 |
| `authaddedmysql` (MySQL, SeaORM) | the same seven steps on the other engine and persistence model | green | 15:45:18–15:47:14 |
| `pgsqlx` | the anonymous storage read refused with `401`; the concurrent cache probes; port `0`; every capability beside auth | green | 15:47:14–15:48:05 |

Earlier attempts of the same rows failed on the three template defects §5 records (a doubled
blank line, the SeaORM `create` signature, the name-width assertion) and on the shared-database
reset; each failure is in the logs and each fix was re-proven by the row. The rows not re-run here
(`mysqlx`, `pgsea`, `mysea`, `authonly`, `observeonly`, `ressea`, `authrefused`, the determinism
proofs, parity) are proven by the gate legs' census below.

**Mutations.** Batch G, 21 runs, 21 killed, two first forms survived and were re-targeted
(`phase-011-mutation-ledger.md` §Batch G). Totals for the phase: 45 mutations across batches A, B, D, E/F, and G, 45 killed (M-A1 and M-G10 after their tests were strengthened, M-G15 after it was re-targeted), plus the three census controls of batch C, all fired.

**Gates on the final source head.** Both legs, one after the other, on this machine,
`CARGO_INCREMENTAL=0`, real services (rv-postgres 17.11, rv-mysql 8.4.11, rv-valkey 9.1.1,
rv-mailpit 1.29.1), every `RENVOR_TEST_*` variable set including both `REQUIRE` flags,
`RENVOR_TEST_STARTER_ROWS` unset for the census, a clean tracked tree.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `f6305a7fcc6c7b7ffab3da80f6718545a3e2b04f` | `f6305a7fcc6c7b7ffab3da80f6718545a3e2b04f` |
| Tree | `b6f0cbfa9ba14768b1b2b372a9dd58f54b7b7274` | `b6f0cbfa9ba14768b1b2b372a9dd58f54b7b7274` |
| Steps | 9/9 ok | 9/9 ok |
| Exit | 0 | 0 |
| Tests | 2194 passed, 0 failed, 5 ignored (145 `test result` lines; 2171 at the checkpoint) | 2194 passed, 0 failed, 5 ignored (145 lines) |
| Census | 87/87 rows reported in (`renvor-cli` 13m 39s) | 87/87 rows reported in (`renvor-cli` 13m 26s) |
| Elapsed | 15:49:57–16:13:58 (step 4 general run 8m 11s) | 16:13:58–16:38:03 (step 4 general run 8m 34s) |

Logs: `gate-1.94.0.log`, `gate-stable.log`, `gates.log` (scratch, quoted here).

**Heads.**

| Head | Tree | What it is |
|---|---|---|
| `db952eff2ff0d1713af39897ea79fac466641d81` | `fc4fb51e…` | the checkpoint head the Codex review read (§10) |
| `f6305a7fcc6c7b7ffab3da80f6718545a3e2b04f` | `b6f0cbfa9ba14768b1b2b372a9dd58f54b7b7274` | **the final source head of the correction round**: eight signed commits `5787541` … `f6305a7` (core, testkit, the starter templates, the generators and apply engine, the manifest check and record, the wizard, the census rows, the records); both gate legs green here |
| the commit that adds this paragraph | — | documents only: the gate table above and the pull request's CI, added after the legs ran on the head they name |

**Pull request and CI.** [#62](https://github.com/renvor-rs/renvor/pull/62), pushed fast-forward from
`db952ef` to `3a62fbd` (the source head `f6305a7` plus the gate-table commit) on 2026-09-05 16:38;
the pull request's description carries the round's summary. On `3a62fbd8fcf65f303c7aae0184ac9e71b41c4ca8`
every check passed — 13 passed, 1 skipped by design (`attest rehearsal artifacts`, release-only):
verify (1.94.0) (39m11s), verify (stable) (39m19s), platform (macos-latest, 1.94.0) (8m09s),
platform (macos-latest, stable) (10m04s), platform (windows-latest, 1.94.0) (16m57s),
platform (windows-latest, stable) (20m08s), docs (1m45s), security (2m34s), dependency-review (6s),
Analyze (rust) (8m08s), Analyze (actions) (42s), CodeQL (3s), package and verify without
publishing (1m58s) — started 13:38:57 UTC, the last complete at 14:18:16 UTC. No check needed a
rerun. The commit that adds this paragraph is documents-only; its own run is the pull request's
final state. **Kept unmerged**; nothing tagged, released, published, or deployed.

**Not done, not claimed.** No independent human review; the Standards and Specification findings
were not received and are open; the merged-tree verification of `generate auth` costs a full
build of the project (measured once by hand: about 6 s on this machine with a warm shared build directory — `repro-auth.log`, `generate auth` 15:39:38 → 15:39:44; a cold build directory costs the project's full build, the same cost `renvor new` pays);
nothing merged, tagged, released, published, or deployed.
