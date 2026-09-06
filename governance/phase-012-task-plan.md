# Phase 012 — Task plan

**Companion to**: [`phase-012-specification-and-decision-brief.md`](phase-012-specification-and-decision-brief.md) (the specification this plan implements; every requirement identifier below is defined there) · [`phase-012-security-carryover.md`](phase-012-security-carryover.md) · `PLAN.md` §6.1 (specify → clarify → plan → ADRs → checklist → tasks → analyze → implement → converge → review) · `CONSTITUTION.md` §Development and Phase Workflow 5 ("Tasks MUST be dependency ordered, independently verifiable, and include tests, documentation, security, migration, and compatibility work")
**Drafted**: 2026-09-07, against `main` at `7281e4f91aeb56695d6eceb322065e5f5fca04ef`, on branch `docs/phase-012-decision-brief`
**Status**: **PLAN — approval requested at the specification checkpoint.** No task is started. Batch B0 is this planning pull request. Nothing here implements, merges, accepts a decision record, closes a limitation, tags, publishes, or deploys.
**Working copy**: `specs/012-rest-documentation-and-production-examples/tasks.md` under the gitignored `specs/` tree is the same text; this tracked file is the clone-visible mirror and the authority if the two differ.

---

## 1. Requirements → tasks → acceptance tests → evidence

One row per requirement. "Evidence" names where the proof is recorded when the task is done; every implementation batch appends to `governance/phase-012-evidence.md` (created by B1a) with the head, the run identifiers, and the compiler identities of the legs that proved it.

### 1.1 L-2 — toolchain (brief §5, §6, §8)

