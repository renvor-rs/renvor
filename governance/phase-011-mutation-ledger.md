# Phase 011 — Mutation Ledger

**Companion to**: [`phase-011-evidence.md`](phase-011-evidence.md)
**Phase**: 011 — Generators, the auth starter, and the testing kit
**Total**: **24 controlled mutations** — **23 killed by a named test, 1 survived its first run**
(M-A1: the test asserted the code, the `supported` detail, and the prose, but not the
machine-readable `reason`; the test was strengthened, the mutation re-applied and killed) — plus the
**three census negative controls** (a starter row renamed, deleted, and `cfg`-gated), each of which
must make `cargo xtask census` fail naming the vanished row, and one non-mutation (a first attempt at
M-F1 that did not compile, recorded and replaced).

A mutation is applied to the implementation, the test that should notice is named **in advance**,
the suite is run, and the pristine file is restored from git and re-run green (the harness asserts
the restored file is byte-identical to the pristine copy — a restore that does not recompile checks
nothing, so every restore is followed by a run). A mutation nothing catches is recorded with why,
never re-run until it passes. Harnesses are scratch scripts (`mutate-a.py`, `mutate-f.py`), not
tracked.

## Totals by batch

| Batch | Mutations | Killed | Survived | Notes |
|---|---|---|---|---|
| A — the configuration model | 10 | 10 | 1 first run (M-A1), killed after the test was strengthened | `crates/renvor-cli/src/config/model.rs`; re-run in full on 2026-09-05 because the batch's original records were not preserved |
| B — auth verification confirmation | 1 | 1 | 0 | `crates/renvor-auth/src/service.rs`; the adapter proof ran green on all four rows |
| C — the starter matrix | 0 + 3 controls | 3 controls fired | — | the census's negative controls are the mutations of this batch: a row renamed, deleted, and `cfg`-gated |
| D — the two entry findings | 2 | 2 | 0 | README count; the `cfg`-gated variant |
| E/F — L-17, the testkit client, the apply engine, the record, verification, the generators, the manifest comment, the Windows path | 11 | 11 | 0 | M-F1 … M-F11; F3–F9 run on head `d8e3a44` by `mutate-f.py`, final pristine run 288 passed; F10 on the closure head; F11 on `f95ab6b` |
| G — the correction round (every Native Codex fix, the transactional commit) | 20 distinct (22 runs) | 20 | 2 in their first form: M-G10 killed after the control was strengthened; M-G15's first edit abandoned as unreachable and re-targeted (M-G15, M-G15b) | run by `mutations-g.py` in an isolated copy of the working tree (`mutsrc`, its own build directory) so the census rows compiling beside it could not pick up a mutation; logs `mutations-g.log`, `mutations-g-<id>.out` |
| H — the Standards and Specification fixes (the continuation of the round) | 8 scripted + 1 by history | 8 + 1 | 0 (M-H8 needed three attempts; the first two failed earlier on a rustfmt width in the same template and are inconclusive, not survivals) | `mutations-h.py` and `mutations-h8.sh` in the same isolated copy; M-H8 is killed by the **generated** negative control while `renvor new` verifies the staged starter, so its kill is a refused generation; M-H9 is the offline case's own red run on the unfixed tree (`red-offline.log`), which is the mutation "the facade offers no interrupt wait and the starter enables `signal` itself" applied by history rather than by script |

## The entries worth reading

- **M-H8 is killed by a generated test, inside generation.** The mutation lives in a template; the control that kills it is rendered into the starter's `tests/support/mod.rs` and runs when `renvor new` verifies the staged project, so the kill is a refused generation (exit 3, "does not pass its own tests") rather than a red `cargo test` in this repository — the same place a consumer's copy would catch it.
- **M-A1 survived, then killed.** `.with("reason", "no_token_issuance_route")` → `"reserved"`:
  `api_and_full_are_unsupported_values_not_reservations` asserted the exit code, the `supported`
  detail, and that the prose says why, and not the machine-readable `reason` a script keys on. The
  test now pins the detail (commit `f0fdd89`); re-applied → FAILED at model.rs:1908; restored → 27
  passed (`m-a1-rerun.log`, 07:35; reproduced independently by the second validation pass,
  `mutations-val2.log`).
- **M-F1 first attempt was a non-mutation.** Returning `parse_address(...)` inside the loop left
  `found` unassigned (E0282); replaced by `break;` after the first `for=`, which reproduces the
  Phase 010 behaviour exactly and is killed by
  `a_malformed_parameter_after_for_refuses_the_header_like_one_before_it`.
