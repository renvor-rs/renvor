<!--
  AUTHORITATIVE PUBLIC COPY of the ratified Renvor Constitution.
  This file is the discoverable copy referenced by all public documentation (spec FR-012).
  Amendments follow the process in the Governance section below.
-->

# Renvor Constitution

## Core Principles

### I. Cohesive, Explicit Rust

Renvor MUST provide cohesive framework ergonomics while preserving normal, inspectable, idiomatic Rust. I/O, async work, state, configuration, transactions, authorization, failures, and lifecycle changes MUST remain visible in APIs and traces. Generated code and macros MUST have understandable expansion or manual equivalents. Hidden global service location, implicit network work, and opaque runtime behavior are prohibited.

Public APIs SHOULD make the correct path easy without preventing direct access to the underlying package when the boundary permits it. Unsafe Rust MUST be absent by default; any unavoidable unsafe block MUST be isolated, justified with a safety invariant, reviewed, and covered by focused tests.

### II. Transport-Independent Application Core

Domain rules and application services MUST NOT depend on REST, GraphQL, Axum request types, frontend frameworks, or Tauri commands. Delivery adapters MUST translate external requests into shared application commands and queries. Authentication context, validation, authorization policies, transaction boundaries, and business outcomes MUST have parity across equivalent transports.

Dependencies MUST point inward: delivery and infrastructure adapters depend on application ports, and application code depends on the domain. A transport MAY expose transport-specific capabilities only when the distinction is documented and does not duplicate domain logic.

### III. Package-First Boundaries

Before implementing common infrastructure, the phase research MUST evaluate maintained ecosystem packages for fitness, security posture, license, maintenance, documentation, MSRV, feature cost, and interoperability. Selected packages MUST sit behind the narrowest stable Renvor boundary justified by the product contract.

Renvor MUST NOT create a custom runtime, HTTP engine, ORM, cryptographic primitive, queue, parser, template engine, frontend platform, or desktop security mechanism merely to own the implementation. Custom infrastructure requires an accepted ADR documenting evaluated packages, their concrete shortcomings, ownership cost, and an exit strategy.

Package failures MUST remain visible. A required package or external capability MUST NOT be silently replaced by an in-memory, insecure, or differently durable implementation.

### IV. Deterministic Lifecycle and Failure Semantics

The runtime MUST implement the lifecycle `Load -> Validate -> Register -> Boot -> Ready -> Drain -> Stop`. Required configuration and dependencies MUST be validated before readiness. Partial startup MUST roll back initialized providers in reverse order. Shutdown MUST reject new work, drain within explicit bounds, and stop providers in reverse order.

Configuration errors, trust failures, unavailable durable storage, migration failures, invalid credentials, and required capability failures MUST stop the affected operation with actionable diagnostics. Silent fallbacks and success reports after partial failure are prohibited. Retries MUST be bounded, observable, safe for the operation, and documented.

### V. Contract-First Compatibility

Public behavior MUST be defined by versioned contracts before implementation is considered complete. REST MUST use the current approved OpenAPI standard and RFC 9457 Problem Details. The initial target is OpenAPI 3.2.0, and emitted documents MUST NOT claim a version that selected tooling does not correctly implement. GraphQL MUST publish a stable schema and compatibility rules. CLI commands MUST define flags, prompts, exit codes, stdout/stderr behavior, JSON output, and cancellation semantics. Generated client contracts MUST be reproducible from reviewed backend artifacts.

The declared compatibility matrix is a release contract. Direct SQLx with PostgreSQL, direct SQLx with MySQL, SeaORM with PostgreSQL, and SeaORM with MySQL MUST pass shared contract suites before REST 1.0. Product 3.0 frontend and desktop combinations MUST pass their declared build and end-to-end gates before appearing in the wizard.

Unsupported combinations MUST fail before writes or startup with a precise incompatibility message. Renvor MUST NOT substitute another ORM, database, transport, frontend, styling profile, render mode, auth mode, or storage system.

### VI. Security, Privacy, and Fail-Closed Defaults

Every phase MUST identify changed trust boundaries and apply current relevant controls from OWASP ASVS 5.0.0, NIST SP 800-63B-4, the OWASP cheat sheets, and applicable protocol specifications. These references guide verification; they MUST NOT be presented as certification.

