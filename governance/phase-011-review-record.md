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
assertion was weakened by the Native fixes. The continuation loosened one assertion of the
census's own, deliberately and on record — the transcript rule for a placed project's test —
stated at the end of this section.

| # | Finding (Codex's title) | Sev. | Reproduced by | Fix | Regression test (red on `db952ef`, green after) |
|---|---|---|---|---|---|
| 1 | Preserve migration history when adding auth | P1 | reading `plan_auth`: every rendered file was planned, `migrations/0001_create_item.up.sql` included, and the untouched copy classified `regenerate`; unit test `adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward` failed with "an applied migration was planned again" | an existing `migrations/0001_*` pair is never planned; the owner column arrives by a forward pair `<version>_add_item_owner.{up,down}.sql` (two statements on PostgreSQL, one on MySQL — both probed against the real engines first; rows that existed belong to the all-zero owner, as the seeds mark theirs) | `commands::generate::auth_tests::adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward`; live: both auth-added rows booted the upgraded project against the ledger the pre-auth run left — `authadded` (PostgreSQL/SQLx) and `authaddedmysql` (MySQL/SeaORM) green on 2026-09-05 15:45 and 15:47, no step skipped |
| 2 | Secure existing generated resources when adding auth | P1 | reading: the resource module is rendered with `auth_session = false` and `plan_auth` re-rendered starter files only; unit test `adding_auth_renders_every_recorded_resource_again_with_its_guards` failed ("the recorded resource is rendered again") | the record carries one `[[resource]]` (name, fields) per generated resource; `plan_auth` renders every recorded module and test again with the guards; an edited module is a conflict, so auth is refused rather than added beside a public write | `…auth_tests::adding_auth_renders_every_recorded_resource_again_with_its_guards`; `record::tests::the_record_carries_the_resources_a_generator_defined`; live: the `Post` resource generated before auth refuses a write without a session afterwards in both auth-added rows (green 15:45, 15:47) |
| 3 | Keep marker edits from claiming the whole file | P1 | reading `apply::commit`: an `Edit` recorded the digest of the merged file; unit test `a_marker_edit_never_claims_the_rest_of_the_file` failed (the re-render classified `Regenerate` instead of a conflict) | for the two marked files the provenance digest is taken over the file with its block emptied, and an edit of a marked file leaves the record entry as it was; `renvor new`'s record needs no special case because an empty block digests to the plain file | `generate::apply::tests::a_marker_edit_never_claims_the_rest_of_the_file` (with a positive control), `…the_recorded_digest_of_a_marked_file_ignores_its_block`; live: a line added outside the markers of `src/routes.rs` made `generate auth` a conflict in both auth-added rows (green 15:45, 15:47) |
| 4 | Require a session before reading stored objects | P1 | reading the storage template: `get` had no session check; render test `the_storage_read_route_requires_a_session_when_auth_is_on` failed | `GET /files/{name}` requires the session when auth is on, and both declarations list `authentication_required` | `commands::new::tests::the_storage_read_route_requires_a_session_when_auth_is_on`; live: the generated `tests/starter.rs` asserts `401` for an anonymous read on the `pgsqlx` row (green 15:48) |
| 5 | Enforce loopback-only access to mail notices | P1 | reading the mail template: without auth, `notice` read the body with no caller check; render test `the_session_free_mail_notice_answers_the_loopback_peer_only` failed | the peer identity is checked **before** anything else is read: only `ClientIdentity::DirectPeer` with a loopback address passes; anything else is `403 not_permitted`; the declaration lists it | the render test above (strengthened after mutation M-G10 survived its first form); the generated mail module carries a unit test that forges a `203.0.113.9` peer through `renvor_testkit::app::TestApplication::request_from` and asserts `403`, with the loopback peer as its positive control — run by `cargo test` of every mail-without-auth row — live: the forged-peer unit test ran in the `mailonly` row's `cargo test` (green 15:31) |
| 6 | Enforce unique versions across the migration directory | P1 | reading `existing_version` (by name only); integration test `two_names_generated_within_a_second_get_distinct_versions` failed with two equal versions (`20260905115550`), and `an_import_refuses_a_version_another_migration_already_holds` failed (the set was written beside the user's file) | the version is allocated past every version the directory holds (`allocate_version`, `existing_versions`, `version_of`); an import whose version another migration holds is `generation_conflict` with `reason = version_present`, nothing written | `commands::generate::tests::a_version_is_allocated_past_every_version_the_directory_holds`; `tests/generate.rs::two_names_generated_within_a_second_get_distinct_versions`, `…an_import_refuses_a_version_another_migration_already_holds` |
| 7 | Attach booted provider state to test requests | P2 | testkit test `a_request_carries_the_state_the_booted_providers_registered` failed with `500` (`StateMissing`) | the kernel freezes the state map into an `Arc` at the end of Boot (`Application::shared_state`, additive) and the test application attaches it to every request; `with_state` replaces it for a map the test built | `renvor_testkit::app::tests::a_request_carries_the_state_the_booted_providers_registered` |
| 8 | Update `Cargo.lock` when adding auth | P2 | reading `plan_auth`: no lockfile in the plan, no resolution; the `authadded` row's `cargo build --locked` assertion — live: both auth-added rows assert `Cargo.lock` is an `edit` and pass `cargo build --locked` (green 15:45, 15:47) | after the conflict check and before the commit (dry run included) the merged tree is copied to scratch — without `target/`, `.git/`, symlinks, within a bound — and verified with the five checks `renvor new` runs; the resolved `Cargo.lock` joins the plan as an `edit` | live: both auth-added rows assert `Cargo.lock` is an `edit` and pass `cargo build --locked` (green 15:45, 15:47) |
| 9 | Reject or quote SQL-reserved resource identifiers | P2 | unit test `a_reserved_sql_word_is_refused_as_a_table_or_a_column` failed (`Order` accepted) | a curated union of PostgreSQL's and MySQL 8.4's reserved words (sorted, binary-searched) refuses a type name whose table, or a field, would be a bare keyword: `unsupported_value`, `reason = reserved_identifier`; quoting was rejected because it would make every generated statement engine-specific | `commands::generate::resource_tests::a_reserved_sql_word_is_refused_as_a_table_or_a_column`; live: the `ressqlx` row refuses `Order` with `reason = reserved_identifier` (green 15:35) |
| 10 | Reject a migration name combined with `--import` | P2 | integration test `a_name_beside_an_import_is_refused_rather_than_ignored` failed (exit 0, the set imported) | `conflicts_with = "import"` on the positional name: a parser refusal, exit `2`, naming the flag | `tests/generate.rs::a_name_beside_an_import_is_refused_rather_than_ignored` |
| 11 | Validate session limits during Validate | P2 | reading the generated `auth_source` validator (keys only); render test `the_auth_section_validates_its_numbers_at_validate` failed | `check_auth_section` in the generated `config.rs` refuses `sessions_per_user = 0`, a zero or over-ceiling idle or absolute bound (the ceilings are `SessionPolicy`'s constants), and idle above absolute — naming the key — and the validator calls it | the render test; the generated config module's own unit test `the_session_numbers_are_checked_at_validate_naming_the_key`, run by every auth row's `cargo test` — live: the generated config test ran in every auth row's `cargo test` (`pgsqlx`, both auth-added rows) |
| 12 | Validate exact supported auth manifest values | P2 | unit test `the_auth_table_holds_the_values_the_generator_writes_and_nothing_else` failed (`garbage` accepted) | `auth.migrations` must be `renvor-auth/<the recorded engine>`, `auth.session_cookie` must be `__Host-rv_session`, `auth.mail` must be `smtp`, each refused by field | `commands::check::tests::the_auth_table_holds_the_values_the_generator_writes_and_nothing_else` |
| 13 | Use a multi-select for capability choices | P2 | source scan `the_capabilities_question_is_a_multi_select_of_exactly_the_five` failed (a text question) | `prompt::multi_select` over `cliclack::MultiSelect` (no new dependency); the wizard offers exactly `Capability::ALL` with a hint each, `mail` pre-selected under `session`, an empty selection serialised to `none`; the question count stays eighteen | the scan test; `every_capability_is_offered_with_its_own_hint`; the pty parity and terminal suites still drive the question with Enter — proven by the gate legs' pty suites (§evidence §11) |
| 14 | Report the actual bound HTTP address | P2 | render test `the_starter_reports_the_address_it_actually_bound` failed (`main` printed the configured string) | `Services::bound_address` asks the HTTP provider (kept in a `OnceLock` and handed to the kernel through `Shared`); `main` prints it or fails; the generated tests bind port `0` and read the announced address | the render test; live: every re-run row's generated test started on port `0` and read the announced address (`nodb`, `cacheonly`, `storageonly`, `mailonly`, `ressqlx`, `authadded`, `authaddedmysql`, `pgsqlx`) |
| 15 | Write provenance after Cargo resolves the lockfile | P2 | integration test `the_record_digests_are_the_placed_files_including_the_resolved_lockfile` failed (no `Cargo.lock` entry) | the record is written after verification, before the manifest; the snapshot policy pins the paths of `Cargo.lock` and the record and not their digests (`template-contract.md` 1.2.0 — the contract already said the lockfile was not pinned, and the code now does what it says) | `tests/generate.rs::the_record_digests_are_the_placed_files_including_the_resolved_lockfile`; `tests/snapshots.rs` |
| 16 | Reuse the application's run identifier in test requests | P2 | testkit test `every_request_carries_the_applications_one_run_identifier` failed (two ids) | one `RunIdentifier` per `TestApplication`: the booted application's, or one generated when a registry alone is wrapped; `run_id()` exposes it | `renvor_testkit::app::tests::every_request_carries_the_applications_one_run_identifier` |
| 17 | Isolate cache ping keys per request | P2 | render test `the_cache_ping_uses_a_key_of_its_own_per_request` failed (`CacheKey::new("ping")`) | the key is `ping:<request id>`; the generated test fires eight concurrent probes | the render test; live: eight concurrent probes on the `cacheonly` and `pgsqlx` rows (green 15:30, 15:48) |

Also made explicit at the maintainer's direction: **generation into an existing project is
transactional** (`apply::commit` stages every sibling, then places every one, and rolls back what
it placed on any failure). The Native round injected a failure at two points; the Standards
continuation replaced that with a boundary after every staged file, after every placed file, and
before the record (`RENVOR_FAIL_AT=generate-stage-<n>`, `generate-place-<n>`, `generate-record`),
swept by `tests/generate.rs::a_failure_at_every_placement_boundary_leaves_the_project_byte_identical`;
`a_rollback_restores_what_was_placed_newest_first_and_names_what_it_could_not` tests the rollback
itself. **One assertion of this repository's own was loosened, deliberately and on record:** the
census's transcript rule for a placed project's test went from "exactly one test passed" to "at
least one passed, none failed", because the generated test binary now carries the negative
control of §3b S1 beside the starter test (`starter_matrix.rs::ran_to_a_pass`). No product test,
gate, contract, or security control was weakened.

### 3b. The Standards axis — five findings, received in the round's continuation

The maintainer supplied the Standards findings after the Native axis had been answered (the
text had been composed in the Codex desktop thread and was not on disk). Each was reproduced on
its own against the corrected head `f6305a7` — never taken as resolved because it overlapped a
Native finding — and dispositioned here independently.

| # | Finding | Sev. | Reproduced by | Disposition |
|---|---|---|---|---|
| S1 | Secret-check failures disclose the credentials they detect (`renvor-testkit` sweep; the generated starter test's cookie and mailed-token diagnostics) | P1 | testkit test `a_failed_sweep_names_the_canary_by_index_and_never_by_value` failed on `f6305a7`: the panic carried the canary and the whole swept line; render test `the_generated_tests_fail_without_printing_a_credential` failed: `tests/starter.rs` printed `{cookie}`, `login.body` (the CSRF token), and the mail's text | **fixed.** The sweep's failure names the canary and the entry **by position** and their lengths; every credential extraction in the generated tests goes through a support helper with a static label (`session_cookie_of`, `csrf_token_of`, `token_in`), the sink listing's failure prints a count, and the login answer's failure prints its status. Negative controls: the testkit test (raw, `Debug`, hex, decimal renderings via `renvor_testkit::every_rendering_of`), and the generated support module's own `a_failed_secret_extraction_names_nothing_secret`, which fails each helper on a canary-bearing input and checks the panic text, with a positive control — run by `cargo test` inside every auth starter's verification and rows. Live: the rows pass with the control compiled into their test binaries; a green row prints no inner test name, and the one transcript on disk that shows the generated control running inside a starter's test binary is `mutations-h-M-H8.out` — where it is `FAILED` beside `the_starter_starts_answers_and_stops_cleanly ... ok`, because that run is the mutation: a helper made to print the cookie, and the control refusing the starter for it. The rows re-run on the continuation's templates are in §evidence §12 |
| S2 | The sealed verification environment forwards credential-bearing proxy URLs and returns raw child output | P1 | unit test `a_proxy_credential_never_reaches_the_sealed_environment` failed on `f6305a7` (`http://alice:s3cr3t…@` passed through verbatim); the end-to-end control `a_build_script_cannot_observe_or_print_a_proxy_credential` failed: a staged project's build script printed the proxy credential and the verification error carried it | **fixed.** `seal` keeps the proxy variables with their `user:password@` removed (the host, scheme, port, and path survive; a non-text value is dropped); `in_staging` reports a child's output only after `redacted_output` — every URL credential replaced, every credential the seal removed replaced, every control character escaped. The build-script control now proves the script sees `http://127.0.0.1:1` and the error names it without the credential in any rendering; `a_childs_output_is_reported_without_url_credentials_or_control_characters` tests the redaction alone. `generation-transaction.md` 1.1.0 states the rule. **One shape the validation agent found passes verbatim:** a value with an unencoded `/` inside its userinfo (`http://user:p/w@host`) is not a URL, the authority is cut at that `/`, no `@` is found, and the value is passed as it was; recorded rather than handled |
| S3 | Existing-project generation can leave partial updates after a write failure | P2 | on `f6305a7` the commit was already staged-then-placed with rollback, but only two boundaries were injected. `a_failure_at_every_placement_boundary_leaves_the_project_byte_identical` failed on `f6305a7` because the per-file boundaries did not exist (an unknown injection point is not a failure) | **fixed and proven at every boundary.** `RENVOR_FAIL_AT=generate-stage-<n>` after each staged file, `generate-place-<n>` after each placed file, `generate-record` before the record; the test sweeps all of them for a two-file plan (5) and a sixteen-file plan (33) — **38 boundaries** — and compares the whole project byte for byte each time, the record and the migration directory included. **Stated gap:** both plans are all `write` actions, so the rollback branch that restores a file's previous bytes (an `edit` or a `regenerate`, which is exactly what the starter-only files — the marked files, `Cargo.toml`, `Cargo.lock` — undergo) is reached end to end by no injected failure; it is covered by the direct unit test of `roll_back` and by the same commit engine (L-14). The auth-added rows exercise those files live: step 3 asserts the refusal code, the conflicting path, and that `src/auth.rs` was not written; step 4 asserts what was rewritten |
| S4 | Migration versions were not globally classified | P2 | the Native reproduction (two equal versions in one second; an imported version written beside a user's file) re-run on `f6305a7`: green after the Native fix | **independently dispositioned as fixed.** The Standards cases: two names in one second (`two_names_generated_within_a_second_get_distinct_versions`, now also asserting each pair's up and down share one version), an imported version held under another name (`an_import_refuses_a_version_another_migration_already_holds`, now also asserting the provenance record is unchanged after the refusal), the whole tree unchanged after the refusal |
| S5 | Generated documentation overstates Windows shutdown coverage | P3 | render test `the_generated_readme_states_the_windows_shutdown_limitation` failed on `f6305a7`: the generated README said the test "sends the interrupt a terminal would and requires a clean exit" | **fixed.** The generated README states the unix behaviour (`SIGINT`, exit 0 within the drain bound) and the Windows limitation (the process is ended, `SKIPPED:` printed for that one assertion, L-10) precisely; L-10 stays retained; `SUPPORT.md` was corrected earlier in the round |

### 3c. The Specification axis — five findings, received with the Standards axis

| # | Finding | Sev. | Reproduced by | Disposition |
|---|---|---|---|---|
| P1 | Keep caught secrets out of test diagnostics (SR-001) | P1 | the same two failing tests as S1, read against SR-001's list — "a log line … a fixture" — the test output is a log the sweep's own failure wrote a credential into | **fixed with S1**, dispositioned separately: SR-001 is satisfied by construction for the three credentials the generated test handles, and the negative control is in the generated project, so every consumer's copy of the test carries its own proof |
| P2 | Do not regenerate a differing file automatically (FR-048, SR-009, `data-model.md`) | P2 | **reproduced, not fixed — a product decision.** On `f6305a7` a file that differs from the render but matches its recorded digest is classified `Regenerate` and overwritten (`apply::tests::absent_identical_untouched_and_modified_are_the_four_cases`, `untouched.txt`; the `authadded` row's `src/main.rs` is `regenerate`). The six scenarios asked for: (a) differs-from-render, digest matches → **overwritten** (the disputed case); (b) a user edit outside a marker → conflict, nothing written (`a_marker_edit_never_claims_the_rest_of_the_file`, the rows' step 3); (c) a user edit inside a marker → carried verbatim, never overwritten (`carry_marked_block`, `insert_before_marker`'s idempotence); (d) a generator-owned marker update → an `edit`, never a conflict; (e) repeated generation → `unchanged`; (f) a failure → the whole project unchanged (S3). | **decided by the maintainer on 2026-09-05 (option 3) and fixed in `2b3e4a8`**: regeneration is gated behind the explicit `--overwrite-unchanged` flag the data model reserved — a digest-matching differing file is *regenerable*, reported, and replaced only under the flag; without it the run is refused naming the flag; a changed file is refused with or without it; a dry run classifies identically. §3g records the round. The specification was edited to state the flag (FR-048, SR-009, `data-model.md` §4) and **not** to permit an implicit overwrite; `command-surface.md` 1.4.0 and `json-output.md`'s `generation_conflict` row say the same. `generate auth` needs the flag on every placed starter, by design |
| P3 | The offline starter guarantee was not satisfied (FR-006) | P2 | `tests/offline.rs` had **no** starter case at all, so FR-006 had never been measured; the new case `a_starter_is_generated_with_networking_unavailable_from_the_cache_a_framework_build_leaves` — an **empty** `CARGO_HOME`, the framework's lockfile closure fetched into it (`cargo fetch --locked`), then `CARGO_NET_OFFLINE=true` generation of a starter — failed on `f6305a7`: `no matching package named signal-hook-registry found … offline mode` (`red-offline.log`) | **fixed: the guarantee is made true.** The facade offers `renvor::shutdown_signal()` (Tokio's `signal`, optional, under `transport-rest`, which every starter enables), the starter waits on it instead of enabling `signal` itself, and `signal-hook-registry` 1.4.8 enters the framework's `Cargo.lock` — the one package the seeded lock was short of (L-3, closed). The case is green after the fix: 41 s, `green-offline.log`. **Stated, not assumed:** the precondition the test realises is "the framework's lockfile closure is in the registry cache", which a `cargo fetch` or the verification gate's all-feature build leaves; a build of a feature subset (`cargo build -p renvor-cli`) leaves less, and FR-006's "has been built" does not say which. That reading is reported for the maintainer, not decided here |
| P4 | Reject migration-version collisions (FR-046) | P2 | as S4, read against FR-046's "refusing a version already present" | **fixed with Native 6 and S4**, dispositioned separately: FR-046's refusal is `generation_conflict` with `reason = version_present` naming the versions, nothing written, the record unchanged |
| P5 | Implement the specified capabilities multi-select (FR-021), verified through the real pty | P2 | the Native fix was a source scan plus the existing pty suites pressing Enter. New pty tests on `f6305a7`: `the_capabilities_question_offers_exactly_the_five`, `selecting_no_capability_is_the_explicit_none`, `cancelling_at_the_capabilities_question_writes_nothing` passed; the multi-selection case needs a framework whose verification can build, so it lives in `tests/parity.rs` | **verified.** Through the real terminal: exactly five choices and no sixth (`none` is what an empty selection means); an empty selection serialises to `none`; Escape at the question exits 4 with nothing written and no staging left; the parity starter case toggles `storage` then `cache` — the reverse of canonical order — beside the pre-selected `mail`, the review's equivalent command reads `--capabilities cache,mail,storage`, and the wizard's tree equals the flags' tree byte for byte with all three recorded. Parity green after the fixes: `green-parity.log`, 30 s (the flag run also exercised the generated negative control during its own verification). |

### 3d. Mutation controls of the round (batch G)

Twenty distinct mutations, one per fix and two for the transactional commit, run by
`mutations-g.py` in an isolated copy of the working tree with its own build directory (so the
census rows compiling beside it could not pick up a mutation): **22 runs, 20 killed, 2 first-form
survivals** (`mutations-g.log`). M-G10 (the mail guard's refusal removed) survived because the
render assertion asked only for `is_loopback()`, and was killed once the assertion asked for the
refusal. M-G15's first form mutated the rename-failure branch no test reaches; that edit was
**abandoned as unreachable, not killed**, and the mutation was re-targeted (M-G15 at the rollback
function, M-G15b at a placement boundary), both killed. The validation agent's third pass added
two mutations of its own for findings 11 and 13, whose first controls were a render scan and a
compile error: V-11 (the validator's call to `check_auth_section` removed) and V-13 (the wizard
skipping the question and taking the initial values) — both killed, cited in its report
(`validation-3-report.md`, scratch). Finding 8 has no unit test and no mutation: its proof is live
only (the auth-added rows assert `Cargo.lock` is an `edit` and run `cargo build --locked`).

### 3e. The validation agent's third pass — verdict NEEDS_FIXES, and what was done with it

The validation agent reviewed the continuation on `cdf5e50` plus the working tree at 18:27–18:52
(`validation-3-report.md`, scratch; task #107 set to `needs_fixes`). It re-applied 16 mutations
of batches G and H (all killed), added three of its own (V-11, V-13, V-15b; all killed), confirmed
Specification P2 is stated accurately and left open, confirmed the offline case realises an empty
cache and the lockfile gained exactly one package, confirmed no credential extraction in the
generated tests prints a value, and found no real secret in any changed file or cited log. It
found sixteen defects: **one code blocker** (D1: seventeen interpolated renderings across eleven
lines in ten assertion messages of `verify.rs`, a credential-handling file, which the kernel's
diagnostics gate refuses; three of the ten assertions predate the round and were pulled into the
gate's scope by the file's new subject — the gate's own leg had failed on exactly that six minutes
after the agent's chain found it) and fifteen
record defects (a fabricated log citation, stale test and injection-point names, an over-counted
mutation batch and its cascaded totals, 37 for 38 boundaries, doubled row text, stale
"not received" sentences, an unfilled placeholder, a mislabelled "green" log, an unscoped "nothing
weakened" sentence, a rollback branch the sweep does not reach, a proxy shape that passes verbatim,
two inconclusive M-H8 attempts, a Phase 009 row count). D1 was fixed in `8c72414` and `0d62313`
(fixed labels, indices where a case must be named; the diagnostics gate passes; both gate legs
green on `0d62313`); every record defect is corrected in this section's file and the evidence,
ledger, and limitations, in the commit that carries this paragraph. Task #107 returned to
`coding_done` for a fourth pass.

**The fourth pass** (on `20705ea`, 20:06–20:20) found D1 resolved, every suite it may run green,
Specification P2 still open and unamended, no secret anywhere, and seven documents-only defects:
a corrected citation of mine that pointed at a transcript the row runner had overwritten (the
failed `authonly` attempt at 18:18:55 was replaced by the 18:21 re-run), a sentence of §3f that
still said the two axes were not received, the mutation ledger's header totals, the D1 count
stated two ways, the unscoped "nothing weakened" sentence, an unevidenced cause for the two
inconclusive M-H8 attempts, and a §5 reference to a defect §5 did not record. Six of the seven
were corrected in `05e9818`; the seventh survived in a second copy of the same citation.

**The fifth pass** (on `05e9818`, 20:53) found those six resolved and matching the logs, the CI
paragraph matching the check runs to the second, Specification P2 still open and unamended, no
secret, and four residuals: the citation remnant in the evidence's §12 claims table, a
"thirteen" that the log says is twelve, a commit enumeration that counted three corrections twice,
and the sentence above that called all seven corrected. All four are corrected in the commit
that carries this paragraph; its verdict on that commit is in the ledger (task #107) and in the
round's final report.

### 3f. What the round did not do

- It did not have the Standards or Specification findings until after the Native axis had been
  answered and pushed; §3b and §3c answer them once received, with one (Specification P2) left
  to the maintainer, who decided it after the sixth validation pass (§3g).
- It did not make the rename-failure branch of `apply::commit` reachable by a test: a rename after
  a successful stage in the same directory fails only on a cross-device move. The rollback
  function that branch calls is tested directly, and the injected-failure branch beside it is
  tested end to end (L-14).
- It did not quote table and column identifiers; it refuses reserved words instead, from a
  curated list that a future engine keyword could fall outside of (L-15).

### 3g. The FR-048 decision round — `--overwrite-unchanged` (2026-09-05, after the sixth pass)

**The decision.** With PR #62 validated and unmerged, the maintainer chose the third of the
choices §3c listed: gate regeneration behind the explicit `--overwrite-unchanged` flag. The
semantics as given, verbatim in substance: without the flag, an existing target that differs from
the render is never written — one whose current digest (complete, or with its managed marker
block removed for a marked file) matches its recorded provenance is classified and reported
*regenerable* and requires the flag, and any conflict fails the whole operation without changing
the project; with the flag, only targets whose digest matches their recorded provenance are
regenerated — never user-edited content, unknown ownership, missing provenance, conflicting marker
content, or a file outside the generator's ownership — and the flag waives neither validation nor
a conflict; `--dry-run` uses exactly the same classification, reports what would be written,
regenerated, edited, or refused, and writes nothing; the behaviour applies to every
existing-project generator; FR-048, SR-009, `data-model.md`, `command-surface.md`, the help text,
and generated documentation are updated consistently and the specification is not changed to
permit implicit overwrites.

**What changed — source head `2b3e4a8`, one signed subject-only commit.**

| Where | What |
|---|---|
| `crates/renvor-cli/src/generate/apply.rs` | `plan(project, planned, overwrite_unchanged)`; a new `Refusal { Changed, OverwriteRequired }`; `OVERWRITE_FLAG`; the refusal is built by `refused()` from the refusing paths in plan order — `details.reason` (`changed_since_generation` whenever a changed path is among them, else `overwrite_required`), `count`, `paths` (both kinds), `changed`, `regenerable`, and `flag` (present whenever a regenerable path is among them); the message names paths and the flag, never contents. An `Edit` — a marked block, or the lockfile the merged build resolves — needs no flag, because it touches the managed region only |
| `crates/renvor-cli/src/config/flags.rs` | `--overwrite-unchanged` on each of `migration`, `resource`, `auth`; `auth`'s help says it needs the flag |
| `crates/renvor-cli/src/commands/generate.rs` | `parts()` hands path, action, and the flag to `run(…, dry_run, overwrite_unchanged)` from one place; `main.rs` calls it |
| `crates/renvor-cli/src/exit.rs` | `GenerationConflict`'s doc names both kinds and the details |
| `crates/renvor-cli/tests/cmd/help-generate.trycmd` | `renvor generate --help` and `renvor generate auth --help` pinned byte for byte, the flag among the options |
| `contracts/command-surface.md` 1.4.0, `contracts/json-output.md` | the classification table with the two refusals, the flag row, `auth`'s row, the `generation_conflict` row's details |
| `governance/phase-011-limitations.md` L-6 | states the flag |
| `specs/011-…/spec.md` FR-048 and SR-009, `data-model.md` §4 | state the flag and the refusal; untracked under `specs/` (gitignored), edited in place on this machine |

Generated documentation was checked and left alone: no template states the old rule. The
migrations README says only that `renvor generate migration <name>` writes a pair; the routes
marker comment and the support module's comment name the generators without describing overwrite
behaviour.

**Red, then green.** The two binary tests were written before the implementation and run against
the reviewed head: `red-fr048.log` is `a9f873e` exported to a scratch directory with the new
`tests/generate.rs` copied over it, its own target directory, 22:37:49–22:38:07 —
`a_regenerable_file_is_refused_without_the_flag_and_replaced_only_with_it` FAILED at its first
assertion (`a regenerable file was replaced without the flag`, the envelope showing
`"action":"regenerate"` and `"written":1`), and
`a_changed_file_is_refused_with_the_flag_and_a_mixed_plan_writes_nothing` FAILED because the
refusal carried no `reason`. The same two failures were seen on the worktree before the change
(the first run of the tests, in the session transcript only), the second then for a wrong reason of the test's own — half a
migration pair removed trips the version check first — corrected before the red run above.

| Required case | Test |
|---|---|
| regenerable file without the flag → refused, zero writes | the regenerable binary test (first half); `apply::tests::absent_identical_untouched_and_modified_are_the_four_cases`; `auth_tests::adding_auth_needs_the_flag_and_replaces_only_what_the_generator_owns`; the `authadded`/`authaddedmysql` rows' step 4 |
| the same file with the flag → regenerated | the regenerable binary test (`written == 1`, the file equals the render, only it and the record moved); the same unit tests; the rows' step 4 |
| user-edited file with the flag → still refused | the mixed-plan binary test; `absent_identical_…` (both flags); `a_line_outside_the_markers_survives_an_edit_and_refuses_a_re_render_with_the_flag` |
| edit outside a managed marker → refused / preserved | `a_line_outside_the_markers_…` (the resource's marker edit keeps the line; the auth re-render refuses, flag or no flag); `apply::tests::a_marker_edit_never_claims_the_rest_of_the_file` |
| unchanged managed marker with the flag → only its owned region changes | `adding_auth_needs_the_flag_…` (the resources block is byte-identical across the re-render while the file changes) |
| mixed create / regenerate / conflict plan → zero writes | the mixed-plan binary test (two absent, one regenerable, one changed; the whole tree byte-identical after every refusal, the absent pair still absent) |
| dry-run classification equals the real plan | the regenerable binary test (`dry["error"] == refused["error"]` without the flag; `dry.files == done.files` with it) |
| repeated successful execution is idempotent | the regenerable binary test (again with the flag, again without: `written == 0`, every action `unchanged`, the tree identical) |
| JSON and human output identify the flag without exposing contents | the regenerable binary test (`details.flag`, the human exit 3 naming `--overwrite-unchanged` and the path; neither stream carries a line of either version of the file); the mixed-plan test's canary is absent from both streams |
| unknown ownership / missing provenance never regenerates | `apply::tests::a_file_never_generated_is_a_conflict_even_without_a_record` (both flags, `reason = changed_since_generation`) |
| the flag reaches every action | `commands::generate::tests::every_generate_action_carries_the_overwrite_flag_and_it_is_off_by_default` |

**Suites on the source head.** `cargo test -p renvor-cli --bin renvor` 313 passed, 0 failed, 1
ignored (310 before: the three new tests); `--test generate` 11 passed (9 before); `--test cli` 4
(the new help snapshot among them); `--test presentation` 17; `--test snapshots` 1;
`renvor-core --test diagnostics` 2; `xtask` 36; `cargo clippy -p renvor-cli --all-targets -D
warnings` clean; `cargo fmt --all --check` clean. The two rows that add the auth starter,
`authadded` (22:28:48–22:30:35) and `authaddedmysql` (22:30:35–22:32:36), passed with the new
step — refused without the flag, added with it, a rerun writing nothing — on the working tree
that became `2b3e4a8` (`row-authadded.log`, `row-authaddedmysql.log`, `rows-summary.log`); the
gate legs below run both again as part of the census.

**Mutations, batch I** (`phase-011-mutation-ledger.md`): ten scripted mutations on the committed
head, ten killed by the test named in advance — the flag not required, the flag waiving a changed
file, a dry run classifying with the flag on, the refusal without its flag detail, the regenerable
list dropped, the primary reason ignoring changed paths, `parts` dropping the flag for `auth`, and
an unrecorded file treated as owned. M-I1's first form was a compile error (a `match` made
non-exhaustive), which kills nothing worth recording; it was replaced by a compiling form (the
`(true, _)` arm) and killed by the behavioural test. One defect of the round's own tooling is
recorded in the evidence §5: the batch script's per-run `git checkout` of the two mutated files
reverted the not-yet-committed implementation on its first launch; the edits were re-applied from
the same scripts, the suites re-run green, the implementation committed, and the script now refuses
to run over uncommitted work.

**Gates.** Both legs on `2b3e4a8` (tree `488ca17`), clean tree, one after the other:
`cargo +1.94.0 xtask verify` 22:40:13–23:03:27 and `cargo +stable xtask verify`
23:03:27–23:25:56, 9/9 steps and exit 0 each, 2209 passed / 0 failed / 5 ignored each (the five
over the continuation's 2204 are this round's tests), census 87/87 each — the evidence §13 has
the table and the log names. The source head was pushed fast-forward at 23:27; the pull
request's checks and the seventh validation pass are recorded by the commit after the one that
carries this section.

## 4. What this record does not claim

- That any review above was independent. The reviewer is an agent commissioned by the same
  session that wrote the code; the maintainer is the sole human.
- That W-023 or W-024 is closed by anything in this file. Closure is a ledger entry in
  `waivers.md`, bound to a head and tree, written only after §2 reports every control satisfied.
- That the phase is complete. The merge-authority checkpoint is the maintainer's decision.