- **M-F4 is the "never overwrite user files" control.** `if recorded(&item.path) == Some(digest)`
  → `if true` makes every modified file look untouched — the one mutation that would silently
  overwrite an author's edit — and `absent_identical_untouched_and_modified_are_the_four_cases`
  kills it.
- **M-G10 survived its first form, and the control was the weak one.** Removing the mail
  route's `if !loopback { return … NotPermitted }` left `is_loopback()` in the file, and the
  render assertion had only asked for that token. It now asks for the refusal itself — the three
  lines — before the services are read; re-applied → FAILED. The generated module's own unit test
  (a forged `203.0.113.9` peer) is the behavioural control and runs in every mail-without-auth
  row.
- **M-G15's first form mutated a branch no test reaches.** It removed the rollback from the
  *rename-failure* branch of `apply::commit`; the injected-failure branch beside it still rolled
  back, so the transactional test passed. A rename after a successful stage in the same directory
  fails only on a cross-device move, which no test can arrange (L-14). The mutation was
  re-targeted twice: M-G15 disables the restore inside `roll_back` itself, killed by the new
  direct test of that function; M-G15b removed the rollback from the injected placement branch,
  killed by the two-point transactional test of the Native round — since replaced by the boundary
  sweep, which kills the same edit in its current form (`generate-place-<n>` returning without
  `roll_back`; the validation agent's V-15b, 26 s).
- **The rename control was a no-op the first time.** The starter rows are generated by a `row!`
  macro; a script that renamed a `fn` that does not exist changed nothing, and the census it ran
  was effectively an unmodified one — which is how the full-row `session` shadowing regression was
  caught (`phase-011-evidence.md` §5). The script now edits the `row!(…)` invocation and asserts
  the replacement matched; all three controls fired on `3df5589` and again on `d8e3a44` (§Batch C).

## Per-batch tables

### Batch A — `crates/renvor-cli/src/config/model.rs` (re-run 2026-09-05)

| ID | Edit | Killed by |
|---|---|---|
| M-A1 | `reason = no_token_issuance_route` → `reserved` | **survived**, then `api_and_full_are_unsupported_values_not_reservations` after the reason detail was pinned |
| M-A2 | the `api`/`full` refusal's code `UnsupportedValue` → `ReservedForLaterPhase` | `api_and_full_are_unsupported_values_not_reservations` (model.rs:1891) |
| M-A3 | a duplicated capability deduplicated silently (`if !set.insert(capability) && false`) | `a_duplicate_capability_is_refused_and_none_with_a_name_is_refused` |
| M-A4 | `none,x` read as an empty selection (`names.len() == 1` → `!names.is_empty()`) | `a_duplicate_capability_is_refused_and_none_with_a_name_is_refused` |
| M-A5 | `--auth session` without a database accepted | `a_session_starter_without_a_database_is_refused` |
| M-A6 | `--auth session` without the `mail` capability accepted | `a_session_starter_without_the_mail_capability_is_refused` |
| M-A7 | `jobs` without a database accepted | `jobs_without_a_database_is_refused` |
| M-A8 | a selection that needs the framework accepted without `--framework-path` | `a_selection_that_needs_the_framework_without_a_path_is_refused_not_recorded` |
| M-A9 | a framework checkout without `Cargo.lock` accepted | `a_framework_without_a_lockfile_is_refused_before_any_write` |
| M-A10 | `--container-cache none` beside the `cache` capability accepted | `the_cache_capability_with_containers_wires_the_container_cache` |

Final pristine run: `config::model` 27 passed.

### Batch B — `crates/renvor-auth/src/service.rs`

| ID | Edit | Killed by |
|---|---|---|
| M-B-01 | `confirm_verification` no longer calls `mark_email_verified` | `service::tests::confirming_a_verification_records_the_instant_and_a_replay_changes_nothing` ("the confirmation must record when the address was verified"); positive control: 185 passed with the write present; `marking_an_address_verified_records_the_first_instant_only` green on all four rows |

### Batch C — the census's negative controls (`phase-011-evidence.md` §4)

Run on head `3df5589` and re-run on the checkpoint head `d8e3a44` (07:29, all three fired again: 10m13s / 9m42s / 9m56s); the script edits the `row!` invocation and asserts the replacement matched.

| Control | Edit to `crates/renvor-cli/tests/starter_matrix.rs` | Expected | Observed |
|---|---|---|---|
| rename | `row!(storage_alone_generates_and_proves_itself, 6)` → `…_renamed` | `cargo xtask census` FAILS naming `storage_alone_generates_and_proves_itself` as not reported | **fired**: exit 1, `[4/9] … FAILED — row renvor-cli::storage_alone_generates_and_proves_itself did not report in` (renvor-cli group 10m46s) |
| delete | the `row!(storage_alone_…)` line removed | the same failure | **fired**: exit 1, the same line (9m51s) |
| cfg-gate | `#[cfg(feature = "never-set")]` above the row | the same failure | **fired**: exit 1, the same line (9m54s) |

### Batch D — the entry findings

| ID | Edit | Killed by |
|---|---|---|
| M-D1 | `crates/renvor-jobs/README.md` "five" → "four" | `the_readme_states_the_number_of_pairs_the_directory_holds` |
| M-D2 | the `#[cfg(feature = "smtp")]` removed from `MailerSource::Configured` alone | `cargo check -p renvor-mail --no-default-features --lib` fails with E0004 at the two gated arms; removing all four gates restores the warning that `port_crates_are_warning_free_without_features` refuses |

### Batch E/F

| ID | Edit | Killed by |
|---|---|---|
| M-F1 | `renvor-http` `from_forwarded`: `break` after the first `for=` (parameters after it unread — 010/L-17) | `identity::forwarded::tests::a_malformed_parameter_after_for_refuses_the_header_like_one_before_it` (forwarded.rs:263) |
| M-F2 | `renvor-testkit` `client::http`: `Content-Type: application/json` on every request | `client::tests::an_empty_body_is_sent_with_a_zero_length_and_no_content_type` (client.rs:185; `m-f2.log`, 07:35; also reproduced by the second validation pass) |
| M-F3 | apply engine: a byte-identical file `Unchanged` → `Regenerate` | `generate::apply::tests::absent_identical_untouched_and_modified_are_the_four_cases` (apply.rs:339) |
| M-F4 | apply engine: a modified file treated as untouched (`if true`) | `generate::apply::tests::absent_identical_untouched_and_modified_are_the_four_cases` (apply.rs:356) |
| M-F5 | the record writes an empty digest for every file | `generate::record::tests::the_record_names_every_file_with_its_digest_and_not_itself` (record.rs:171) |
| M-F6 | `RENVOR_TEST_REQUIRE_DATABASE` added to the sealed environment's allow-list | `generate::verify::tests::verification_runs_in_a_sealed_environment` (verify.rs:461) |
| M-F7 | a failing verification check reported as success (`if false`) | `generate::verify::tests::a_project_that_does_not_compile_is_a_generation_failure` (verify.rs:470) |
| M-F8 | the migration version's month unpadded (`{month}`) | `commands::generate::tests::the_version_is_the_utc_instant_to_the_second` (generate.rs:336) |
| M-F9 | a resource name need not start with an upper-case letter | `commands::generate::resource_tests::names_are_pascal_case_and_columns_are_the_closed_set` (generate.rs:940) |
| M-F10 | `renvor.toml.j2`: the comment names `RENVOR_AUTH__CSRF_KEY` again (the first-pass defect) | `commands::new::tests::the_manifest_names_the_variables_the_starter_reads` (new.rs:1249; `m-f10.log`, 07:42) — the pin the second pass asked for (D-7) |
| M-F11 | `config/model.rs` `without_verbatim_prefix`: the `\\?\` disk prefix is never removed | `config::model::tests::a_verbatim_windows_prefix_is_removed_from_the_canonical_path` (model.rs:2232; `m-f11.log`, 10:51) — the Windows defect PR #62 found |

### Batch G — the correction round (2026-09-05, isolated copy of the tree after the fixes)

| ID | Edit | Killed by |
|---|---|---|
| M-G1 | apply engine `plan`: compare the whole-file digest again for a marked file | `generate::apply::tests::a_marker_edit_never_claims_the_rest_of_the_file` |
| M-G2 | apply engine `commit`: an `Edit` of a marked file records the merged digest again (`if true \|\| …`) | the same test |
| M-G3 | `plan_auth`: the existing `migrations/0001_*` pair is planned again (the `continue` removed) | `commands::generate::auth_tests::adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward` |
| M-G4 | `plan_auth`: no recorded resource is rendered again (`.take(0)`) | `…auth_tests::adding_auth_renders_every_recorded_resource_again_with_its_guards` |
| M-G5 | `allocate_version`: the instant is returned without moving past the taken versions | `commands::generate::tests::a_version_is_allocated_past_every_version_the_directory_holds` |
| M-G6 | `colliding_versions`: never reports a collision (`if false && …`) | `tests/generate.rs::an_import_refuses_a_version_another_migration_already_holds` |
| M-G7 | `is_reserved_sql_word`: true only for the empty identifier | `commands::generate::resource_tests::a_reserved_sql_word_is_refused_as_a_table_or_a_column` |
| M-G8 | `check`: the cookie name need only be non-empty again | `commands::check::tests::the_auth_table_holds_the_values_the_generator_writes_and_nothing_else` |
| M-G9 | storage template: the session check removed from `get` | `commands::new::tests::the_storage_read_route_requires_a_session_when_auth_is_on` |
| M-G10 | mail template: the loopback refusal removed (the `matches!` kept) | **survived the first assertion**; killed by `the_session_free_mail_notice_answers_the_loopback_peer_only` once it asked for the refusal lines (see above) |
| M-G12 | testkit `boot`: the booted application's state not attached | `renvor_testkit::app::tests::a_request_carries_the_state_the_booted_providers_registered` |
| M-G13 | kernel `boot`: the state map never frozen into `shared` | the same test (through the kernel) |
| M-G14 | `new`: the record written before verification again, and not after | `tests/generate.rs::the_record_digests_are_the_placed_files_including_the_resolved_lockfile` |
| M-G15 | apply engine `roll_back`: a previous file is not restored (`Some(_) => Ok(())`) | `generate::apply::tests::a_rollback_restores_what_was_placed_newest_first_and_names_what_it_could_not` (first form: the rename-failure branch, unreachable — see above) |
| M-G15b | apply engine `commit`: a placement boundary's injected failure returns without rolling back | at the time, `tests/generate.rs::a_failure_during_commit_leaves_the_project_unchanged`; now `a_failure_at_every_placement_boundary_leaves_the_project_byte_identical` (the same edit at `generate-place-<n>`, re-killed by the validation agent as V-15b) |
| M-G16 | `flags`: `conflicts_with = "import"` removed | `tests/generate.rs::a_name_beside_an_import_is_refused_rather_than_ignored` |
| M-G17 | `prompts`: the capabilities question is not a multi-select (a renamed call) | `config::prompts::tests::the_capabilities_question_is_a_multi_select_of_exactly_the_five` (a compile error is a kill: the scan reads the source) |
| M-G18 | cache template: the fixed key `ping` again | `commands::new::tests::the_cache_ping_uses_a_key_of_its_own_per_request` |
| M-G19 | main template: the configured address printed instead of the bound one | `commands::new::tests::the_starter_reports_the_address_it_actually_bound` |
| M-G20 | testkit: a fresh `RunIdentifier` per request again | `renvor_testkit::app::tests::every_request_carries_the_applications_one_run_identifier` |

There is no M-G11: the identifier was skipped when the batch was written and is left unused so the
log files keep their names.

### Batch H — the Standards and Specification fixes (2026-09-05, isolated copy of the tree after the fixes)

| ID | Edit | Killed by |
|---|---|---|
| M-H1 | testkit sweep: the failure prints the canary and the swept line again | `renvor_testkit::app::tests::a_failed_sweep_names_the_canary_by_index_and_never_by_value` |
| M-H2 | `without_proxy_credential`: the value is returned unchanged (no credential stripped) | `generate::verify::tests::a_proxy_credential_never_reaches_the_sealed_environment` |
| M-H3 | `redacted_output`: the child's text is returned as it was | `generate::verify::tests::a_childs_output_is_reported_without_url_credentials_or_control_characters` |
| M-H4 | apply engine: the `generate-record` boundary removed | `tests/generate.rs::a_failure_at_every_placement_boundary_leaves_the_project_byte_identical` (an injection point that does not fire is a run that succeeds where it must fail) |
| M-H5 | apply engine: the record boundary fails without rolling back | the same test (the project is left changed) |
| M-H6 | README template: the unqualified "requires a clean exit" sentence restored | `commands::new::tests::the_generated_readme_states_the_windows_shutdown_limitation` |
| M-H7 | starter test template: `"{cookie}"` printed again after the helper | `commands::new::tests::the_generated_tests_fail_without_printing_a_credential` |
| M-H8 | support template: `session_cookie_of` prints the cookie's value in its panic | the generated `a_failed_secret_extraction_names_nothing_secret`, run by `cargo test` inside `renvor new`'s verification of an auth starter: generation was **refused** (`mutations-h-M-H8.out`, exit 3, "does not pass its own tests"). Two earlier attempts (`mutations-h.log`: "SURVIVED-or-UNKNOWN exit=3") failed before the tests ran, on the rustfmt width of a generated line in the same template, and are inconclusive rather than survivals |
| M-H9 | the facade's `shutdown_signal` absent and the starter enabling Tokio `signal` itself (the tree before the fix) | `tests/offline.rs::a_starter_is_generated_with_networking_unavailable_from_the_cache_a_framework_build_leaves` — `red-offline.log`: `no matching package named signal-hook-registry found` |