Input MUST be validated for type, length, format, encoding, cardinality, and semantic constraints. Database values MUST use parameter binding; dynamic identifiers and ordering MUST use allowlists. Authorization MUST be deny-by-default and enforced inside application operations. Passwords MUST use Argon2id with reviewed parameters. Sessions, tokens, reset links, and desktop credentials MUST support expiry, rotation or revocation as appropriate.

Secrets MUST NOT enter repositories, generated manifests, logs, telemetry, URLs, browser bundles, desktop resources, examples, fixtures, or snapshots. Responses MUST redact implementation details and personal data. Work, bodies, uploads, queues, queries, pagination, retries, and concurrency MUST be bounded. Security-sensitive failures MUST fail closed.

Browser bearer and refresh credentials MUST NOT be stored in `localStorage` or `sessionStorage`. Tauri long-lived credentials MUST use an audited operating-system-backed secret store. Tauri capabilities, IPC, navigation, content security policy, plugins, signing, and updates MUST follow least privilege.

### VII. Deterministic and Safe Generation

`renvor new` MUST provide an interactive wizard and an equivalent non-interactive flag for every choice. Both interfaces MUST resolve to the same validated configuration and project manifest. The wizard MUST ask for target, transport, persistence model, database, auth starter, frontend, compatible render mode, styling profile where applicable, desktop option, capabilities, and local tooling.

Next.js, Yew, Dioxus, and Leptos MUST each offer plain CSS, SCSS, and Tailwind CSS as explicit styling choices. All three are first-party profiles with equivalent functionality, accessibility, auth flows, theme support, and verification. Only selected styling dependencies and files may be generated.

Generation MUST validate the entire selection before writing, render in an owned staging directory, verify the result, and commit output through an atomic destination change where the platform permits it. Cancellation or failure MUST leave the requested destination unchanged. Existing user files MUST NOT be overwritten. `--dry-run` MUST produce an accurate file manifest without writes.

Generated code MUST be readable, formatted, documented, testable, and owned by the application team. Templates MUST be versioned and reproducible. Unverified executable templates MUST NOT be downloaded or evaluated during normal generation.

Package lifecycle commands MUST provide equivalent dry-run, non-interactive, JSON-output, conflict, verification, and rollback behavior. Adding a package to an existing project MUST preserve user changes and MUST NOT execute arbitrary remote installation scripts.

### VIII. Feature and Platform Isolation

REST-only applications MUST NOT compile GraphQL dependencies. API-only applications MUST NOT compile or install frontend or Tauri dependencies. Selecting one persistence or styling profile MUST NOT install unselected direct dependencies. Server-only dependencies and secrets MUST remain outside browser, WebAssembly, and desktop asset graphs.

Feature combinations MUST be checked through minimal, individual, representative combination, and all-supported-feature builds. Mutually exclusive features MUST fail with corrective compile-time or generation-time messages.

Tauri MUST default to a remote authenticated Renvor backend. An embedded server, sidecar, or in-process application adapter requires a separate accepted ADR and threat model. Next.js inside Tauri MUST use a validated static-export profile without server actions, request-time rendering, server route handlers, or server secrets.

### IX. Real-Boundary Verification

Tests MUST exercise the boundary where the risk exists. Database adapters require real PostgreSQL and MySQL integration tests. HTTP and GraphQL behavior requires real router/middleware execution. Auth requires adversarial flow and policy tests. Generated projects must format, compile, migrate, start, and execute representative operations. Frontends require production builds, accessibility checks, and browser end-to-end tests. Tauri requires native-platform security and lifecycle smoke tests.

Unit tests alone are insufficient for cross-component promises. Contract suites MUST be shared across interchangeable adapters. Parsers and untrusted formats SHOULD receive property or fuzz testing. Runtime work SHOULD receive load, soak, cancellation, backpressure, graceful-shutdown, and failure-injection tests appropriate to risk.

Flaky tests are defects. Temporary quarantine requires an owner, reason, expiry, and preserved release coverage. A phase MUST NOT pass while its stated acceptance evidence is missing.

### X. Documentation Is a Release Artifact

Every public feature MUST ship with reference documentation, a tested example, configuration and security guidance, failure behavior, compatibility information, and upgrade notes. CLI help, OpenAPI, GraphQL schemas, crate documentation, generated templates, examples, and the documentation site MUST describe the same contract.

