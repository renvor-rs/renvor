# Phase 012 — Security carry-over plan: Phase 011 limitations L-1 and L-2

**Companion to**: [`phase-011-limitations.md`](phase-011-limitations.md) · [`phase-011-evidence.md`](phase-011-evidence.md) · [`phase-010-limitations.md`](phase-010-limitations.md) · `PLAN.md` §"Phase 012 — REST documentation and production examples"
**Drafted**: 2026-09-06, against `main` at `ab701d2a6731f271d53fd0a380554c32e9d8a740` (tree `1722c687bb6568c6cf2e997a1a423db6757b5dcc`, the Phase 011 squash merge), by the maintainer's session
**Status**: **PLANNING RECORD.** Both limitations stay **open** exactly as `phase-011-limitations.md` states them. This file closes nothing, grants no waiver, accepts no decision record, reinterprets no requirement, changes no contract, and implements nothing. Every proposal below is labelled *Proposal*; every choice that is the maintainer's is labelled *Decision needed* and numbered so that Phase 012's specification can cite it.
**Working copy**: `specs/012-rest-documentation-and-production-examples/security-carryover.md`, under the gitignored `specs/` tree, is the same text; this tracked file is the clone-visible mirror and the authority if the two differ.

## 0. Why this file exists

Phase 011's limitations ledger carries two security-relevant rows whose target is Phase 012, and
`PLAN.md`'s Phase 012 acceptance reads "commands run from clean environments; examples are
exercised in continuous integration; claims link to evidence; all current limitations are
visible". Both rows bear directly on that: a production example that speaks TLS to its backends
cannot be exercised in CI until L-1 is closed, and "a clean environment" has no defined toolchain
for a generated project until L-2 is decided. Writing the plan now, from the merged tree, fixes the
starting statement of each row before Phase 012's specification is drafted, so the phase starts
from a record rather than from memory.

Rows are quoted verbatim from `main` at `ab701d2`. Code references were checked at that head.
Measurements dated 2026-09-06 were taken on the maintainer's machine during the cheat-sheet
execution of the same day (`rustc` 1.94.0 and 1.97.1, PostgreSQL 17.11, Valkey 9.1.1, Mailpit
1.29.1 containers, all plaintext on loopback).

---

## 1. L-1 — a generated starter speaks plaintext to Valkey, SMTP, and the OTLP receiver

### 1.1 The row, verbatim

From `governance/phase-011-limitations.md`, table "Security-relevant":

> | **L-1** | **A generated starter speaks plaintext to Valkey, SMTP, and the OTLP receiver.** The development configuration the generator writes opts into plaintext explicitly (SR-005: a written double opt-in, never a URL scheme), the four full rows prove every capability against real plaintext loopback services, and no generated project has completed a TLS handshake with any of them. Inherits 010/L-1. | No trusted certificate authority exists on this machine or on a CI runner; a self-signed CA leg is CI work the generator cannot carry. | Maintainer; **Phase 012 CI leg with a self-signed CA**. Consequence: the TLS settings the templates render are compiled but never exercised end to end. |

Its ancestor, from `governance/phase-010-limitations.md`:

> | **L-1** | **No real TLS handshake is exercised** against Valkey, an SMTP relay, or an OTLP collector. The adapters configure rustls with native roots and the `ring` provider (source-verified, and the single-provider state is asserted by step 7), and the plaintext-loopback paths run against real servers. | No trusted certificate authority exists on this machine or a CI runner. | Maintainer; a CI leg with a self-signed CA in Phase 011 *(Phase 011 disposition: retained; owner, target, consequence stated — `phase-011-limitations.md`.)* |

And Phase 011's disposition of that ancestor (`phase-011-limitations.md`, "Retained from Phase 010"):

> | **010/L-1** — no real TLS handshake exercised | **Retained.** No CA is available to a test; the generated starter inherits it as this phase's L-1. | Maintainer; **Phase 012 CI leg with a self-signed CA**. Consequence as L-1 above. |

| Field | Value |
|---|---|
| ID | **L-1** (Phase 011), inheriting **010/L-1** (Phase 010); both rows stay in their ledgers |
| Owner | Maintainer |
| Target | Phase 012 CI leg with a self-signed CA |
| Consequence | The TLS settings the templates render are compiled but never exercised end to end |
| Status on 2026-09-06 | **Open.** Not narrowed, not waived, not reinterpreted here |

### 1.2 The security requirement that remains unmet

- **Constitution, Security**: "Production services MUST use authenticated TLS to external dependencies where supported." The adapters *default* to TLS; whether an authenticated TLS session to any of the three services actually succeeds, and whether an unauthenticated one is refused, has never been observed.
- **`PLAN.md` §security baseline**: "Transport security — TLS 1.3 preferred; TLS 1.2 minimum where ecosystem support requires it", and "Avoid silent fallback when configuration, credentials, durable storage, TLS trust, or required dependencies fail." No measurement records which protocol version rustls negotiates through any adapter, because no negotiation has happened.
- **Contract C-C7** (`contracts/capabilities-contract.md` 1.1.0): "TLS by default; plaintext is a double opt-in; a credential is never part of a URL" — both adapters default to rustls with the native root store (SMTP: implicit TLS or required STARTTLS); the OTLP exporter accepts `http://` only to loopback. The *refusal* half of C-C7 is proven (§1.4); the *acceptance* half — a TLS endpoint that works, against a trusted certificate, and fails closed against an untrusted one — is the unmet part.
- **SR-005** (Phase 011 specification, mirrored in `phase-011-evidence.md`): "Plaintext to Valkey and SMTP stays a double opt-in written explicitly in the generated development configuration, never in a URL; the OTLP endpoint accepts `http://` to loopback only." Met for the development configuration; the production configuration the same templates describe in comments (`tls = true`, `security = "starttls"`, an `https://` collector) is the untested path.