| Requirement | Task(s) | Acceptance tests (named; RED first, then GREEN) | Evidence |
|---|---|---|---|
| FR-012-1 (the pin; independent reads; refusals) | **T-012-03** templates; **T-012-04** framework-checkout reads | `every_generated_tree_declares_a_pin_and_a_rust_version` (snapshots, all skeleton variants and all census rows); `a_starter_pin_is_the_checkouts_channel_and_its_msrv_is_the_manifests` (a checkout copy with channel `1.95.0` and `rust-version` `1.94.0` renders pin 1.95.0, rust-version 1.94.0); `an_alias_channel_is_refused_not_resolved` (`stable`, `beta`, `nightly`, `nightly-2026-01-01`, a custom name, a `path`); `a_malformed_channel_is_refused_by_name`; `a_pin_below_the_manifests_msrv_is_refused_by_name`; `an_unreadable_msrv_is_refused_by_name`; positive control: the framework's own checkout | evidence §L-2; the JSON envelopes of each refusal retained |
| FR-012-2 (`rust-version` = MSRV) | T-012-03 | covered by the first two tests above; `a_skeletons_pin_and_rust_version_are_the_generators_msrv` (`doctor`'s constant and the rendered values asserted equal — the chosen default, D-L2-4) | snapshots at template version 8 |
| FR-012-3 (README) | T-012-03 | `the_readme_names_the_pin_the_msrv_the_rustup_floor_and_the_bare_toolchain_rule` (string assertions on the rendered README, both tree kinds) | snapshots |
| FR-012-9 (Dockerfile from the pin) | T-012-03 | `the_dockerfile_builder_tag_equals_the_rendered_channel`; `the_builder_sets_rustup_auto_install_0` | snapshots |
| FR-012-4, FR-012-5a (the record, measured; which operations write it) | **T-012-05** record v2; **T-012-08** evidence probe | `the_record_is_written_after_verification_from_the_sealed_probe` (RED: a record written from the process environment); `generate_resource_and_migration_leave_verified_with_byte_identical`; `generate_auth_rewrites_verified_with_with_operation_auth`; `a_stale_rustc_info_json_is_never_read` (a planted cache file naming another compiler; the record names the probe's) | evidence §L-2 |
| FR-012-5b (reader dispatch) | T-012-05 | `a_legacy_record_without_a_version_is_accepted_and_reports_unknown`; `a_version_2_record_is_validated_strictly`; `a_newer_record_is_refused_by_name_before_any_plan` (`record_unsupported`, `details.record_version`, `details.supported`; the working tree byte-identical afterwards) | C-1 1.5.0 and C-2 registry rows; evidence |
| FR-012-5c (old reader / new record, documented) | T-012-05, **T-012-02** | a doc test in `record.rs` and the README string; a control that a `renvor` built from `7281e4f` reading a version-2 record fails with the unknown-field error — run once by hand from the retained binary and quoted, not a CI test | evidence §L-2 |
| FR-012-5d (evidence freshness) | T-012-05 | `check_reports_verified_with_as_historical_after_a_generate_resource`; `check_reports_verified_with_as_current_on_an_untouched_tree`; JSON `data.verified_with.historical` | evidence |
| FR-012-5e (snapshot policy; JSON fields) | T-012-05 | the snapshot suite (`[toolchain]` pinned, `[verified_with]` not); `json/new_success.json` and `json/generate_*.json` fixtures carry `data.toolchain` and `data.verified_with` | snapshots, `tests/json` |
| FR-012-6 (forced/dropped variables; every invocation) | **T-012-06** seal v2 | the three existing seal tests unchanged; `the_seal_forces_rustup_auto_install_0_and_drops_the_install_server_variables`; `rustfmt_at_generation_runs_under_the_seal`; `doctor_probes_run_under_the_seal`; `dev_and_routes_force_rustup_auto_install_0_without_sealing` | evidence |
| FR-012-7a (rustup floor, ordered first) | **T-012-07** preflight | `rustup_below_1_28_1_is_refused_before_any_proxy_runs` (a stub `rustup --version` on a `PATH` with **no** `cargo`: exit 5, `details.tool = "rustup >= 1.28.1"`); `an_unparseable_rustup_version_is_refused` | evidence |
| FR-012-7b (resolution probe; refusals by name) | T-012-07 | `a_pinned_but_absent_toolchain_is_tool_missing_not_a_compiler_error` (exit 5, `details.tool`, `details.remedy`, nothing staged); `a_compiler_below_the_msrv_is_refused_by_name`; `a_pin_without_rustfmt_or_clippy_is_refused_naming_the_component`; `the_attribution_strings_of_rustup_1_28_1_and_1_29_0_are_pinned` (a table test; unknown text → `unknown`) | evidence; the refusal envelopes retained |
| FR-012-7c (proxy detection) | T-012-07 | `a_bare_toolchain_is_recorded_as_no_rustup` (a directory of symlinks to a real toolchain's `rustc`, `cargo`, `rustfmt`, `cargo-clippy` binaries, `rustup` off `PATH`: `proxy = false`, `selected_by = "no_rustup"`, exit 0, the notice); `a_hidden_rustup_with_proxies_on_path_is_not_no_rustup` (`rustup` off `PATH`, the proxies on it: `proxy = true`, the real attribution) | evidence |
| FR-012-7d, FR-012-7e (evidence probe; sanitization) | **T-012-08** | `the_record_names_cargos_effective_compiler_not_paths` — controls (i) `RUSTC` to another installed toolchain's binary, (ii) `[build] rustc` in the sealed `CARGO_HOME`'s `config.toml`, (iii) a pass-through `RUSTC_WRAPPER` (`wrapper = true`, identity unchanged), (iv) the trivial case; `an_unreadable_compiler_identity_is_project_verification_failed_and_redacted` (a `RUSTC` shim printing terminal control bytes and a fake credential: `compiler_identity_unreadable`, neither byte in any stream); `no_flag_value_enters_the_record` (`RUSTFLAGS` carrying a marker string; the record carries `rustflags = true` and not the marker) | evidence |
| FR-012-8, SR-012-2 (the notice; no silent fallback) | **T-012-09** | `an_override_proceeds_with_the_notice_on_stderr_and_both_values_in_the_record`; `nothing_is_printed_when_the_pin_is_what_ran`; the stderr/stdout discipline test of C-1 | evidence |
| SR-012-1 (no provisioning) | **T-012-12** | `an_absent_pin_with_an_unroutable_dist_server_is_refused_in_bounded_time_and_installs_nothing` (asserts the `RUSTUP_HOME/toolchains` listing is unchanged, by reading the directory, not by `rustup toolchain list`) | evidence |
| SR-012-3 (wrapper residual as a ledger row) | **T-012-17** | the row exists in `phase-012-limitations.md` with owner, target, consequence | ledger |
| SR-012-4 (no listing/installing commands) | T-012-07, **T-012-13** | `the_generator_and_doctor_invoke_no_rustup_subcommand_but_version_and_show` (a `rustup` shim that fails on any other subcommand, on `PATH` for every command) | evidence |
| SR-012-5 (offline contract preserved) | T-012-12 | the existing `tests/offline.rs` case unchanged and green; `an_offline_generation_with_the_pin_installed_passes_and_records_it`; `an_offline_generation_with_the_pin_absent_is_tool_missing_and_fetches_nothing` (an unroutable registry as well as dist server) | evidence |
| FR-012-12, FR-012-13, FR-012-14 and C-sel-1…4 (selection across staging and placement) | **T-012-10** | `generate_auth_stages_beside_the_project_and_shares_its_ancestors`; `a_divergent_scratch_resolution_is_refused` (the negative control of C-sel-3); `c_sel_1_a_closer_toolchain_file_beats_a_farther_directory_override`; `c_sel_2_a_directory_override_on_the_project_beats_its_file`; `c_sel_3_a_legacy_tree_under_an_ancestor_pin_resolves_that_pin_in_the_scratch_copy`; `c_sel_4_the_environment_beats_the_file` (= AC-012-6). All four under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` in CI (U-10), `SKIPPED:` locally without two toolchains | evidence; both legs |
| FR-012-10a/b/c (legacy trees) | **T-012-11** | `generate_into_a_template_7_tree_proceeds_with_pinned_none_and_the_notice` (the fixture under `tests/fixtures`, or `legacy_compatibility.rs`'s pattern); `generate_auth_on_a_legacy_tree_inserts_no_pin_and_no_rust_version` (the plan lists neither; `Cargo.toml` re-rendered without the line); `generate_resource_on_any_tree_writes_no_verified_with` | evidence |
| FR-012-11 (`doctor`) | T-012-13 | `doctor_in_a_pinned_directory_reports_the_pin_the_resolution_and_the_components_without_listing`; `doctor_outside_a_project_omits_the_section`; `doctor_below_the_rustup_floor_reports_not_probed`; JSON `data.doctor.toolchain` | evidence |
| AC-012-4 (xtask step 1) | **T-012-14** | `step_1_prints_the_compiler_identity` (an xtask test over its own output); `verification-sequence.md` 2.4.0 | the gate's own output on both legs |
| AC-012-5 (census assertion) | **T-012-15** | `every_placed_record_names_the_legs_compiler` in `starter_matrix.rs`, every row, both legs; negative control: a record edited to another release fails the assertion | both legs, run identifiers |
| AC-012-7 remainder (dated sentences) | **T-012-16** | a docs-scan test that both sentences carry the dated note | `SUPPORT.md`, `rust-toolchain.toml` |
| §6 contract texts; §8 confirmed | **T-012-02** | the revision texts diffed against the brief's §8 table in the pull request; the `version:` fields and status texts move together (the D-L2-0 lesson) | the PR body of B1a |
| ADR-0038 proposed | **T-012-01** | the record exists, `proposed`, with rejected alternatives | `decisions/` |

### 1.2 L-1 — TLS (brief §9)

| Requirement | Task(s) | Acceptance tests | Evidence |
|---|---|---|---|
| D-L1-5 certificates, SR-012-10 (absolute path; `SSL_CERT_DIR` unset) | **T-012-19** materials script | the script's own controls, run in the job: `openssl verify` accepts the positive leaves under the CA and rejects `mismatch` for `localhost`, `expired`, and the `other-ca` leaf; every key `0600`; `openssl version` printed; a test refuses a relative `SSL_CERT_FILE` | the job's step summary |
| D-L1-3 receiver; package review | **T-012-20** | `an_https_export_reaches_the_receiver_and_the_negotiated_version_is_recorded`; `an_export_to_an_untrusted_issuer_is_counted_failed_and_nothing_is_received`; the same for `mismatch`, `expired`, and a plaintext listener at an `https://` endpoint; `cargo deny check` unchanged; xtask step 7's single-provider row green; the local licence check the deferred-verification record asked for (the resolved graph's licences asserted against the same list dependency-review uses) | evidence §L-1; `deny.toml` diff empty |
| Valkey TLS (AC-012-20/21/23) | **T-012-21** | `a_tls_session_to_valkey_completes_against_a_trusted_ca`; `an_untrusted_issuer_fails_boot_with_unavailable_and_no_plaintext_retry`; `a_hostname_mismatch_is_refused`; `an_expired_leaf_is_refused`; `a_plaintext_listener_on_the_tls_port_is_refused`; each negative asserts no command reached the instance (`DBSIZE` unchanged) and that the positive on the trusted endpoint passed in the same run | evidence; both toolchains |
| SMTP TLS (AC-012-20/21/22/23) | **T-012-22** | `an_implicit_tls_submission_completes`; `a_starttls_submission_completes`; `a_relay_without_starttls_is_refused_under_required_starttls`; `a_plaintext_peer_on_the_implicit_tls_port_is_refused`; the untrusted-issuer, mismatch, and expired cases under both securities; each negative asserts the container's message count is zero | evidence |
| SR-012-11 (trust cases in fresh processes) | **T-012-23** the `tls` job | three invocations per suite — trusted, unrelated CA, unset — each a separate process with `SSL_CERT_DIR` unset; the expected outcome per invocation asserted by the job (positive cases pass only under the trusted invocation) | the job's step summary, both toolchains |
| D-L1-7 (version observed) | T-012-20, T-012-23 | the receiver's `protocol_version()` per test; the loopback capture decoded per port (U-3); a 1.2 result explained in the evidence | evidence |
| AC-L1-5, AC-012-24 (the starter `tls` row) | **T-012-24** | `the_tls_starter_row_boots_and_its_generated_test_passes` (both securities); `the_tls_starter_row_fails_boot_without_the_ca_and_never_falls_back`; a grep for `allow_insecure_loopback` finds nothing | evidence; both toolchains |
| AC-L1-6 (templates and README re-read; C-C7 sentence) | **T-012-25** | the three template lines and the README paragraph compared with what the leg did; C-C7 1.1.1 names the run | contract diff; evidence |
| AC-L1-7 (ledger closure) | **T-012-45** only | the rows L-1 and 010/L-1 marked closed **with the measurement**, after the validator's pass; the rows themselves unedited | ledgers |
| SR-012-12, D-L1-4, D-L1-6 (deferrals and exclusions) | T-012-17, T-012-25 | the `ca_file` deferral row; the mTLS and OS-store sentences in the closing measurement | ledger; evidence |
| ADR-0039 proposed | **T-012-18** | the record exists, `proposed` | `decisions/` |

### 1.3 WI-012-5 — database TLS (brief §11)

| Requirement | Task(s) | Acceptance tests | Evidence |
|---|---|---|---|
| the configuration surface (U-9, with 010/L-15) | **T-012-26** | a measurement: which of `sslmode`/`sslrootcert`, `ssl-mode`/`ssl-ca` pass through `ConnectionString` today; a design note; an ADR proposal if the type changes | evidence §WI-012-5 |
| the services | **T-012-27** | PostgreSQL with `ssl=on` and a `hostssl` rule, MySQL with `--require_secure_transport=ON`, in the `tls` job, under the same CA | the job |
| four rows, verify-full / VERIFY_IDENTITY, both adapters; negatives; control; starter boot | **T-012-28** | `a_verify_full_session_completes_on_<row>` ×4; `an_untrusted_issuer_is_refused_on_<engine>_<adapter>`; `a_hostname_mismatch_is_refused_…`; `a_plaintext_only_server_is_refused_under_a_verifying_mode_…`; the unset-CA control; the starter boots under `verify-full` | evidence; both toolchains |
| disposition | **T-012-29** | either the closure measurement, or a new limitation row with owner, target, consequence | ledger |

### 1.4 S8 — correctness (brief §11)

| Work item | Task | Acceptance tests | Evidence |
|---|---|---|---|
| WI-012-1 | **T-012-30** | (a)–(d) of the brief's WI-012-1; the new census row `seedednoauth` on both legs; the reverted-template negative control run once and quoted; the L-16 disposition (U-8) | evidence; census count |
| WI-012-2 | **T-012-31** | (a)–(c); the family assertions per operation and the absence assertion on a starter that did not select the capability | evidence |
| WI-012-3 | **T-012-32** | (a) the cause established on Linux **first** (a looped run beside a concurrent spawner, or `strace -f` on the exec, retained as evidence); (b) the change matching the confirmed cause; (c) N consecutive green runs on `ubuntu-latest`, both legs, N stated in advance (proposed: 20), no re-run; (d) the raw-`ESC` control still fails. **A retry is not pre-authorised**; if (a) refutes the hypothesis, the task returns to the maintainer with the finding | evidence |

### 1.5 S5 — the artifact handoff (brief §10.6)

| Requirement | Task(s) | Acceptance tests | Evidence |
|---|---|---|---|
| ADR-0040 proposed | **T-012-33** | exists, `proposed` | `decisions/` |
| the JSON envelope schema (U-5) | **T-012-34** | every fixture under `tests/json/` validates against the schema; a mutated fixture (a removed required key) fails; the schema is part of the artifact | `tests/json`, evidence |
| FR-012-D13 `docs-artifact.yml` | **T-012-35** | the workflow produces the archive, `SHA256SUMS`, and the attestation at a named SHA; `manifest.json` carries the SHA, tree, version, MSRV, rustc identities (from the identity step), date; the captured help matches the `trycmd` snapshots (a job step diffs them) | the run identifier; the digest |
| FR-012-D14 `import-framework-artifact.yml` | **T-012-36** | the workflow verifies checksum and attestation (a tampered archive control fails), regenerates every generated page and the stamp, and opens a pull request; a hand-edited generated page fails the drift check (FR-012-D17) | `renvor-docs` run identifier |

### 1.6 S1, S3, S4, S2, S6 — the documentation set (brief §10.2–§10.5, §10.7)

| Requirement | Task(s) | Acceptance tests | Evidence |
|---|---|---|---|
| FR-012-D1…D3 (versioning, search, generated stamp) | **T-012-37** | the build carries one version entry labelled from the manifest; the selector renders; the stamp is generated and matches `manifest.json`; search indexes the generated references | `renvor-docs` CI `docs` |
| existing pages refreshed to the SHA | **T-012-38** | each of the seven stale/partial pages diffed against the contracts and code at the SHA; the `verification` page's census count read from the artifact | the page's evidence links |
| FR-012-D9 new pages, first group (installation, `renvor-new`, `renvor-generate`, architecture, configuration, secrets, containers) | **T-012-39** | each page's claims link evidence (FR-012-D22 green); the §18 map in the brief updated with the page identifiers | `renvor-docs` CI |
| FR-012-D9 new pages, second group (rest, errors, validation, openapi, pagination, migrations, the five capabilities, testing-kit, feature-flags, policies) | **T-012-40** | as above | `renvor-docs` CI |
| FR-012-D10…D12 (deployment, hardening) | **T-012-41** | the hardening page's TLS section written from the B2 evidence and linking it; every unproven claim labelled | `renvor-docs` CI |
| FR-012-D5…D8, D18…D20 (quickstart, the authenticated example, the clean-environment job) | **T-012-42** | `docs-examples` green on both toolchains from a container that has only what the installation page names; the commands extracted from the page; the authenticated example's tests green against the four-row services; the run identifiers linked from the pages (U-4 decided before the example is committed) | the run identifiers |
| FR-012-D21…D23 (limitations page, evidence-link check, prerelease-claim check) | **T-012-43** | the limitations page's row count equals the ledgers' at the SHA; the two checks each have a positive and a negative control page in CI | `renvor-docs` CI |
| FR-012-D4 (protection) | **T-012-44** | branch protection observed through the API after the maintainer's authorization; `PLAN.md` §26.1's table updated with the date | `PLAN.md` |

### 1.7 Closure

| Requirement | Task | Acceptance | Evidence |
|---|---|---|---|
| `PLAN.md` §6.2 completion record; the §12 inventory dispositions; L-1/L-2 closure with measurements; `phase-012-{evidence,limitations,review-record,dependency-inventory,mutation-ledger}.md` | **T-012-45** | every row of the brief's §12 carries a disposition; L-1 and L-2 marked closed only with the measurements of brief §9.8 and §5.9 and after the validator's pass; the review record states which reviews were automated | `governance/` |

---

## 2. Dependency-ordered batches

Each batch is one or more pull requests; each pull request runs the full framework gate (`verify` ×2, `platform` ×4, `security`, `docs`, CodeQL) and, from B2 on, the `tls` job. A batch that changes source runs `cargo xtask verify` locally on both toolchains before its pull request opens (`CONTRIBUTING.md`). Template-version bumps are serialised: one bump per merged batch.

| Batch | Tasks | Repository / files | Depends on | Verification | Notes |
|---|---|---|---|---|---|
| **B0** | this plan | `renvor`: `governance/phase-012-*.md` | — | documentation checks; the required CI once on the final head | the specification-approval checkpoint |
| **B1a** — L-2 declaration, record, contracts | T-012-01, T-012-02, T-012-03, T-012-04, T-012-05 | `renvor`: `decisions/0038-*.md`; `contracts/{template-contract,generation-transaction,command-surface,json-output,verification-sequence,support-policy}.md`; `crates/renvor-cli/templates/**` (+ `rust_toolchain.toml.j2`, both `Cargo.toml.j2`, `README.md.j2` ×2, `Dockerfile.j2`); `crates/renvor-cli/src/{templates.rs,config/**,generate/record.rs,commands/check.rs}`; `tests/snapshots/**`, `tests/json/**`; `governance/phase-012-evidence.md` (new) | B0 approved | both gate legs; template version 7 → 8 with `cargo insta review` | the §8 diff check is part of this PR's body |
| **B1b** — L-2 seal, probes, selection, offline | T-012-06, T-012-07, T-012-08, T-012-09, T-012-10, T-012-11, T-012-12 | `renvor`: `crates/renvor-cli/src/generate/verify.rs`, `commands/{generate,new,dev,routes}.rs`; `tests/{generated,offline,generate,legacy_compatibility}.rs`; `.github/workflows/ci.yml` (a second installed toolchain for the controls — U-10) | B1a | both gate legs; the controls under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` | the largest source batch; mutation-tested per the repository's ledger practice |
| **B1c** — L-2 doctor, xtask, census, dated notes, ledger | T-012-13, T-012-14, T-012-15, T-012-16, T-012-17 | `renvor`: `commands/doctor.rs`; `xtask/src/main.rs`; `tests/starter_matrix.rs`; `SUPPORT.md`, `rust-toolchain.toml`; `governance/phase-012-limitations.md` (new) | B1b | both gate legs; the census count moves | after B1c, L-2's closure conditions (§3 below) can be measured |
| **B2** — L-1 TLS leg | T-012-18 … T-012-25 | `renvor`: `decisions/0039-*.md`; `.github/scripts/tls-materials.sh`; `.github/workflows/ci.yml` (the `tls` job); `crates/renvor-observability/{Cargo.toml,tests/otlp.rs}`; `crates/renvor-cache/tests/valkey.rs`; `crates/renvor-mail/tests/smtp.rs`; `crates/renvor-cli/tests/starter_matrix.rs` (the `tls` row); `contracts/capabilities-contract.md`; templates/README re-read | B1b (the `tls` row uses the new seal); T-012-19…T-012-23 can start in parallel with B1 on a branch and rebase-free merge after it | both gate legs + `tls` ×2 | the deferred-verification licence check lands with the dev-dependency |
| **B3** — WI-012-5 database TLS | T-012-26 … T-012-29 | `renvor`: `crates/renvor-database/src/**` (measurement first), `crates/renvor-sqlx/tests/**`, `crates/renvor-seaorm/tests/**`, the `tls` job | B2 (CA script and job) | both gate legs + `tls` ×2 | U-9 decided in T-012-26 before any type changes |
| **B4** — correctness | T-012-30, T-012-31, T-012-32 (one PR each) | `renvor`: `crates/renvor-cli/templates/starter/{src_seed,src_capabilities_*}.rs.j2`; `tests/starter_matrix.rs`; `tests/hostile.rs` | B1a for the two template items (template-version serialisation); T-012-32 independent | both gate legs | T-012-32's soak is N runs of the `verify` legs on one head, recorded, not re-runs of a failure |
| **B5** — the artifact handoff | T-012-33, T-012-34, T-012-35 (`renvor`); T-012-36 (`renvor-docs`) | `renvor`: `decisions/0040-*.md`; `.github/workflows/docs-artifact.yml`; `crates/renvor-cli/tests/json/schema.json` + test; `renvor-docs`: `.github/workflows/import-framework-artifact.yml`, `scripts/generate-*.mjs` | B1c and B4 (so the captured help and the record shape are final) | framework gate; `renvor-docs` CI; one attested artifact produced and imported end to end at a named SHA | U-5 and U-7 decided before T-012-34/T-012-35 |
| **B6** — the documentation set | T-012-37, T-012-38, T-012-39, T-012-40 (three to four `renvor-docs` PRs) | `renvor-docs`: `docusaurus.config.js`, `sidebars.js`, `docs/**` | B5 (the stamp and the generated pages); prose drafting may start after B1a | `renvor-docs` CI (`docs`, `container`), FR-012-D22/D23 checks | every page stamped to the same artifact SHA |
| **B7** — deployment/hardening, examples, clean environment, checks | T-012-41 (`renvor-docs`); T-012-42 (`renvor` + `renvor-docs`); T-012-43 (`renvor-docs`) | `renvor`: `examples/authenticated-api/**`, `.github/workflows/docs-examples.yml`; `renvor-docs`: `docs/{deployment,hardening,quickstart}.mdx`, `scripts/check-*.mjs` | B2 (the hardening page's TLS section), B5, B6 | framework gate + `docs-examples` ×2; `renvor-docs` CI | U-4 decided before the example is committed |
| **B8** — protection | T-012-44 | `renvor-docs` settings; `renvor`: `PLAN.md` §26.1 | authorization A-1 | observed through the API | a settings change, not a code change |
| **B9** — closure | T-012-45 | `renvor`: `governance/phase-012-*.md`, the ledgers | everything above; the validator's final pass | the required CI on the closure head | the maintainer marks the phase complete; deployment stays a separate authorization |

Parallelism that is safe: B2's T-012-19…T-012-23 beside B1; B4's T-012-32 beside anything; B6's prose beside B5's mechanism. Parallelism that is not: two template-version bumps in flight; two changes to the census count; two ledgers edited at once.

---

## 3. Verification and capacity needs (stated realistically)

| Need | Framework batches (B1–B5, B7) | Documentation batches (B5–B7) |
|---|---|---|
| Local gate per pull request | `cargo xtask verify` on **both** toolchains (`+1.94.0`, `+stable`): about 40–45 minutes per leg on this machine with warm caches; the four-row services (PostgreSQL 17.11, MySQL 8.4.11, Valkey 9.1.1, Mailpit 1.29.1) running; `RENVOR_TEST_REQUIRE_*` set. Two legs are never run concurrently (the four-row suites are not process-safe) | `npm ci && npm test && npm run build` plus lychee: about 3 minutes; Node 22 from `.nvmrc` |
| Local disk | 19 GiB free on 2026-09-07 at 96 % use. The two per-toolchain target directories (`target/tc-1.94.0`, `target/tc-stable`, 1.2 GB each) plus a census build (`target/starter-matrix`, several GB per leg) fit; **no cache is deleted without approval**; if a batch needs more, the batch says so before it starts | negligible |
| Local toolchains | 1.94.0 and stable (1.97.1 on this machine; 1.98.1 in CI — the two differ, and only CI's stable proves a 1.98 lint) plus a third installed toolchain for the selection controls (1.95.0 is present); rustup 1.29.0 | Node 22, npm |
| CI per pull request (`renvor`) | `verify` ×2 (up to 75 minutes each with the census), `platform` ×4 (about 25 minutes), `security`, `docs`, CodeQL (about 10 minutes), and from B2 the `tls` job ×2 (estimated 20 minutes each: images, certificates, the capture); wall time per pull request about 75–80 minutes | — |
| CI per pull request (`renvor-docs`) | — | `docs` and `container`, about 10–15 minutes; from B5 the import workflow on dispatch |
| Services the `tls` job needs on `ubuntu-latest` | Docker (present), `openssl` (present, version recorded), `tcpdump` (present) and `tshark` (installed by `apt` in the job, pinned by version in the workflow) | — |
| Runner permissions | `docs-artifact.yml` needs `id-token: write` and `attestations: write` on its attest job only (the `release-dry-run.yml` pattern); every other job stays `contents: read` | `import-framework-artifact.yml` needs `contents: write` and `pull-requests: write` to open its pull request |
| Human time | the maintainer reviews and merges roughly 18–22 pull requests across two repositories; each implementation batch also needs the validator's pass before its evidence is recorded | — |
| Calendar (a realistic order of magnitude, not a promise) | B1: three pull requests; B2: one to two; B3: one to two; B4: three; B5: two; B6: three to four; B7: two to three; B8: one; B9: one | — |

---

## 4. Unresolved implementation choices — the one consolidated list

Each is a genuine choice that the specification does not settle; each carries a recommendation. None blocks the planning checkpoint; each must be decided before the task that names it starts.

| # | Choice | Options | Recommendation | Decide before |
|---|---|---|---|---|
| **U-1** | Exit code for `record_unsupported` | **3** (C-1's row 3: "validation failure … invalid manifest") or **5** (the 2026-09-06 draft's "environment failure") | **3** — a record from the future is an invalid manifest for this reader, not a missing tool | T-012-05 |
| **U-2** | How the evidence probe measures Cargo's effective compiler | **a probe crate with a build script** (brief FR-012-7d) or **parsing `cargo build -vv`'s `Running rustc …` lines** | the probe crate — it reads the `RUSTC`/wrapper variables Cargo actually exports and needs no output parsing beyond `-vV` | T-012-08 |
| **U-3** | The TLS-version observation point for SMTP and Valkey | **loopback capture decoded with `tshark`** (brief §9.6) or **server-side logs** | the capture — stack-independent and reproducible; neither server documents a reliable per-connection version log | T-012-23 |
| **U-4** | Source of truth for the authenticated example | **a committed tree with relative path dependencies plus a CI diff against a fresh regeneration** (ignoring the path lines) or **regeneration in CI only, nothing committed** | the committed tree — the example has hand-written domain code the generator cannot produce, and readers need to read it; the CI diff keeps it honest | T-012-42 |
| **U-5** | Source of the JSON envelope schema | **a hand-written JSON Schema tested against every fixture under `tests/json/`** or **`schemars` derives** (a new dependency: package research per constitution III) | the hand-written schema — no new dependency, and the fixtures already pin every shape | T-012-34 |
| **U-6** | The version label before any release | `pre-release (snapshot <sha7>)` or `0.0.0-pre` | the former — a number implies a release train the project does not have yet | T-012-37 |
| **U-7** | Whether a commit-addressed, attested artifact satisfies "an immutable framework artifact" before a signed tag exists | **yes, for the prerelease, stated on every page** or **wait for Phase 013's tag** (which would leave S5 unmeasured this phase) | yes, as an explicit, dated assumption in ADR-0040; the workflows take a tag input the day one exists | T-012-33 |
| **U-8** | L-16's remedy in this phase | **format every staged starter Rust file with `rustfmt` before verification** (changes what `renvor new` writes; C-4 says the starter templates stay hand-formatted) or **fix the sites WI-012-1 finds and add the long-name row** | the second, this phase — the first is a C-4 rule change that deserves its own row and its own diff, not a rider on a defect fix | T-012-30 |
| **U-9** | How database TLS mode and CA reach the driver | **URL parameters through `ConnectionString` as today** or **typed settings designed together with 010/L-15's proposed ADR** (endpoint plus `Secret` plus TLS options) | measure first (T-012-26); if the URL parameters pass through today, use them for the proof and open the typed-settings ADR with 010/L-15; do not change `ConnectionString` inside the TLS batch | T-012-26 |
| **U-10** | Where the two-toolchain selection controls run | **install a second toolchain on the `verify` legs** (one more installing step, `RUSTUP_AUTO_INSTALL` still `0`, the identity step unchanged) or **run them only in the `tls` job** | the first — the controls belong with the gate that runs the census | T-012-10 |

---

## 5. Authorizations this plan needs from the maintainer (not choices)

| # | Authorization | Needed by |
|---|---|---|
| **A-0** | Approval of the specification (the brief) and this plan — the checkpoint this pull request stops at | B1a |
| **A-1** | The `renvor-docs` repository-settings change: branch protection on `main`, `docs` and `container` required (`PLAN.md` §26.1, §26.10) | B8 |
| **A-2** | Acceptance of ADR-0038, ADR-0039, ADR-0040 (each proposed in its batch; accepted only by the maintainer, under the single-maintainer waiver pattern) | B1c, B2, B5 |
| **A-3** | Marking L-1, 010/L-1, and L-2 closed with their measurements; every §12 disposition of the brief, in particular 009/L-1 | B9 |
| **A-4** | Whether the `tls` job becomes a required check (a settings change) | after B2 |
| **A-5** | Deployment of any documentation change to `docs.renvor.dev` — **outside this phase's tasks**; a separate authorization under `renvor-infra`'s process | never implied |
| **A-6** | Any deviation from a decision in the brief's §4 discovered during implementation — reported, not absorbed | as it arises |

---

## 6. Closure conditions, stated exactly

**L-2 closes** when: FR-012-1 … FR-012-14 and SR-012-1 … SR-012-5 have their tests green on both gate legs at a named head; AC-012-4 … AC-012-7 are met; the C-sel controls ran under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` on both legs; C-4, C-5, C-1, and `verification-sequence.md` are revised with the version numbers confirmed against the brief's §8 in the same pull request; ADR-0038 is accepted; `phase-012-evidence.md` records the head, the runs, and the compiler identities; and `phase-011-limitations.md` L-2 is marked closed with that measurement, the row itself unedited.

**L-1 closes** when: every item of the brief's §9.8 is recorded against a named head and run for both toolchains of the `tls` job; the three trust cases per positive case are recorded; the negotiated TLS version per connection is recorded from the observation point named in §9.6; the generated starter row passed both boots and its negative; C-C7 1.1.1 names the run; ADR-0039 is accepted; the exclusions of §9.9 are stated in the closing measurement; and L-1 and 010/L-1 are marked closed with that measurement, the rows unedited. Database TLS is WI-012-5 and closes or defers separately (T-012-29).

**The phase closes** when every feature in it meets `PLAN.md` §23, every row of the brief's §12 carries a disposition, the review record states which reviews were automated, and the maintainer approves — `CONSTITUTION.md` §Development and Phase Workflow 9.