Documentation builds, links, snippets, and quickstarts MUST be verified. Limitations and development status MUST be visible. Claims about performance, reliability, compatibility, or security MUST link to reproducible evidence and MUST NOT exceed what was measured.

### XI. Supply-Chain and Release Integrity

Applications and release tools MUST use committed lockfiles. Publishable crates MUST declare license, repository, documentation, description, Rust version, and intended contents. Dependencies and licenses MUST be reviewed; advisories MUST be monitored. Workflow permissions MUST be least privilege and third-party workflow actions MUST be immutably pinned.

Release candidates MUST pass formatting, linting, MSRV/current-stable tests, documentation tests, feature isolation, semantic-version compatibility, dependency policy, security review, package inspection, and clean generated-project tests. Releases MUST include checksums, an SBOM, provenance, signed tags, and signed platform artifacts where applicable.

After the one-time first-release bootstrap required for a new crates.io name, publication MUST use protected identity-federated trusted publishing for the approved repository and workflow. Bootstrap credentials MUST be least-scope, separately approved, never committed, and revoked immediately after use. Long-lived registry credentials are prohibited. Because published crate versions are immutable, a defective publication MUST be yanked and replaced with a new version rather than overwritten.

### XII. Simplicity, Phasing, and Honest Scope

Reliability precedes breadth. REST 1.0 MUST be stable before GraphQL 2.0 work is released. GraphQL 2.0 MUST be stable before full-stack web and Tauri 3.0 are released. The installable package ecosystem follows product 3.0. Vue and Angular remain later research candidates until separately specified, reviewed, and accepted.

A new abstraction, macro, crate, feature flag, provider, or compatibility row MUST solve a demonstrated requirement. The smallest design satisfying the contract is preferred. Complexity MUST be recorded in an ADR when it creates a lasting public or operational commitment.

Renvor MUST identify unshipped capabilities as planned and MUST NOT imply they are available. Performance marketing without measurements, durability claims without failure tests, and compatibility claims without matrix evidence are prohibited.

### XIII. Independent Installable Packages

Renvor MUST support separately developed, separately versioned packages published as normal crates on crates.io and installed into an existing compatible application with `renvor add`. Official packages MUST live outside the core workspace, use only public extension contracts, and own their repository, release, support, security, documentation, and crates.io lifecycle.

Package installation MUST be a declarative, previewable, transactional source change followed by dependency resolution, formatting, build, tests, and an explicit migration/deployment plan. It MUST NOT inject native code into an already running process, mutate a live production database, execute arbitrary remote scripts, overwrite user changes, or hide added permissions and Tauri capabilities.

Every package MUST declare framework compatibility, MSRV, dependencies, features, providers, configuration, migrations, generated operations, supported database/frontend/styling rows, permissions, conflicts, license, provenance, upgrade behavior, and non-destructive removal behavior. Unsupported or ambiguous installations MUST fail before writes.

The separately published `renvor-rbac` crate is the first official reference package. Its role and permission checks MUST execute in application services, remain deny-by-default, preserve tenant boundaries, invalidate cached grants correctly, and pass every claimed persistence and frontend contract.

## Architecture and Technology Constraints

- The workspace MUST use stable Rust, the Rust 2024 edition, Cargo resolver 3, and an explicit MSRV tested in continuous integration.
- Tokio, Axum, and Tower are the planned runtime/HTTP foundation. Replacements require an accepted ADR.
- Persistence MUST expose direct SQLx and SeaORM programming models for PostgreSQL and MySQL. SeaORM's internal use of SQLx MUST be documented accurately and is not a violation of model isolation.
- REST MUST stabilize before optional GraphQL. Equivalent operations MUST reuse application services and policies.
- Next.js MUST use the App Router and strict TypeScript.
- Next.js, Yew, Dioxus, and Leptos MUST each offer plain CSS, SCSS, and Tailwind CSS as selectable styling profiles, using the selected framework's supported asset pipeline.
- Yew, Dioxus, and Leptos MUST enter the supported matrix only for render and target modes proven against their frozen versions.
- Tauri MUST use the Tauri 2 capability model, local assets, strict navigation/CSP policy, validated commands, signed artifacts, and signed updates.
- Frontend templates are CLI assets, not default Rust runtime dependencies.
- Installable packages MUST be normal separate crates on crates.io with declarative Renvor metadata and public extension contracts; the core repository MUST NOT absorb official packages merely for release convenience.
- Important external packages, version freezes, and licenses MUST be recorded in phase research before implementation.