What is **not** in the row's wording, and stays outside it unless the maintainer widens it (D-L1-2): TLS to PostgreSQL and MySQL. The persistence adapters enable SQLx's `tls-rustls-ring-native-roots` feature and the four rows connect to the containers without TLS; that gap is adjacent to L-1 and is not this row.

### 1.3 Affected code and entry points (at `ab701d2`)

| Adapter | Entry points | TLS stack |
|---|---|---|
| `renvor-cache` (`valkey` feature) | `ValkeyEndpoint::tls(host, port)` (the default) and `ValkeyEndpoint::plaintext(host, port)` in `crates/renvor-cache/src/valkey.rs`; `ValkeySettings::with_allow_insecure_loopback`; `ValkeyCache::connect`; `ValkeyProvider::from_config`; the `[cache]` keys `tls`, `allow_insecure_loopback` (`crates/renvor-cache/src/config.rs`) | `redis` 1.6.0 with `tokio-rustls-comp` + `connection-manager`; `rustls` 0.23.43 with `ring`, `std`, `tls12`; the native store through `rustls-native-certs` 0.8.4 (ADR-0033, decision table). The module doc of `valkey.rs` ("The process-level crypto provider") records that `redis` builds its `ClientConfig` from the process-level provider — a second provider in a consumer's graph panics inside the client, asserted structurally by `xtask` step 7 and never by a handshake |
| `renvor-mail` (`smtp` feature) | `Security::{ImplicitTls, StartTls, PlaintextLoopback}` and `SmtpEndpoint::new(host, security)` in `crates/renvor-mail/src/smtp.rs` — `Tls::Wrapper(TlsParameters::new(host))` for implicit TLS, `Tls::Required(TlsParameters::new(host))` for STARTTLS, `Tls::None` for loopback plaintext, all on `AsyncSmtpTransport::builder_dangerous(host)`; `SmtpMailer::connect`; `verify()` (`EHLO`/`NOOP`, run at Boot by `MailProvider`); the `[mail]` keys `security`, `allow_insecure_loopback` (`crates/renvor-mail/src/config.rs`) | `lettre` 0.11.23 with `tokio1-rustls`, `ring`, `rustls-native-certs`, `smtp-transport`, `pool`, `builder` (ADR-0034) |
| `renvor-observability` (`otel` feature) | endpoint validation in `crates/renvor-observability/src/otel.rs` (`https://` anywhere, `http://` only to a loopback host; `is_loopback`); the exporter's connector built with `with_provider_and_native_roots(rustls::crypto::ring::default_provider())`; `OtlpSection` (`crates/renvor-observability/src/config.rs`, key `endpoint`) | `hyper-rustls` 0.27.9 with `native-tokio`, `http1`, `tls12`, `ring`; `rustls` 0.23.43 (ADR-0036) |
| The generated starter (`crates/renvor-cli/templates/starter/`) | `config_cache.toml.j2` renders `tls = false` + `allow_insecure_loopback = true`; `config_mail.toml.j2` renders `security = "plaintext"` + `allow_insecure_loopback = true`; `config_otlp.toml.example.j2` renders `endpoint = "http://127.0.0.1:4318/v1/traces"`; `README.md.j2` says "TLS to the backends is off for loopback and on by default anywhere else"; `src_capabilities_{cache,mail,observability}.rs.j2` build the providers `from_config` | Whatever the three crates above do; the starter adds no TLS code of its own |
| Root-store discovery | `rustls-native-certs` (mail, and `redis`'s `tls-rustls` path per ADR-0033) and `hyper-rustls`'s `native-tokio` read the platform store and honour `SSL_CERT_FILE` / `SSL_CERT_DIR` as the crate documents; the generator's sealed verification environment passes exactly those two variables through (`crates/renvor-cli/src/generate/verify.rs`, `PASSED_THROUGH`) | To be re-verified by the leg, not assumed (§1.5, AC-L1-4) |

### 1.4 Existing evidence, and what it does not prove

**Evidence that exists** (all at `ab701d2`, all green in the Phase 011 gates):

| Proof | Where | What it establishes |
|---|---|---|
| `a_plaintext_endpoint_to_a_non_loopback_host_is_refused_even_with_the_opt_in`, `a_plaintext_endpoint_to_loopback_needs_the_opt_in`, `a_tls_endpoint_needs_no_opt_in_anywhere` | `crates/renvor-cache/src/valkey.rs` (unit) | the settings boundary refuses plaintext off loopback and accepts TLS anywhere, before a socket exists |
| `plaintext_off_loopback_is_refused_at_validate_naming_tls_and_its_layer` | `crates/renvor-cache/src/config.rs` | the same refusal from a configuration section, at Validate, naming the key |
| `a_plaintext_endpoint_to_a_non_loopback_host_fails_boot_before_any_socket` | `crates/renvor-cache/tests/valkey.rs` | the provider fails Boot on the refusal |
| `plaintext_needs_loopback_and_the_flag_together` | `crates/renvor-mail/src/smtp.rs` | the SMTP double opt-in, three hosts, with and without the flag |
| `plaintext_off_loopback_is_refused_at_validate_naming_security` | `crates/renvor-mail/src/config.rs` | the same from `[mail]` |
| `endpoints_are_https_or_loopback_http` | `crates/renvor-observability/src/otel.rs` | the OTLP endpoint rule |
| `a_plaintext_endpoint_off_loopback_is_refused_before_anything_is_built` | `crates/renvor-observability/tests/otlp.rs` | the rule at the exporter boundary |
| `xtask` step 7 | `xtask/src/main.rs` | exactly one `rustls` crypto provider (`ring`) in the graph; none of `webpki-roots`, `native-tls`, `openssl`, `rustls-platform-verifier` |
| the four full census rows `pgsqlx`, `mysqlx`, `pgsea`, `mysea` and the lean rows `cacheonly`, `mailonly`, `observeonly` | `crates/renvor-cli/tests/starter_matrix.rs`, run locally and in CI's `verify` jobs | every generated capability works against a real service **over plaintext loopback** |
| the cheat-sheet execution of 2026-09-06 (out of repository) | `renvor-blog-api-cheatsheet.md`, verification record for sections 19–22 | the same, by hand, on `ab701d2` — plaintext loopback only |

**What none of it proves:**

- that any adapter completes a TLS handshake at all — with Valkey (`ValkeyEndpoint::tls`), with an SMTP relay over implicit TLS *or* over STARTTLS (`Tls::Required` must refuse a server that does not offer STARTTLS; that refusal is likewise unobserved), or with an HTTPS OTLP receiver;
- that the native root store is loaded at runtime on any platform, or that a CA supplied through `SSL_CERT_FILE` is honoured;
- that certificate validation fails closed: an untrusted issuer, a hostname mismatch, an expired leaf, and a plaintext server answering a TLS endpoint must each produce the closed error category the contracts name (`CacheError::Unavailable`/`CacheBootError`, `MailError::Unavailable`/`Refused`, the exporter's counted failure) and never a fallback to plaintext;
- which protocol version is negotiated, so the `PLAN.md` "TLS 1.3 preferred; 1.2 minimum" baseline is unmeasured; `tls12` is compiled in on all three stacks;
- that `[cache] tls = true`, `[mail] security = "starttls"` / `"implicit_tls"`, and an `https://` `[otlp] endpoint` — the three lines the templates tell an operator to switch to — boot a generated starter;
- that Valkey's `ConnectionManager` reconnects over TLS within `ReconnectBounds`;
- SNI, session resumption, and client certificates — none is claimed by any contract, and none is measured;
- the redis provider-install caveat (§1.3) beyond the structural single-provider assertion.

### 1.5 Acceptance criteria for closure — *Proposal*

The row closes when **all** of the following are recorded in Phase 012's evidence against a named head, with the maintainer's disposition of each decision in §1.8:

| # | Criterion |
|---|---|
| **AC-L1-1** | A CI job in `.github/workflows/ci.yml` (proposed name: `tls (self-signed CA)`, `ubuntu-latest`) generates a throw-away CA and per-service leaf certificates at run time — never committed, never reused — and starts **Valkey with a TLS port**, **Mailpit with a certificate** (implicit TLS on its TLS port and STARTTLS on the submission port), and **an HTTPS OTLP receiver** (D-L1-3), trusting the CA through the mechanism D-L1-1 selects. |
| **AC-L1-2** | Four positive handshakes are observed and asserted, not inferred: a Valkey `set`/`get` over `ValkeyEndpoint::tls`; one SMTP `send` over `Security::ImplicitTls` and one over `Security::StartTls`, each followed by a `verify()`; one OTLP export over `https://` that the receiver records. Each test names the negotiated protocol version in its evidence line (rustls exposes it), so the `PLAN.md` baseline becomes a measurement. |
| **AC-L1-3** | Negative controls, each a separate test that fails closed with the contract's category and never falls back: (a) a leaf signed by an issuer the run does not trust; (b) a leaf whose name does not match the host; (c) an expired leaf; (d) a plaintext listener answering the TLS endpoint; (e) for SMTP, a relay that advertises no STARTTLS under `Security::StartTls`. A negative control that passes because the connection never happened is a false pass — each must first assert that the positive case on the same endpoint succeeded in the same run. |
| **AC-L1-4** | The root-store path is proven, not assumed: the same tests fail when the CA is removed from the trust source (D-L1-1), which is the control that shows the trust really came from that source. |
| **AC-L1-5** | At least one **generated starter** row is generated with the production-shaped configuration (`[cache] tls = true`, `[mail] security = "starttls"` or `"implicit_tls"`, `[otlp] endpoint = "https://…"`), boots against the TLS services with **no** `allow_insecure_loopback`, and passes its own `tests/starter.rs` — so L-1's consequence ("compiled but never exercised end to end") is measured on a generated project, not only on the crates. |
| **AC-L1-6** | The three lines in the templates and the generated README that describe the production path are re-read against what the leg actually did, and corrected if they differ; `capabilities-contract.md` C-C7 gains a sentence naming the leg as its proof. |
| **AC-L1-7** | `phase-011-limitations.md` L-1 and `phase-010-limitations.md` 010/L-1 are marked closed **with the measurement** (head, job name, run identifier, the protocol versions observed), in the same form the ledger uses for every closed row; the rows themselves are not edited. |
| **AC-L1-8** | The leg's dependencies pass the repository's dependency gates unchanged (`deny.toml`, the advisory policy, MSRV, licence): a test-only certificate generator, if any, is researched like any package (D-L1-5). |

### 1.6 Regression tests and negative controls — *Proposal*

| Test (proposed name) | Crate / file | Kind | What a mutation must not survive |
|---|---|---|---|
| `a_tls_session_to_valkey_completes_against_a_trusted_ca` | `renvor-cache/tests/valkey.rs` (behind an env-gated `RENVOR_TEST_VALKEY_TLS_URL`… pattern of the existing gated tests) | positive | removing `with_provider_and_native_roots`/the TLS connector |
| `an_untrusted_issuer_fails_boot_with_unavailable_and_no_plaintext_retry` | same | negative | any retry or downgrade path |
| `a_hostname_mismatch_is_refused` | same | negative | disabling name verification |
| `a_plaintext_listener_on_the_tls_port_is_refused` | same | negative | a lenient handshake |
| `an_implicit_tls_submission_completes` / `a_starttls_submission_completes` | `renvor-mail/tests/smtp.rs` | positive | swapping `Tls::Wrapper`/`Tls::Required` for `Tls::None` or `Tls::Opportunistic` |
| `a_relay_without_starttls_is_refused_under_required_starttls` | same | negative | `Tls::Opportunistic` |
| `an_https_export_reaches_the_receiver` | `renvor-observability/tests/otlp.rs` | positive | an `http` connector |
| `an_https_export_to_an_untrusted_receiver_is_counted_as_failed_and_not_sent` | same | negative | ignoring the verifier result |
| the trust-source control: every positive above re-run with the CA withheld, expected to fail | the CI job | control | a test that passes without the CA is not proving the CA |
| `the_tls_starter_row_boots_and_its_generated_test_passes` | `renvor-cli/tests/starter_matrix.rs` (a new row or a flag on an existing one) | end to end | a template rendering `allow_insecure_loopback` where it must not |

Each negative control needs a **positive control in the same run** (the same endpoint, trusted) — the existing suites' pattern.

### 1.7 Applicable database, adapter, and platform coverage

| Axis | Coverage the closure must state |
|---|---|
| Adapters | cache (Valkey), mail (SMTP implicit TLS and STARTTLS), observability (OTLP over HTTPS). Storage is a filesystem root and has no transport. Jobs live in the application's database row and are covered by D-L1-2's answer, not by L-1 |
| Databases | not L-1's subject (D-L1-2). If the maintainer widens the row, both engines, all four rows, because SQLx's TLS feature is the same on both |
| Platforms | the leg runs on `ubuntu-latest` — the only CI leg with a container daemon (`SUPPORT.md`). Root-store discovery on macOS and Windows stays unproven unless the leg also runs a file-CA case on the platform jobs without services (D-L1-6); the platform jobs currently run the `nodb` row only |
| Toolchains | both gate legs (`1.94.0`, `stable`), as for every CI job |

### 1.8 Dependencies and decisions the maintainer must take — *Decision needed*

| # | Decision | Proposal (not a decision) |
|---|---|---|
| **D-L1-1** | How the leg trusts its CA: through `SSL_CERT_FILE`/`SSL_CERT_DIR` (honoured by `rustls-native-certs`, no API change, passed through the sealed environment already) **or** through a new per-adapter setting (`[cache] ca_file`, `[mail] ca_file`, `[otlp] ca_file`), which is a public configuration change under the configuration contract and C-C11 | start with the environment variables; record that an operator cannot pin a CA per adapter as a new limitation if that stays true |
| **D-L1-2** | Whether TLS to PostgreSQL/MySQL joins the leg, as a widening of L-1 or as a new row | a new row, so L-1 closes on its own wording |
| **D-L1-3** | The HTTPS OTLP receiver: extend the crate's own loopback receiver with `tokio-rustls` (a test-only dependency through the package gate) **or** run a collector image in CI | the crate's receiver, so the proof is not a third party's configuration |
| **D-L1-4** | Client certificates (mTLS) | out of scope; no contract names them |
| **D-L1-5** | Certificate generation in CI: the runner's `openssl` CLI (no crate) **or** a Rust generator (`rcgen`, subject to licence/advisory/MSRV research per constitution §research) | `openssl` CLI first; it adds nothing to the graph |
| **D-L1-6** | Whether macOS/Windows root-store discovery is in scope for closure | out of scope for closure; recorded as a consequence |
| **D-L1-7** | Whether a negotiated TLS 1.2 (allowed by `tls12` on all three stacks) is acceptable for the baseline, or whether 1.3 must be asserted | record what is observed; do not restrict without a decision record |

Dependencies: Valkey 9.1.1's image must be built with TLS support (verify at leg time; `valkey-server --tls-port` refuses otherwise); Mailpit 1.29.1 supports a certificate for SMTP; the CI runner needs no daemon change.

---

## 2. L-2 — verification runs in a sealed environment that still inherits the caller's toolchain

### 2.1 The row, verbatim

From `governance/phase-011-limitations.md`, table "Security-relevant":

> | **L-2** | **Verification runs in a sealed environment that still inherits the caller's toolchain.** `renvor new` clears the environment before `fmt`, `clippy`, `build`, `test`, and the route-dump smoke and passes through an allow-list only (`PATH`, `HOME`, `USER`, `LOGNAME`, `TMPDIR`, `TEMP`, `TMP`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TERM`, and the `CARGO_HOME`/`RUSTUP_*` family), so no `RENVOR_*` test variable reaches the staged project — but the `cargo` that verifies is whichever one `PATH` resolves, and the starter ships no `rust-toolchain.toml`. | Pinning a toolchain in a generated project is a template-contract decision (C-4) the phase did not take; the framework's own MSRV is enforced by the gate on both toolchains, not by the generated tree. | `renvor-cli`; **Phase 012 (C-4 1.2.0)**. Consequence: a starter verified on one toolchain may fail its own `clippy -D warnings` on a newer one until the author pins it. |

No Phase 010 ancestor: the sealed environment is Phase 011's.

| Field | Value |
|---|---|
| ID | **L-2** (Phase 011) |
| Owner | `renvor-cli` |
| Target | Phase 012, as a revision of contract C-4 (`contracts/template-contract.md`) — see the version note under D-L2-0 |
| Consequence | A starter verified on one toolchain may fail its own `clippy -D warnings` on a newer one until the author pins it |
| Status on 2026-09-06 | **Open.** Not narrowed, not waived, not reinterpreted here |

A precision about the row's own summary, recorded so the plan quotes the code rather than the
row: `PASSED_THROUGH` in `crates/renvor-cli/src/generate/verify.rs` also passes `RUSTC`,
`RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS`, `CARGO_BUILD_JOBS`, `CARGO_INCREMENTAL`,
`CARGO_NET_*`, `CARGO_HTTP_*`, `CARGO_REGISTRIES_CRATES_IO_PROTOCOL`, `CARGO_TERM_COLOR`,
`SSL_CERT_FILE`, `SSL_CERT_DIR`, the proxy variables (credential stripped), and the Windows
process variables. The toolchain *identity* therefore enters through three doors, not one: `PATH`
(which `cargo`), `RUSTUP_TOOLCHAIN` (which toolchain that `cargo` proxies to), and
`RUSTC`/`RUSTC_WRAPPER`/`RUSTFLAGS` (what that toolchain is made to do). The row names the first.

### 2.2 The security requirement that remains unmet

- **Constitution, Technical Standards**: "The workspace MUST use stable Rust, the Rust 2024 edition, Cargo resolver 3, and an explicit MSRV tested in continuous integration." The framework meets it (`rust-toolchain.toml` pins `1.94.0`; CI runs `1.94.0` and `stable`). The **generated project** declares no MSRV and pins no toolchain: `crates/renvor-cli/templates/starter/Cargo.toml.j2` and `crates/renvor-cli/templates/Cargo.toml.j2` carry `edition = "2024"` and no `rust-version`; no template renders a `rust-toolchain.toml`.
- **Constitution, Release**: release candidates must pass "clean generated-project tests" — a clean generated project's toolchain is undefined today, so "clean" means "whatever the runner had".
- **`PLAN.md` principle 7** ("Generated code is owned code. Scaffolds are readable, formatted, testable…") and **Phase 012's acceptance** ("commands run from clean environments"): the proof that a starter is formatted and lint-clean is made against one compiler and read as if it held for all.
- **Contract C-5** (`contracts/generation-transaction.md` 1.1.0, "The checks run in a sealed environment"): the seal is specified to exclude *secrets* — `RENVOR_*`, credentials, proxy passwords — and to admit "what the toolchain needs". It is silent on which toolchain — read here as the contract's intent rather than an omission (a reading, not a finding) — and L-2 is the recorded consequence of that silence.
- **Contract C-4** (`contracts/template-contract.md`): governs what a template renders and how a rendered tree is proven; it does not mention a toolchain file. The row names C-4 as the place the decision belongs.
- **The security reading**: the verification that certifies a starter ("`fmt`, `clippy -D warnings`, `build`, `test`, and the route dump all passed, so this tree is placed") is only as strong as the compiler that ran it; a wrapper or flags inherited from the operator's shell run inside the "sealed" step; and the generated Dockerfile builds the same project on a *different*, pinned compiler (`FROM docker.io/library/rust:1.94.0-slim`, `crates/renvor-cli/templates/Dockerfile.j2`) whose comment says it matches "this project's `rust-toolchain.toml`" — a file the project does not have. Two compilers for one project, and a comment that names a file that is not there.

### 2.3 Affected code and entry points (at `ab701d2`)

| Where | What |
|---|---|
| `crates/renvor-cli/src/generate/verify.rs` | `PASSED_THROUGH` (the allow-list), `PROXY_VARIABLES`, `Sealed`, `seal`, `in_staging`; the five checks — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`, `cargo test`, and the smoke (`cargo run --quiet` for a skeleton, the route-dump request for a starter); `Command::new(program)` resolves `cargo` from the sealed `PATH`; the target directory is `CARGO_TARGET_DIR` when absolute, else a temporary directory |
| `crates/renvor-cli/src/commands/new.rs` | staging, `seed_lockfile` (FR-006), the verification call on the real **and the dry-run** path, the provenance record written after verification |
| `crates/renvor-cli/src/commands/generate.rs` | `renvor generate auth` verifies the merged tree in a scratch copy with the same five checks; `renvor generate resource` runs the toolchain's `rustfmt` at generation (C-4 §"Generated-on-demand files"; `tool_missing` when absent) |
| `crates/renvor-cli/src/generate/record.rs` | `.renvor/generated.toml` records `generator_version`, `template_version`, and a digest per file — **no toolchain** |
| `crates/renvor-cli/src/commands/doctor.rs` | probes `cargo` against a declared minimum (read from the workspace, "a hard-coded `1.94.0` here would be a second declaration"); the only operator-facing toolchain check today, and it is advisory to `renvor new` |
| Templates | `templates/starter/Cargo.toml.j2`, `templates/Cargo.toml.j2` (`edition = "2024"`, no `rust-version`); `templates/Dockerfile.j2` (builder pinned to `rust:1.94.0-slim`; the comment names a `rust-toolchain.toml`); no toolchain-file template exists |
| Contracts | C-4 `template-contract.md` (the decision's home), C-5 `generation-transaction.md` (the seal), C-1 `command-surface.md` (the refusal codes a pinned-but-missing toolchain would need: `tool_missing` and exit `5` exist) |
| Tests | `verification_runs_in_a_sealed_environment`, `a_proxy_credential_never_reaches_the_sealed_environment`, `a_childs_output_is_reported_without_url_credentials_or_control_characters` (`verify.rs`); `crates/renvor-cli/tests/starter_matrix.rs` (every row generated under the gate's toolchain); `crates/renvor-cli/tests/offline.rs` (FR-006) |

### 2.4 Existing evidence, and what it does not prove

**Evidence that exists:**

- `verification_runs_in_a_sealed_environment` proves the allow-list: `RUSTUP_TOOLCHAIN` passes, everything outside the list is dropped.
- Both gate legs — `cargo +1.94.0 xtask verify` and `cargo +stable xtask verify` — run the census, so every starter row was generated and verified under `rustc` 1.94.0 **and** under 1.97.1 (`phase-011-evidence.md` §10 and §13; CI matrix `toolchain: ["1.94.0", "stable"]` on `ubuntu-latest`, and the `nodb` row on macOS and Windows for both).
- Measured 2026-09-06 (a throw-away crate whose build script prints `rustc --version` and whose binary prints `RUSTUP_TOOLCHAIN`; its output is retained beside the cheat-sheet evidence, out of repository, as `logs/D-envprobe.log`): `cargo +1.94.0 run` and `RUSTUP_TOOLCHAIN=1.94.0 cargo run` both reach the child with `RUSTUP_TOOLCHAIN=1.94.0-aarch64-apple-darwin` and `rustc 1.94.0`; plain `cargo run` outside the framework tree reaches it with `stable-aarch64-apple-darwin` / `rustc 1.97.1`. So the gate's stable leg really did verify the rows on stable — through rustup's proxy behaviour, which the seal passes through — and a project generated in a directory outside the framework is verified on the operator's **default** toolchain.
- Measured 2026-09-06 during the cheat-sheet execution: two starters and several throw-away projects were generated, verified, built, linted, and tested on `rustc 1.97.1` (the default of a directory outside the framework) with `clippy -D warnings` green. The only tree rendered under **both** toolchains that day was the seed-defect control of §3 (`RUSTUP_TOOLCHAIN=1.94.0`), refused by both; every other tree ran on 1.97.1 alone. Agreement of every census row on both toolchains is the gate's measurement (`phase-011-evidence.md` §10, §13), not this one's.

**What none of it proves:**

- **which toolchain verified a given placed project** — nothing records it (`.renvor/generated.toml` has no such field, `renvor.toml` neither), so an author cannot tell, and a later `renvor generate` cannot check;
- that a starter verifies on a toolchain **older** than the framework's MSRV (edition 2024 needs a compiler that has it; the framework's own MSRV features need 1.94.0): the expected failure is a compiler error inside `project_verification_failed` blaming "a defect in renvor's templates", not a `tool_missing`/environment refusal naming the toolchain — no test pins that message;
- that a **future** stable's new default lints keep `clippy -D warnings` green on every rendered variant (the consequence the row names; by construction unprovable ahead of time, which is why a pin is the remedy);
- that `RUSTC`, `RUSTC_WRAPPER`, and `RUSTFLAGS` inherited from the operator's shell are benign inside the seal — no control exercises them (a wrapper that is `sccache` is the intended use; a wrapper that is anything else runs with the operator's rights, which is the same trust the operator already has, and is recorded here rather than decided);
- that the Dockerfile's builder (`rust:1.94.0-slim`) and the host verification agree; on this machine they did not (1.97.1 on the host), and nothing noticed;
- Windows and macOS: the platform jobs verify the `nodb` row only, under the matrix toolchain; rustup's handling of a `rust-toolchain.toml` in a nested directory is the same on all three, but a pin has never been rendered, so it has never been tested there.

### 2.5 Acceptance criteria for closure — *Proposal*

Closure needs **one** mechanism decided first (D-L2-1); the criteria below are written for the proposal (a rendered `rust-toolchain.toml`) and hold, with the obvious substitutions, for the alternatives.

| # | Criterion |
|---|---|
| **AC-L2-1** | **C-4 is revised** (version per D-L2-0) to state what a generated tree declares about its toolchain, for both the skeleton and the starter, and what `renvor new` and `renvor generate` do when the declared toolchain is absent or incompatible. `contracts/generation-transaction.md` gains one sentence saying the seal's `PATH`/`RUSTUP_TOOLCHAIN` pass-through no longer decides the compiler alone. |
| **AC-L2-2** | Every generated tree carries the declaration: a `rust-toolchain.toml` rendered from the **framework checkout's** own file (channel `1.94.0`, components `rustfmt` and `clippy`, profile `minimal`) for a starter, and the same channel for a skeleton (D-L2-4 decides whether the skeleton pins at all); and `rust-version` in `Cargo.toml` if D-L2-1 chooses both. The snapshot manifests (`crates/renvor-cli/tests/snapshots/`) move to the next template version with the new path. |
| **AC-L2-3** | Verification uses the declared toolchain: a test generates a starter with `RUSTUP_TOOLCHAIN` unset, in a directory whose ancestors carry no toolchain file, on a machine whose default is *not* the pin (the stable gate leg is exactly that machine), and asserts — by reading the toolchain from inside the staged build (a `rustc --version` the smoke binary reports, or a build-script probe as measured above) — that the pinned toolchain ran. The same for `renvor generate auth`'s scratch verification and `renvor generate resource`'s `rustfmt`. |
| **AC-L2-4** | The record says which toolchain verified the tree: `.renvor/generated.toml` gains a `verified_with` line (the `rustc --version` string), written after verification with the digests; `renvor check` reports it; the snapshot policy pins the key, not the value (as it does for `Cargo.lock`). |
| **AC-L2-5** | A missing or incompatible pinned toolchain is a named refusal before any check runs: `tool_missing`, exit `5`, `details.tool = "rustup toolchain <channel>"` (or the nearest existing shape), with the remedy command — never a compiler error inside `project_verification_failed`, and never a silent fall-through to the default toolchain. A test removes the toolchain from `PATH`'s view (an empty `RUSTUP_HOME` in the sealed environment) and asserts the code and the message. |
| **AC-L2-6** | The stable gate leg **still proves stable**: `RUSTUP_TOOLCHAIN` set by rustup's `+stable` proxy takes precedence over a `rust-toolchain.toml` (rustup's documented override order), so the census rows keep being verified on both toolchains; a test asserts the precedence rather than assuming it, and the evidence records both toolchains per row as today. |
| **AC-L2-7** | The generated Dockerfile's builder pin and the rendered toolchain file name the **same** channel from the same source, and the Dockerfile comment names a file that exists. |
| **AC-L2-8** | The generated README states the pin, why it is there (this row), and how an author changes it; `SUPPORT.md`'s platform table is unchanged unless the platform jobs' behaviour changes. |
| **AC-L2-9** | `phase-011-limitations.md` L-2 is marked closed with the measurement (head, the tests above, the two toolchains observed); the row is not edited. |
| **AC-L2-10** | The decision on `RUSTC`/`RUSTC_WRAPPER`/`RUSTFLAGS` (D-L2-3) is recorded either as an accepted residual in the ledger (a new row, if they stay) or as a change to `PASSED_THROUGH` with its own control. |

### 2.6 Regression tests and negative controls — *Proposal*

| Test (proposed name) | Where | Kind | What a mutation must not survive |
|---|---|---|---|
| `every_generated_tree_declares_the_frameworks_toolchain` | `renvor-cli/tests/snapshots.rs` + `starter_matrix.rs` | positive | dropping the file from a variant; rendering a channel other than the framework's |
| `verification_runs_on_the_declared_toolchain_not_the_default` | `renvor-cli/tests/generated.rs` (needs two installed toolchains; skipped with a `SKIPPED:` line and required under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` in CI, the census pattern) | positive | ignoring the file; resolving `cargo` before the file is written |
| `the_record_names_the_toolchain_that_verified_the_tree` | `renvor-cli/src/generate/record.rs` unit + `generated.rs` | positive | writing the record before verification (the Codex-review regression of Phase 011, in the other direction) |
| `a_missing_pinned_toolchain_is_tool_missing_not_a_compiler_error` | `generated.rs` | negative | letting `cargo` fall through to the default toolchain |
| `an_environment_toolchain_override_wins_and_is_recorded` | `verify.rs` unit + `generated.rs` | control for AC-L2-6 | a pin that silently defeats the stable leg |
| `the_dockerfile_builder_and_the_toolchain_file_agree` | `snapshots.rs` | positive | editing one template without the other |
| `a_renvor_generate_auth_scratch_verification_uses_the_pin` / `resource_rustfmt_uses_the_pin` | `renvor-cli/tests/generate.rs` | positive | a second toolchain resolution path |
| the three existing seal tests | `verify.rs` | unchanged | — |

### 2.7 Applicable database, adapter, and platform coverage

| Axis | Coverage the closure must state |
|---|---|
| Rows | all ten starter rows (`pgsqlx`, `mysqlx`, `pgsea`, `mysea`, `authonly`, `cacheonly`, `storageonly`, `mailonly`, `observeonly`, `nodb`) and the six skeleton variants — the pin is rendered into every tree, and the census is the proof that every tree still verifies |
| Databases | both engines through the rows; the pin is engine-independent |
| Toolchains | both gate legs; AC-L2-3 needs the stable leg as the "default is not the pin" machine, and AC-L2-6 keeps the stable leg meaningful |
| Platforms | Linux (`verify`), macOS and Windows (`platform`, `nodb` row) — a toolchain file in a nested directory must be honoured on all three, and the Windows path text (the verbatim canonical-path defect `phase-011-evidence.md` §5 records: observed at `03a3e8d`, fixed in `f95ab6b` and `2df9f81`) must not regress |
| Commands | `renvor new` (real and dry run), `renvor generate auth` (scratch verification), `renvor generate resource` (`rustfmt`), `renvor check` (reporting), `renvor doctor` (the operator's probe) |

### 2.8 Dependencies and decisions the maintainer must take — *Decision needed*

| # | Decision | Proposal (not a decision) |
|---|---|---|
| **D-L2-0** | **The contract version to target.** L-2 names "C-4 1.2.0". On `main` at `ab701d2`, `contracts/template-contract.md`'s status text already records a **1.2.0** revision (2026-09-05, the correction round) while its front-matter `version` still reads **1.1.0**; `contracts/command-surface.md` has the same shape (`1.3.0` in front matter, a `1.4.0` revision in the status text). Whether the front matter or the status text is authoritative decides whether L-2's target is "1.2.0" or "the next revision, 1.3.0" | treat the status text as authoritative, correct both front-matter fields in a records-only change, and give L-2's revision the next number. **Not fixed here** — contract edits are outside a planning record |
| **D-L2-1** | **The mechanism**: (a) a rendered `rust-toolchain.toml` (rustup selects the compiler; needs rustup); (b) `rust-version` in `Cargo.toml` (cargo refuses an *older* compiler only; a newer one still runs); (c) both; (d) record-only (`verified_with` in the record, no pin) | (c): the file selects, `rust-version` documents and refuses older compilers where rustup is absent; the record names what ran |
| **D-L2-2** | What the **stable** gate leg proves once trees are pinned | keep it: rustup's `RUSTUP_TOOLCHAIN` precedence lets the stable leg verify pinned trees on stable; assert the precedence (AC-L2-6) |
| **D-L2-3** | Whether `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, `RUSTDOCFLAGS` stay in `PASSED_THROUGH` | keep `RUSTC_WRAPPER` (build caches are the intended use) and record the residual as a ledger row; drop nothing without a control that shows the seal still verifies with a wrapper set |
| **D-L2-4** | Whether the **skeleton** (the dependency-free tree) pins too, and to what — it has no framework path to read a channel from | pin the skeleton to the generator's own MSRV (a constant the crate already knows: `renvor doctor`'s minimum reads it), so the two trees differ only in source, not in policy |
| **D-L2-5** | Whether the pin follows the framework checkout's file at generation time or a constant compiled into `renvor` | the checkout's file for a starter (the starter's compiler must match the framework it depends on by path); a constant for the skeleton |
| **D-L2-6** | A machine **without rustup** (a distribution `cargo`): the file is inert, `rust-version` still refuses older, and the record still names what ran | accept; state it in the generated README |
| **D-L2-7** | Whether `renvor doctor` should read the pin of a project in the current directory and report the installed toolchains against it | yes, as reporting only; no new refusal |

Dependencies: none outside the repository. The template version moves (snapshot policy), `renvor check` learns one key, and `SUPPORT.md`/`CONTRIBUTING.md` may need a sentence on rustup.

---

## 3. Findings adjacent to this plan, recorded for Phase 012 triage — *not security rows, not closed, not decided*

These surfaced while this plan and the cheat-sheet execution were prepared on 2026-09-06 and belong to Phase 012's correctness list, not to L-1/L-2:

1. **A framework-backed starter with `--seed-data` and `--auth none` fails its own pre-placement `cargo fmt --check`** on `src/seed.rs` (rustfmt wants `SeedDeclaration::new(…)` broken over lines), so `renvor new … --database postgres --example-domain --seed-data --capabilities storage --framework-path <checkout> --yes` is refused with `project_verification_failed` ("a defect in renvor's templates") and nothing is placed. Reproduced on both ORMs and on both toolchains (`rustfmt` 1.8.0 under 1.94.0, 1.9.0 under 1.97.1); the same command without `--seed-data`, or with `--auth session --capabilities mail`, succeeds. No census row renders seeds without auth (`FULL` is the only flag set with `--seed-data`, and every `FULL` row has auth) — the "combination no row renders" shape of L-16. The skeleton's seeded variant is unaffected (it uses `templates/src_seed.rs.j2`, a different file). Reproduction and controls: the cheat sheet's verification record for 2026-09-06.
2. **Two contract front-matter versions lag their own status text** (`template-contract.md` 1.1.0 vs 1.2.0; `command-surface.md` 1.3.0 vs 1.4.0) — D-L2-0 above.
3. **The generated Dockerfile's comment names a `rust-toolchain.toml` the generated project does not contain** — §2.2 and AC-L2-7.
4. **A generated starter's `/metrics` renders only the jobs families.** Observed 2026-09-06 on a `cache,jobs,mail,storage,observability` starter after cache reads, a file write, and a mail submission: `process_start_time_seconds` and the six `renvor_jobs_*` families, nothing from `renvor_cache_*`, `renvor_mail_*`, or `renvor_storage_*`. The starter's `src/capabilities/jobs.rs` takes the metrics `Registry`; the cache, mail, and storage modules construct their providers without it (`crates/renvor-cli/templates/starter/src_capabilities_{cache,mail,storage}.rs.j2`), so the counters the capabilities contract names are not rendered by the starter's own route. An observation, not a security row; the capabilities crates' own suites still prove the counters.
5. **`a_hostile_argv0_does_not_reach_the_terminal_raw` (`crates/renvor-cli/tests/hostile.rs`) failed once in CI on a documentation-only change**: the `verify (1.94.0)` job of this pull request's run on `2f74962` (2026-09-06, `ubuntu-latest`) panicked at the `expect("the planted binary runs")` with `Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }`, step `[4/9] tests`; the same test passed three consecutive local runs on 1.94.0 (macOS) and every earlier CI run of the tree. The test copies the `renvor` executable into a temporary directory and executes the copy at once, in a test binary whose other tests spawn `renvor` children concurrently — the shape in which Linux answers `ETXTBSY` when a fork inherits the still-open write descriptor of the copy until its own exec. That is a hypothesis from the error and the test's shape, not a proven cause. It is recorded here because the test is a terminal-injection control (a raw `ESC` from `argv[0]` must never reach a human stream): a control that flakes invites routine re-runs, and routine re-runs hide a real failure. A Phase 012 fix belongs in the test harness, not in the generator.

## 4. What this file does not do

It does not close, narrow, waive, or reinterpret L-1 or L-2; it does not edit the rows or their
ledgers; it does not change a contract, a template, a test, or a workflow; it does not start Phase
012, whose specification (`specs/012-…/spec.md`) does not exist yet and will take this file as an
input. Every "Proposal" and "Decision needed" above stands until the maintainer disposes of it in
that specification or in a decision record.

## 5. How this record was checked

- The rows were copied from `git show main:governance/phase-011-limitations.md` and
  `git show main:governance/phase-010-limitations.md` at `ab701d2` without edits.
- Every file, function, template, and test named in §1.3, §1.4, §2.3, and §2.4 was located in the
  tree at `ab701d2` before it was named here; line numbers are deliberately omitted so the
  references survive unrelated edits.
- The measurements of 2026-09-06 are the cheat-sheet execution's (out of repository) and one
  throw-away probe crate; neither touched the framework tree.
