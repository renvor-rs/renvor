# Phase 011 — Review Record

**Companion to**: [`phase-011-evidence.md`](phase-011-evidence.md)
**Phase**: 011 — Generators, the auth starter, and the testing kit

**No independent human review of this phase has occurred, and none is claimed.** Everything below
is maintainer-commissioned and **advisory**: a validation agent's review of the W-023/W-024
removal-plan controls (twice — once at an interim head, once at the checkpoint head), the
repository's own gates, and the Codex review the maintainer runs with `/codex:review` at the
checkpoint (§3, written by the maintainer's session, not this one). The phase is **not closed**;
nothing here grants a waiver, accepts a decision record, or authorises a merge.

## 1. The validation agent's first pass — interim head `9dd5bc4` (tree `d1e1350`), 2026-09-05

Commissioned to check every removal-plan control of W-023 and W-024 against an executable proof,
and to name what was prose. It exported the head read-only with `git archive` (the worktree was
mid-batch and briefly did not compile) and ran the database-free suites there; it watched the
first census run finish green (82 rows). **Verdict: 6/10, not closable yet.** Its table found
every control implemented and green, three controls **partial**, and the closure items missing:

| Finding | Disposition |
|---|---|
| No governance evidence names a head and tree; the census binaries were compiled ~80 s before the head was committed | `phase-011-evidence.md` §4 and §10 bind the census, the controls, and the gates to the checkpoint head and tree; every census cited there was run on a clean, committed tree |
| The three FR-032 negative controls (rename, delete, `cfg`-gate a row) had no evidence | run on the checkpoint head, each failing the census by name (`phase-011-evidence.md` §4); the first attempt's rename was a no-op (a `fn` that does not exist — the rows are a macro) and is recorded as such |
| L-14 and the waiver rows untouched | closed only after the second pass (§2), in `waivers.md` and `phase-010-limitations.md`, against the named head |
| **Defect 1** — `renvor.toml`'s comments named `RENVOR_AUTH__CSRF_KEY`, `RENVOR_AUTH__ABUSE_KEY`, `RENVOR_CACHE__PASSWORD` (double underscore) while the starter reads the single-underscore names | fixed in the template; the generated `.env.example` and the starter's own reader already agreed |
| **Defect 2** — the FR-015 sweep checked the bodies of a fixed reply subset for the password and the CSRF token only: no headers, no cookie value, no mailed token, no positive control | the generated test now sweeps every header value and the body of each reply (`leaks`), asserts the session cookie's value never appears in a body, asserts the mailed verification token comes back in no response, and starts with a positive control (a planted header the sweep must see) |
| **Defect 3** — the verification-mail confirmation printed `SKIPPED` and passed without `RENVOR_TEST_SMTP_API_URL`, with no requirement guard | with `RENVOR_TEST_REQUIRE_CAPABILITIES` set (the gate sets it, and the matrix forwards it) a missing sink fails the generated test; the census cannot pass on a skipped mail flow |
| **Defect 4** — the generated mail route fell back to `localhost` as the sender domain and the jobs route to a `default` queue when the section could not be read | both routes answer `503 Unavailable` instead; no invented value |
| **Defect 5** — `contracts/verification-sequence.md` said `version: "2.2.0"` under a 2.3.0 status line | `2.3.0` |
| **Defect 6** — a CI comment cited the old 45-minute bound; the job says 75 | the comment says 75 |
| **Defect 7** — batch A's mutations M-A1 … M-A8 were referenced and recorded nowhere; batch C had no mutation ledger | batch A re-run in full (ten mutations, one survivor fixed); batch C's mutations are the census controls; `phase-011-mutation-ledger.md` |
| **FR-024** — "unselected appears nowhere" was asserted by `Cargo.toml` text and file presence, not by the lock closure the requirement names | `assert_recorded` now walks the placed project's `Cargo.lock` from the manifest's runtime dependencies and asserts each `renvor-<capability>` and `renvor-auth-http` is reachable exactly when selected. The walk's first run found what text search could not: `renvor-auth` reaches every database-backed starter through the persistence adapters, chosen or not — recorded as limitation L-13, asserted as such |
| **AC-002** — the migration ledger was dropped and never read back | the generated test reads the ledger through the application's own driver and asserts one row per shipped `*.up.sql` |
| W-024 (3c) — no CI run existed for the branch | the pull request's checks are recorded in `phase-011-evidence.md` §10 |

The reviewer also confirmed, at that head: no hard-coded secret or credential in templates, fixtures,
or the two generated test files; `random_key()` draws from the standard library's entropy; no inert
choice in any template.

## 2. The validation agent's second pass — the checkpoint head

Commissioned against head `d8e3a445363da965f40470b12100082d02c68254` (tree
`ea7e3a52d0db8198b14508041e1622a327780317`) with the census, the three head-bound controls, and
the mutation logs in hand; forbidden to modify tracked files, to run the gate or the census, or to
compile the starter rows (a control script was running beside it). It ran the database-free suites
in the worktree (`renvor-cli` 288 + 5 + 4 + 1, `renvor-testkit --all-features` 22, `xtask` 36),
re-applied six mutations in a `git archive` export of the head (M-A1, M-B-01, M-D1, M-F2, M-F4,
M-F7 — every one killed by its named test, every file restored byte-identical), and swept every
template, fixture, snapshot, generated test, cited log, and record for credentials (**none**; two
known benign canaries elsewhere in the tree).

**Every removal-plan control of W-023 and W-024: satisfied**, with file:line for each — the
explicit refusals and the governed-choice pins, the pty parity, the `renvor.toml` persistence, the
wiring and the lock-closure walk, the compile-and-start verification, the four full rows with the
ledger read back, the lean rows against their real servers, the census on this head with both
`REQUIRE` flags set, the three negative controls fired on this head, no tag or release. Every
first-pass fix verified in code, not prose.

**Verdict: NOT CLOSABLE at `d8e3a44`** — for one gate regression and for the state of the records:

| Finding | Severity | Disposition |
|---|---|---|
| **D-1** `cargo test -p xtask` fails `publication_order_is_topological`: the testkit's new optional `renvor-http` edge (feature `http`) puts it before `renvor-openapi` in the release dry-run's publication order, so step 4 of `cargo xtask verify` cannot pass on this head | HIGH | `renvor-testkit` moved after `renvor-http` in `release-dry-run.yml` and `RELEASING.md`; the test passes; the closure head is the one carrying this fix, gated there (§evidence §10) |
| **D-2** closure prerequisites unwritten: five `PENDING` fields; a review-record row promising the pull request's checks while no pull request existed | HIGH (governance) | filled at the checkpoint: the gates and the pull request are in the evidence's §10 (PR #62, its checks, and the two Windows corrections they forced), the Codex review in this record's §3 |
| **D-3** the mutation ledger said the controls were re-run on the final head before those runs existed | MEDIUM | the three outcomes on `d8e3a44` are quoted; the sentence now names both heads |
| **D-4** two mutations cited without a log (M-A1's re-application, M-F2) | MEDIUM | both re-run to logs (`m-a1-rerun.log`, `m-f2.log`) and independently reproduced by this pass |
| **D-5** "fourteen starter-matrix rows" — `ROW_EVIDENCE` holds eighteen plus parity | LOW | corrected |
| **D-6** "twelve `auth_repositories` rows" — eight | LOW | corrected |
| **D-7** the `renvor.toml` comment's variable names are fixed but nothing pins them | LOW (gap, not a false claim) | a manifest-render test now asserts the single-underscore names and refuses a double underscore (mutation M-F10) |
| **D-8** `cargo doc --all-features` "green by hand" cited no run | LOW | cited, and re-run by this pass |

On 010/L-14 the reviewer's reading: the recorded measurement is sufficient in substance to close
it, and it must be closed against the head that carries D-1's fix with the gate green there, not
against `d8e3a44`. That is what the closure records do (§evidence §10). Task statuses set by the
reviewer: #101, #102, #103 `validated`; #105, #106 `needs_fixes` (D-1 and the records).

## 3. Codex review at the checkpoint, and the one bounded correction round

The maintainer ran `/codex:review` against the pull request head `db952ef` (base `origin/main`
`4f38300`) on 2026-09-05 at 13:26–13:47 (Codex session `01a0711a-fccd-71e1-a3cc-55460f8e8c01`;
the reviewer's own verdict: "patch is incorrect", confidence 0.99). The maintainer then directed
one bounded correction round covering three review axes. **Each axis is recorded on its own
below, and only what reached the correcting session is recorded.**

### 3a. The Native Codex axis — seventeen findings, six P1 and eleven P2

Every finding was reproduced independently before it was fixed — by reading the cited site and
by a test that failed on the unfixed tree (the red run is in the scratch logs named). Every fix
is a root-cause change with the regression test that failed first; no test, gate, contract, or
assertion was weakened.

| # | Finding (Codex's title) | Sev. | Reproduced by | Fix | Regression test (red on `db952ef`, green after) |
|---|---|---|---|---|---|
| 1 | Preserve migration history when adding auth | P1 | reading `plan_auth`: every rendered file was planned, `migrations/0001_create_item.up.sql` included, and the untouched copy classified `regenerate`; unit test `adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward` failed with "an applied migration was planned again" | an existing `migrations/0001_*` pair is never planned; the owner column arrives by a forward pair `<version>_add_item_owner.{up,down}.sql` (two statements on PostgreSQL, one on MySQL — both probed against the real engines first; rows that existed belong to the all-zero owner, as the seeds mark theirs) | `commands::generate::auth_tests::adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward`; live: `authadded` and the new `authaddedmysql` rows boot the upgraded project **against the ledger the pre-auth run left** — live: both auth-added rows booted the upgraded project against the ledger the pre-auth run left — `authadded` (PostgreSQL/SQLx) and `authaddedmysql` (MySQL/SeaORM) green on 2026-09-05 15:45 and 15:47, no step skipped |
| 2 | Secure existing generated resources when adding auth | P1 | reading: the resource module is rendered with `auth_session = false` and `plan_auth` re-rendered starter files only; unit test `adding_auth_renders_every_recorded_resource_again_with_its_guards` failed ("the recorded resource is rendered again") | the record carries one `[[resource]]` (name, fields) per generated resource; `plan_auth` renders every recorded module and test again with the guards; an edited module is a conflict, so auth is refused rather than added beside a public write | `…auth_tests::adding_auth_renders_every_recorded_resource_again_with_its_guards`; `record::tests::the_record_carries_the_resources_a_generator_defined`; live: the `Post` resource generated before auth in both auth-added rows refuses a write without a session afterwards — live: the `Post` resource generated before auth refuses a write without a session afterwards in both auth-added rows (green 15:45, 15:47) |
| 3 | Keep marker edits from claiming the whole file | P1 | reading `apply::commit`: an `Edit` recorded the digest of the merged file; unit test `a_marker_edit_never_claims_the_rest_of_the_file` failed (the re-render classified `Regenerate` instead of a conflict) | for the two marked files the provenance digest is taken over the file with its block emptied, and an edit of a marked file leaves the record entry as it was; `renvor new`'s record needs no special case because an empty block digests to the plain file | `generate::apply::tests::a_marker_edit_never_claims_the_rest_of_the_file` (with a positive control), `…the_recorded_digest_of_a_marked_file_ignores_its_block`; live: a line added outside the markers of `src/routes.rs` makes `generate auth` a conflict in both auth-added rows — live: a line added outside the markers of `src/routes.rs` made `generate auth` a conflict in both auth-added rows (green 15:45, 15:47) |
| 4 | Require a session before reading stored objects | P1 | reading the storage template: `get` had no session check; render test `the_storage_read_route_requires_a_session_when_auth_is_on` failed | `GET /files/{name}` requires the session when auth is on, and both declarations list `authentication_required` | `commands::new::tests::the_storage_read_route_requires_a_session_when_auth_is_on`; live: the generated `tests/starter.rs` now asserts `401` for an anonymous read on every auth+storage row — live: the generated `tests/starter.rs` asserts `401` for an anonymous read on the `pgsqlx` row (green 15:48) |
| 5 | Enforce loopback-only access to mail notices | P1 | reading the mail template: without auth, `notice` read the body with no caller check; render test `the_session_free_mail_notice_answers_the_loopback_peer_only` failed | the peer identity is checked **before** anything else is read: only `ClientIdentity::DirectPeer` with a loopback address passes; anything else is `403 not_permitted`; the declaration lists it | the render test above (strengthened after mutation M-G10 survived its first form); the generated mail module carries a unit test that forges a `203.0.113.9` peer through `renvor_testkit::app::TestApplication::request_from` and asserts `403`, with the loopback peer as its positive control — run by `cargo test` of every mail-without-auth row — live: the forged-peer unit test ran in the `mailonly` row's `cargo test` (green 15:31) |
| 6 | Enforce unique versions across the migration directory | P1 | reading `existing_version` (by name only); integration test `two_names_generated_within_a_second_get_distinct_versions` failed with two equal versions (`20260905115550`), and `an_import_refuses_a_version_another_migration_already_holds` failed (the set was written beside the user's file) | the version is allocated past every version the directory holds (`allocate_version`, `existing_versions`, `version_of`); an import whose version another migration holds is `generation_conflict` with `reason = version_present`, nothing written | `commands::generate::tests::a_version_is_allocated_past_every_version_the_directory_holds`; `tests/generate.rs::two_names_generated_within_a_second_get_distinct_versions`, `…an_import_refuses_a_version_another_migration_already_holds` |
| 7 | Attach booted provider state to test requests | P2 | testkit test `a_request_carries_the_state_the_booted_providers_registered` failed with `500` (`StateMissing`) | the kernel freezes the state map into an `Arc` at the end of Boot (`Application::shared_state`, additive) and the test application attaches it to every request; `with_state` replaces it for a map the test built | `renvor_testkit::app::tests::a_request_carries_the_state_the_booted_providers_registered` |
| 8 | Update `Cargo.lock` when adding auth | P2 | reading `plan_auth`: no lockfile in the plan, no resolution; the `authadded` row's `cargo build --locked` assertion — live: both auth-added rows assert `Cargo.lock` is an `edit` and pass `cargo build --locked` (green 15:45, 15:47) | after the conflict check and before the commit (dry run included) the merged tree is copied to scratch — without `target/`, `.git/`, symlinks, within a bound — and verified with the five checks `renvor new` runs; the resolved `Cargo.lock` joins the plan as an `edit` | live: `authadded`/`authaddedmysql` assert `Cargo.lock` is an `edit` and run `cargo build --locked` — live: both auth-added rows assert `Cargo.lock` is an `edit` and pass `cargo build --locked` (green 15:45, 15:47) |
| 9 | Reject or quote SQL-reserved resource identifiers | P2 | unit test `a_reserved_sql_word_is_refused_as_a_table_or_a_column` failed (`Order` accepted) | a curated union of PostgreSQL's and MySQL 8.4's reserved words (sorted, binary-searched) refuses a type name whose table, or a field, would be a bare keyword: `unsupported_value`, `reason = reserved_identifier`; quoting was rejected because it would make every generated statement engine-specific | `commands::generate::resource_tests::a_reserved_sql_word_is_refused_as_a_table_or_a_column`; live: the resource rows refuse `Order` — live: the `ressqlx` row refuses `Order` with `reason = reserved_identifier` (green 15:35) |
| 10 | Reject a migration name combined with `--import` | P2 | integration test `a_name_beside_an_import_is_refused_rather_than_ignored` failed (exit 0, the set imported) | `conflicts_with = "import"` on the positional name: a parser refusal, exit `2`, naming the flag | `tests/generate.rs::a_name_beside_an_import_is_refused_rather_than_ignored` |
| 11 | Validate session limits during Validate | P2 | reading the generated `auth_source` validator (keys only); render test `the_auth_section_validates_its_numbers_at_validate` failed | `check_auth_section` in the generated `config.rs` refuses `sessions_per_user = 0`, a zero or over-ceiling idle or absolute bound (the ceilings are `SessionPolicy`'s constants), and idle above absolute — naming the key — and the validator calls it | the render test; the generated config module's own unit test `the_session_numbers_are_checked_at_validate_naming_the_key`, run by every auth row's `cargo test` — live: the generated config test ran in every auth row's `cargo test` (`pgsqlx`, both auth-added rows) |
| 12 | Validate exact supported auth manifest values | P2 | unit test `the_auth_table_holds_the_values_the_generator_writes_and_nothing_else` failed (`garbage` accepted) | `auth.migrations` must be `renvor-auth/<the recorded engine>`, `auth.session_cookie` must be `__Host-rv_session`, `auth.mail` must be `smtp`, each refused by field | `commands::check::tests::the_auth_table_holds_the_values_the_generator_writes_and_nothing_else` |
| 13 | Use a multi-select for capability choices | P2 | source scan `the_capabilities_question_is_a_multi_select_of_exactly_the_five` failed (a text question) | `prompt::multi_select` over `cliclack::MultiSelect` (no new dependency); the wizard offers exactly `Capability::ALL` with a hint each, `mail` pre-selected under `session`, an empty selection serialised to `none`; the question count stays eighteen | the scan test; `every_capability_is_offered_with_its_own_hint`; the pty parity and terminal suites still drive the question with Enter — proven by the gate legs' pty suites (§evidence §11) |
| 14 | Report the actual bound HTTP address | P2 | render test `the_starter_reports_the_address_it_actually_bound` failed (`main` printed the configured string) | `Services::bound_address` asks the HTTP provider (kept in a `OnceLock` and handed to the kernel through `Shared`); `main` prints it or fails; the generated tests bind port `0` and read the announced address | the render test; live: every row's generated test now starts on port `0` and refuses an announcement ending in `:0` — live: every re-run row's generated test started on port `0` and read the announced address (`nodb`, `cacheonly`, `storageonly`, `mailonly`, `ressqlx`, `authadded`, `authaddedmysql`, `pgsqlx`) |
| 15 | Write provenance after Cargo resolves the lockfile | P2 | integration test `the_record_digests_are_the_placed_files_including_the_resolved_lockfile` failed (no `Cargo.lock` entry) | the record is written after verification, before the manifest; the snapshot policy pins the paths of `Cargo.lock` and the record and not their digests (`template-contract.md` 1.2.0 — the contract already said the lockfile was not pinned, and the code now does what it says) | `tests/generate.rs::the_record_digests_are_the_placed_files_including_the_resolved_lockfile`; `tests/snapshots.rs` |
| 16 | Reuse the application's run identifier in test requests | P2 | testkit test `every_request_carries_the_applications_one_run_identifier` failed (two ids) | one `RunIdentifier` per `TestApplication`: the booted application's, or one generated when a registry alone is wrapped; `run_id()` exposes it | `renvor_testkit::app::tests::every_request_carries_the_applications_one_run_identifier` |
| 17 | Isolate cache ping keys per request | P2 | render test `the_cache_ping_uses_a_key_of_its_own_per_request` failed (`CacheKey::new("ping")`) | the key is `ping:<request id>`; the generated test fires eight concurrent probes | the render test; live: the concurrent probes on every cache row — live: eight concurrent probes on the `cacheonly` and `pgsqlx` rows (green 15:30, 15:48) |

Also made explicit at the maintainer's direction: **generation into an existing project is
transactional** (`apply::commit` stages every sibling, then places every one, and rolls back what
it placed on any failure; `tests/generate.rs::a_failure_during_commit_leaves_the_project_unchanged`
injects a failure at both points with `RENVOR_FAIL_AT`, and
`a_rollback_restores_what_was_placed_newest_first_and_names_what_it_could_not` tests the rollback
itself).

### 3b. The Standards axis

**Not received.** The maintainer's instruction named a Standards axis and a "P3 Windows
documentation mismatch"; the text of those findings was composed in the maintainer's Codex desktop
thread, which is not persisted on this machine (the two on-disk rollouts of the session hold only
the Native Codex output above; Codex's prompt history records the maintainer's turns at 14:18 and
14:28 but not the assistant's). The correcting session therefore could not reproduce any Standards
finding as stated. What it did instead, and records as its own reading rather than as Codex's: the
one Windows documentation mismatch it could identify — `SUPPORT.md` listed **two** `#[cfg(unix)]`-
gated behaviours while this pull request added a third (the generated starter test's clean-exit
assertion after an interrupt, skipped on Windows, L-10), and said nothing about the `nodb`-only
starter row of the platform job — is corrected in `SUPPORT.md`. If the Standards findings say
something else, they are open.

### 3c. The Specification axis

**Not received**, for the same reason as §3b. The Native Codex findings that cite the
specification (FR-021, FR-027, FR-028, FR-034, FR-046, FR-050, and the provenance rule of §4.4)
are dispositioned in §3a against the specification's text, which was treated as authoritative
throughout; no finding required a change of scope, a reading of "ships", or a waiver, so nothing
was escalated.

### 3d. Mutation controls of the round (batch G)

Twenty-one mutations, one per fix and two for the transactional commit, run by `mutations-g.py`
in an isolated copy of the working tree with its own build directory (so the census rows compiling
beside it could not pick up a mutation): **21 killed**. Two first forms survived — M-G10 (the
mail guard's refusal removed) because the render assertion asked only for `is_loopback()`, and
M-G15 because it mutated the rename-failure branch no test reaches — and each was answered by
strengthening or re-targeting the control, not by dropping the mutation (`phase-011-mutation-ledger.md`
§Batch G, `mutations-g.log`).

### 3e. What the round did not do

- It did not run the Standards or Specification findings, because it did not have them (§3b, §3c).
- It did not make the rename-failure branch of `apply::commit` reachable by a test: a rename after
  a successful stage in the same directory fails only on a cross-device move. The rollback
  function that branch calls is tested directly, and the injected-failure branch beside it is
  tested end to end (L-14).
- It did not quote table and column identifiers; it refuses reserved words instead, from a
  curated list that a future engine keyword could fall outside of (L-15).

## 4. What this record does not claim

- That any review above was independent. The reviewer is an agent commissioned by the same
  session that wrote the code; the maintainer is the sole human.
- That W-023 or W-024 is closed by anything in this file. Closure is a ledger entry in
  `waivers.md`, bound to a head and tree, written only after §2 reports every control satisfied.
- That the phase is complete. The merge-authority checkpoint is the maintainer's decision.