## Package Ecosystem Governance

- Each official package MUST have a separate repository, crates.io package identity, owners, release history, support policy, security contact, continuous integration, documentation, and semantic version.
- crates.io is the canonical source for installable Renvor package crates. A discovery catalog MAY index compatibility and evidence but MUST NOT replace registry integrity verification.
- `renvor add` MUST install a package into an existing compatible Renvor application's source and manifests, then verify the project. It MUST NOT hot-load code into a live Rust process.
- Package assets embedded in a crate MUST be listed by package inspection and treated as untrusted until validated. JavaScript dependencies declared by a frontend companion MUST use the selected frontend's normal registry and lockfile.
- Core and package versions MUST remain independent. Every package MUST declare and test its supported Renvor range.
- Official packages MUST use the same dependency, license, provenance, SBOM, advisory, trusted-publishing, and release-review controls as core crates.
- Removing a package MUST preserve its application data by default. Destructive cleanup requires a separate explicit operation with a preview and recovery plan.

## Security and Privacy Requirements

- Each specification MUST include abuse cases, sensitive-data classification, authentication/authorization impact, resource bounds, and failure behavior.
- Authentication defaults MUST use opaque server-side browser sessions unless the specified client mode requires a different reviewed protocol. Optional API tokens MUST validate fixed algorithms, issuer, audience, expiry, and not-before claims.
- Browser cookie sessions MUST use secure attributes and CSRF defenses. Safe HTTP methods MUST NOT change server state.
- Password reset and verification tokens MUST be generated with operating-system randomness, stored in a non-recoverable form, expire, and be single-use.
- Production services MUST use authenticated TLS to external dependencies where supported. Local HTTPS trust changes MUST be explicit and reversible.
- Logs and telemetry MUST use structured fields, correlation identifiers, and centrally tested redaction.
- Release blockers include known critical/high vulnerabilities, credential exposure, authorization bypass, unbounded attacker-controlled work, unsafe package installation behavior, unsigned required artifacts, or missing required compatibility evidence.

## Development and Phase Workflow

1. Work MUST follow the numbered phases in `/PLAN.md`; one Spec Kit feature directory represents one active phase.
2. The phase specification MUST state user outcomes, exclusions, security properties, and measurable acceptance criteria before implementation planning.
3. Research MUST verify package APIs, maintenance, versions, licenses, MSRV, and standards against primary sources.
4. Consequential decisions MUST be captured as proposed ADRs and reviewed before being treated as accepted.
5. Tasks MUST be dependency ordered, independently verifiable, and include tests, documentation, security, migration, and compatibility work.
6. Implementation MUST keep the workspace buildable and MUST preserve unrelated work.
7. An independent review MUST compare implementation evidence with the specification, constitution, compatibility matrix, and security checklist.
8. A phase remains open when blockers or acceptance gaps exist. Fix and review loops continue until the phase passes.
9. Only maintainers approve phase completion and public release.

## Governance

This constitution governs all Renvor specifications, plans, ADRs, code, templates, examples, documentation, and releases. `/PLAN.md` is the program execution authority but MUST comply with this constitution. When another document conflicts, the constitution takes precedence.

Amendments require:

1. a written proposal explaining the change and affected principles;
2. impact analysis for public APIs, generated projects, security, compatibility, documentation, and active phases;
3. a migration plan when existing behavior or evidence changes;
4. maintainer approval;
5. an updated semantic version and amendment date;
6. synchronization of affected templates and guidance before new phase work proceeds.

Constitution versions follow semantic versioning:

- **MAJOR:** removes or redefines a governing principle or compatibility promise.
- **MINOR:** adds a principle or materially expands mandatory governance.
- **PATCH:** clarifies wording without changing required behavior.

Every phase review and release review MUST include a constitution check. Exceptions are allowed only through a time-bounded written waiver naming the violated rule, reason, compensating controls, owner, expiry, and removal plan. Security release blockers cannot be waived for a public release.

**Version:** 2.0.0 | **Ratified:** 2026-08-11 | **Last Amended:** 2026-08-17
