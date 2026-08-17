# Feature Specification: Governance, Names, Toolchain, and Repository Security Foundation

**Feature Directory**: `specs/001-governance-foundation`

**Feature Branch**: `001-governance-foundation` *(intended name; no branch was created by this command because no branch hook is configured — the working branch at specification time was `master`)*

**Phase**: PLAN.md Phase 001

**Created**: 2026-08-11

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=\"specs/001-governance-foundation\" Specify Phase 001 from PLAN.md as one independently verifiable feature. Establish the trustworthy Renvor project foundation. Verify public names on GitHub and crates.io, ratify governance, create the Rust 2024 resolver-3 workspace, define the MSRV and license policies, secure repository defaults, choose the documentation platform, and create the CI and release skeleton. Exclude runtime framework features. Success requires every Phase 001 acceptance criterion in PLAN.md."

## Clarifications

### Session 2026-08-11

- Q: At the end of Phase 001, are public names only verified, or actually taken (organization registered, placeholder versions published to reserve package names)? → A: Verify every name on both platforms and register/claim the source-hosting organization and repository; publish nothing to the package registry. Package names remain technically claimable by others until a later phase publishes, and that residual risk is recorded rather than eliminated.
- Q: Which license applies to Renvor's source and documentation, and what obligation attaches to generated project output? → A: Dual `MIT OR Apache-2.0` at the recipient's choice for Renvor's own source and documentation; contributions accepted under the same dual terms; project code generated for a user carries no Renvor licensing obligation and is owned outright by that user.
- Q: How is the default branch protected when a single maintainer cannot approve their own pull request? → A: A pull request and all required checks are mandatory, direct pushes are blocked for everyone including administrators with no bypass, and zero human approvals are required while the project has one maintainer. The absent human review is recorded as a time-bounded waiver that expires when a second maintainer joins.
- Q: Public or private repository during Phase 001, and which security controls are mandatory before coding? → A: The repository is created public and stays public for its whole life, so every platform security control a public repository provides is free and live from day one with nothing waived for unavailability. The first content push is gated: names verified, both license texts present, and the security contact live before anything is pushed. This satisfies PLAN.md §19.1 rather than deviating from it, and the zero-publication rule for the package registry still stands.
- Q: Which platform hosts the versioned, searchable prose documentation? → A: Docusaurus, chosen for first-class multi-version documentation and built-in search, mature accessible theming, permissive licensing, and active maintenance. The accepted cost is a Node toolchain in the documentation build path; alternatives considered and rejected were mdBook (no built-in versioning), MkDocs with Material (adds a Python toolchain the program does not otherwise need), and Zola (versioning, search, and docs navigation all hand-built).
- Q: What is the initial MSRV and what policy governs it, given that a release-count window would expire on 2026-08-20? *(maintainer decision, supplied directly)* → A: The initial MSRV is **Rust 1.94.0**, declared as a **fixed, explicitly versioned support floor** — not an N-3, N-4, or otherwise rolling value. A new Rust stable release does not invalidate it. Continuous integration tests the exact declared MSRV and the current stable channel. The floor may be raised only in a planned minor or major Renvor release, and only after an accepted decision record names a concrete dependency, language, or security requirement forcing it; every change is documented in the support policy, changelog, and release notes. Each newly declared MSRV is supported for at least six months. The policy is reviewed quarterly, but a review does not by itself change the declared version. Rust 1.94.0 must be revalidated against the actual persistence dependencies before Phase 006. Separately, `resolver = "3"` must be declared explicitly in the virtual workspace manifest rather than relied upon through edition inheritance.

## User Scenarios & Testing *(mandatory)*

The people served by this feature are the project maintainers, prospective contributors, security reporters, and the downstream consumers who will later judge whether Renvor is safe to depend on. Nothing here delivers runtime framework behavior; the deliverable is a foundation that can be trusted, audited, and repeated.

### User Story 1 - Verified public identity before anything is frozen (Priority: P1)

A maintainer needs to know, with dated evidence, that every public name Renvor intends to occupy is actually available or already owned by the project: the source-hosting organization and repository, every crate name, the installed executable name, and the documentation domain. Until that evidence exists, no public reference is frozen and nothing is pushed publicly. If any name is unavailable or ownership is ambiguous, work stops for an explicit, recorded naming decision — no substitute name is ever chosen automatically.

**Why this priority**: Every later artifact — manifests, documentation, templates, examples, release automation — embeds these names. Discovering a conflict after they are embedded forces a rename across the entire program and destroys published references. This is the one decision that must be right before anything else is built.

**Independent Test**: Read the name availability record. For each name in the public identity contract, confirm it states the location checked, the date checked, the observed status, and who checked it. Confirm no unconfirmed name appears in any frozen public reference, and confirm that at least one deliberately unavailable name is handled by stopping rather than substituting.

**Acceptance Scenarios**:

1. **Given** the public identity contract lists a product name, crate prefix, facade crate, CLI package, executable name, project state directory, and environment prefix, **When** the maintainer completes name verification, **Then** every one of those items plus the hosting organization/repository and the documentation domain has a dated evidence entry with a definite status.
2. **Given** a required crate name is already occupied by another party, **When** verification records that status, **Then** the feature is blocked, an explicit naming decision is requested, and no alternative name is adopted automatically.
3. **Given** verification is incomplete for any single name, **When** a maintainer attempts to freeze public references or publish anything, **Then** the action is refused and the reason names the unverified item.
4. **Given** the product is named `Renvor` while the installed executable is `renover`, **When** a reviewer reads the accepted naming decision record, **Then** the distinction is explained deliberately and every document, test, and example uses the executable name consistently. *(Historical: this scenario was written and satisfied while the executable was `renover`. ADR-0010 unified the names on 2026-08-17 — the executable is now `renvor` and there is no distinction left to explain. The scenario is preserved as the Phase 001 acceptance criterion it was, not rewritten.)*

---

### User Story 2 - A clean checkout that verifies itself (Priority: P2)

A new contributor clones the repository on a supported operating system, follows the documented setup, and runs one documented verification sequence. Formatting, linting, tests, and documentation checks all execute and pass. Afterwards the working tree is still clean — no build output, no local environment file, no editor or operating-system artifact, and no secret is left untracked or modified.

**Why this priority**: A foundation that cannot verify itself from scratch cannot support any later phase. This is also the criterion that proves the ignore rules and toolchain policy actually work rather than merely being documented.

**Independent Test**: On a machine with no prior project state, clone, run the documented verification sequence, and confirm every check executes (never silently skipped) and passes, then confirm the working tree reports no untracked or modified files.

**Acceptance Scenarios**:

1. **Given** a fresh clone with no cached state, **When** the contributor runs the documented verification sequence, **Then** formatting, linting, tests, and documentation checks each run to completion and report success.
2. **Given** the verification sequence has completed, **When** the contributor inspects the working tree, **Then** no build output, local configuration, editor artifact, operating-system artifact, or secret-bearing file appears as untracked or modified.
3. **Given** the declared minimum supported toolchain version of 1.94.0 and the current stable toolchain, **When** verification runs on each, **Then** both pass and any toolchain older than the declared floor is refused with an actionable message rather than silently accepted.
4. **Given** a newer Rust stable release ships, **When** verification runs afterwards, **Then** the declared floor is unchanged, the pinned minimum-version job still targets 1.94.0, and no policy violation is recorded.
5. **Given** a check cannot run in the current environment, **When** verification executes, **Then** the run fails with a stated reason instead of reporting success with a skipped check.

---

### User Story 3 - Governance, licensing, and a working security-reporting path (Priority: P3)

A prospective contributor, a legal reviewer, and a security researcher each arrive at the repository root. Within one navigation step each finds what they need: how the project is licensed and how contributions are licensed, how decisions are made and who approves them, how to contribute, the expected conduct, what is supported for how long, and a private path to report a vulnerability that reaches a named, monitored contact.

**Why this priority**: These documents convert an unknown repository into one an organization can legally and safely adopt. They also gate the first public push: PLAN.md forbids publication until ownership, names, licenses, and security contacts are confirmed.

**Independent Test**: From the repository root, locate the license, contribution guide, code of conduct, security policy, support policy, and governance document. Verify the security policy's private reporting path resolves to a monitored contact, and verify each publishable package's declared license matches the stated license policy.

**Acceptance Scenarios**:

1. **Given** a visitor at the repository root, **When** they look for licensing, contribution, conduct, security reporting, support, and governance information, **Then** all six are discoverable within one link from the root landing document.
2. **Given** the license policy, **When** a reviewer inspects every publishable package's declared metadata, **Then** the declared license matches the policy exactly, with no package left unlicensed or inconsistently licensed.
3. **Given** a security researcher with a suspected vulnerability, **When** they follow the documented private reporting path, **Then** the report reaches a named, monitored contact without being disclosed publicly first, and the policy states the expected acknowledgement window.
4. **Given** a consequential decision was made during this feature, **When** a reviewer inspects its decision record, **Then** the record shows an explicit state and, if accepted, the name and date of an independent review — no record is marked accepted without one.
5. **Given** the ratified constitution, **When** a reviewer opens the repository root, **Then** the constitution, its version, and its ratification date are discoverable and consistent with what governance documents claim.

---

### User Story 4 - Secure repository defaults and least-privilege automation (Priority: P4)

A maintainer configures the hosting platform so that the default branch can only change through a pull request with passing checks and no administrator bypass, automation runs with the smallest permission it needs, third-party automation is pinned to immutable references, and the platform's secret-scanning, dependency-review, and code-scanning protections are on. Any protection the plan cannot provide — including the independent human review a single maintainer cannot supply — is recorded with a reason, a compensating control, an owner, and an expiry, never silently omitted.

**Why this priority**: Supply-chain compromise is the highest-impact risk in the program's risk register, and every later phase inherits whatever posture is established here. Retrofitting protections after contributors and automation exist is far more disruptive.

**Independent Test**: Inspect the hosting platform configuration and every automation definition. Confirm the default branch requires a pull request and passing checks with no bypass permission on any account, that the current approval requirement matches the maintainer count and any shortfall carries a waiver, that every automation grants read-only permission by default, that no third-party automation reference is mutable, and that each unavailable protection has a recorded waiver with owner and expiry.

**Acceptance Scenarios**:

1. **Given** the default branch, **When** any account including an administrator attempts to push directly or to merge with a failing required check, **Then** the change is refused and no bypass permission exists that would allow it.
2. **Given** the project has a single maintainer, **When** that maintainer merges their own pull request with zero approvals, **Then** the merge is permitted and a recorded waiver names the missing independent review, its compensating controls, its owner, and the second-maintainer condition that ends it.
3. **Given** every automated workflow, **When** a reviewer inspects declared permissions, **Then** the default is read-only and each elevated permission is scoped to the job that needs it and justified.
4. **Given** every third-party automation step, **When** a reviewer inspects its reference, **Then** it resolves to an immutable version rather than a moving tag or branch.
5. **Given** the repository is public from creation, **When** a reviewer inspects its settings before the first content push, **Then** secret scanning with push protection, code scanning, dependency graph and alerts, and dependency review are all enabled and verified working, with none disabled for cost or plan reasons.
6. **Given** a report template, a pull-request template, a security-reporting template, and a release template, **When** a contributor opens the corresponding flow, **Then** the template appears and requests the information reviewers need.

---

### User Story 5 - A documentation platform chosen on evidence (Priority: P5)

The maintainers evaluate candidate documentation platforms against the project's actual needs — versioned output, working search, link checking, tested code snippets, accessible output, reproducible builds, an acceptable license, and healthy maintenance — then record the choice, the rejected alternatives, the reasons, and a named owner. A placeholder documentation set builds from a clean checkout and its links check clean.

**Why this priority**: Documentation is a release artifact under the constitution, and the platform choice constrains every later phase's documentation work. Choosing it now, on recorded evidence, prevents an unreviewed default from becoming permanent.

**Independent Test**: Read the documentation platform decision record; confirm it names the evaluated alternatives, the evaluation criteria, the decision, the consequences, and an owner. Then build the placeholder documentation set from a clean checkout, run link checking, and confirm a machine missing the documentation toolchain fails the sequence with an actionable message rather than skipping it.

**Acceptance Scenarios**:

1. **Given** the documentation platform decision record, **When** a reviewer reads it, **Then** it names Docusaurus as the decision, lists mdBook, MkDocs with Material, and Zola as rejected alternatives with reasons, states the accepted Node toolchain cost, and names an owner — and it is not marked accepted without a recorded review.
2. **Given** a clean checkout, **When** the placeholder documentation set is built, **Then** the build succeeds and link checking reports no broken links.
3. **Given** the selected platform, **When** a reviewer checks it against the stated criteria, **Then** multi-version output and search are demonstrated working rather than assumed.
4. **Given** a contributor without the documentation toolchain installed, **When** they run the verification sequence, **Then** the run fails with a message naming the missing toolchain and how to install it, rather than skipping the documentation checks and reporting success.

---

### User Story 6 - A release rehearsal that provably publishes nothing (Priority: P6)

From a clean checkout, a maintainer runs the release rehearsal. It packages a placeholder internal package, inspects the exact file list that would ship, validates the required package metadata, and performs zero publish operations. The rehearsal records what was produced, on which platform, by whom, and when — and the resulting evidence pack maps every Phase 001 acceptance criterion to a dated artifact.

**Why this priority**: This is the criterion that proves the release path exists and is safe before any name is permanently consumed on a public registry. It also produces the evidence pack that gates entry to Phase 002.

**Independent Test**: Run the rehearsal from a clean checkout, confirm a package artifact is produced for the placeholder package, confirm the registry recorded no new version, and confirm each acceptance criterion in the evidence pack links to a dated artifact.

**Acceptance Scenarios**:

1. **Given** a clean checkout, **When** the release rehearsal runs, **Then** it produces a package artifact for the placeholder package and performs no publish operation against any public registry.
2. **Given** the rehearsal completes, **When** a reviewer inspects the packaged file list, **Then** it contains only intended files — no secret, no local configuration, no build output, and no unintended asset.
3. **Given** each package intended for future publication, **When** metadata is validated, **Then** description, license, repository, homepage, documentation, readme, keywords, categories, minimum supported toolchain version, and included files are complete, and no package intended for publication depends on a path-only dependency.
4. **Given** the release documentation, **When** a reviewer reads it, **Then** it states the publication order for the intended package set, the rule to wait for registry availability before dependents, the immutability of published versions, and the yank-and-replace remedy for a defective release.
5. **Given** the completed feature, **When** a reviewer opens the phase completion record, **Then** every Phase 001 acceptance criterion links to dated evidence naming the command, platform, operator, and result.

---

### Edge Cases

- **A required name is taken.** Verification records the conflict, the feature stops, and an explicit naming decision is requested. No substitute is adopted automatically and no partially-renamed state is committed.
- **A name is registered but unused (placeholder or squatted).** Treated as unavailable, not as "probably fine". Ownership must be transferred or an explicit decision recorded.
- **A name is available today but taken tomorrow.** Evidence entries carry dates; an entry older than the recorded validity window must be re-verified before public references are frozen. Because this feature deliberately does not publish to reserve package names, this exposure is accepted and tracked as a known limitation rather than removed.
- **The documentation domain is unavailable while the code names are free.** Recorded as a blocking naming decision in its own right, because published documentation links are effectively permanent.
- **The hosting organization does not exist yet, or the maintainer lacks the rights to configure protections.** Ownership acquisition is part of this feature; protections that cannot yet be applied are recorded as blockers, not quietly deferred.
- **The hosting platform does not offer a required protection.** Recorded with reason, compensating control, named owner, and expiry date. Silent omission is a failure, and cost or plan tier is not an accepted reason because the repository is public and the public tier supplies these controls.
- **A package name is found taken after the public repository already exists.** The repository is created public only once the organization and repository names are verified, but a later conflict on a package name still triggers the stop rule. The rename then happens in the open, before any content push froze the name — which is why FR-052 gates the first push rather than the repository's existence.
- **The declared minimum supported toolchain cannot satisfy a required dependency.** The floor is raised through the recorded process — an accepted decision record naming the forcing requirement, landing in a planned minor or major release — or the dependency is rejected. The declared minimum is never claimed without a passing verification run at that exact version.
- **A newer Rust stable is released mid-phase.** Nothing happens to the declared floor. The stable verification job picks up the new release; the minimum-version job stays pinned at 1.94.0. This is the explicit consequence of choosing a fixed floor over a rolling window, and it is the intended behaviour rather than drift.
- **A quarterly policy review concludes the floor could move.** The review records that conclusion but changes nothing on its own. Moving the floor still requires the decision record, the forcing requirement, the planned release, and the six-month dwell time to have elapsed.
- **A verification check cannot execute in the current environment.** The run fails closed. A skipped check must never be reported as a pass.
- **A secret is discovered in the working tree or history.** Treated as a release blocker: rotate, revoke, purge, record, and only then continue.
- **A decision record is marked accepted without a recorded review.** Treated as a defect in this feature's own acceptance, not a paperwork detail.
- **The default branch name differs between local convention and the hosting platform.** The default branch is named explicitly once, protected, and referenced consistently by automation and documentation. The working repository is currently on `master` while the program treats `main` as the default; this must be reconciled deliberately, not left to platform defaults.
- **The only maintainer must merge their own change.** Permitted while the single-maintainer waiver is active, because required checks and the no-bypass rule still apply. It is a failure only if the waiver is missing, expired, or the approval requirement was never raised after a second maintainer joined.
- **The release rehearsal is invoked with credentials present in the environment.** The rehearsal still performs no publish operation; publishing must fail closed rather than succeed accidentally.
- **A dependency's license is unacceptable or its project is unmaintained.** The dependency policy names the outcome — replace, waive with expiry, or block — rather than leaving reviewers to improvise.

## Requirements *(mandatory)*

### Functional Requirements

#### Public identity and naming

- **FR-001**: The project MUST produce a name availability record covering the product name, the package-name prefix, the facade package, the command-line package, the installed executable name, the project state directory name, the environment-variable prefix, the source-hosting organization and repository names, and the documentation domain.
- **FR-002**: Each entry in the name availability record MUST state the location checked, the date checked, the observed status (available, already owned by this project, held by another party, or ambiguous), and the person who checked it.
- **FR-003**: If any entry is not "available" or "already owned by this project", the feature MUST stop and require an explicit recorded naming decision. Automatic selection of an alternative name is prohibited.
- **FR-004**: Public references MUST NOT be frozen, pushed publicly, or published until every entry in the name availability record is confirmed. Creating the empty public repository to claim the organization and repository names is permitted before the first content push, because that creation is itself part of acquiring the names.
- **FR-052**: The first content push to the public repository MUST already contain the confirmed names, both license texts, and a live security contact. No commit containing an unconfirmed public name may be pushed.
- **FR-053**: Until the project makes its first release, the repository MUST carry a prominent development-status notice stating that nothing has been released, that listed capabilities are planned rather than available, and that the project is not yet ready to be depended upon.
- **FR-005**: An accepted decision record MUST explain the intentional distinction between the product name and the installed executable name, and all documentation, tests, and examples MUST use the executable name consistently.
- **FR-006**: The name availability record MUST state a validity window after which entries require re-verification before public references are frozen.
- **FR-048**: This feature MUST acquire and hold the source-hosting organization and repository under the project's control, because acquisition there is free, reversible, and does not require publication.
- **FR-049**: This feature MUST NOT reserve package-registry names by publishing placeholder versions. The residual risk that a verified but unpublished package name is claimed by another party MUST be recorded as a known limitation with a named owner and the phase that closes it.

#### Governance, licensing, and contacts

- **FR-007**: The repository MUST publish, discoverable within one link from the root landing document, a license, a contribution guide, a code of conduct, a security policy, a support policy, and a governance document naming decision authority.
- **FR-008**: Renvor's own source and documentation MUST be licensed `MIT OR Apache-2.0` at the recipient's choice, with the full text of both licenses present in the repository, and the contribution guide MUST state that contributions are accepted under the same dual terms.
- **FR-009**: Every package intended for publication MUST declare `MIT OR Apache-2.0` license metadata, with no package left unlicensed or declaring different terms.
- **FR-050**: The license policy MUST state that project code generated for a user carries no Renvor licensing obligation and is owned outright by that user, and generated output MUST NOT embed a Renvor license header implying otherwise.
- **FR-010**: A dependency and license policy MUST state which licenses are permitted, which require review, which are prohibited, and how security advisories and unmaintained dependencies are handled, including the outcome options available to a reviewer. Advisory handling MUST state **bounded response windows**, so that an advisory cannot remain unactioned indefinitely without violating a rule. The windows are **Renvor policy decisions, not durations mandated by CVSS, FIRST, RustSec, or NIST**. Maximum time to triage — severity assessed, affected versions determined, named owner assigned — measured from confirmed detection: **24 hours** for known active exploitation or Critical, **48 hours** for High, **5 calendar days** for Medium, **10 calendar days** for Low. Maximum time to remediation, also measured from confirmed detection: known active exploitation begins **immediately** with a mitigation and advisory decision within **24 hours**; Critical **7 calendar days**; High **14 calendar days**; Medium **30 calendar days**; Low **90 calendar days** or the next scheduled prerelease, whichever comes first. Severity MUST NOT be determined by CVSS score alone. Absence of an upstream fix MUST NOT extend a deadline — the dependency is removed, disabled, replaced, or isolated, or the affected release is blocked. Known Critical and High vulnerabilities are public-release blockers and **cannot be waived**. Silently ignoring an advisory without a narrowly scoped, dated record is prohibited. This policy governs advisories against **dependencies** and is distinct from the inbound private-report timetable in FR-011. The complete policy is `governance/dependency-advisory-policy.md`.
- **FR-011**: The security policy MUST provide a private vulnerability reporting path that reaches a named, monitored contact, and MUST state the expected acknowledgement window and disclosure expectations.
- **FR-012**: The ratified constitution MUST be discoverable from the repository root together with its version and ratification date, and governance documents MUST NOT contradict it.
- **FR-013**: The project MUST define a decision-record process with explicit states (at minimum proposed, accepted, rejected, superseded), a recorded reviewer, and a recorded date. A record MUST NOT be marked accepted without a recorded independent review. The governance document MUST establish who qualifies as an independent reviewer. Where no independent reviewer exists, a record MUST NOT be silently accepted: acceptance requires a waiver under FR-015 recording the gap, its compensating controls, an owner, and an expiry date. This process MUST be settled before any decision record in this feature reaches the accepted state.
- **FR-014**: This feature MUST produce accepted decision records for: public naming and namespace; workspace package boundaries and facade stability; minimum supported toolchain version, toolchain pinning, and dependency update policy; and documentation platform and versioning.
- **FR-015**: Any deviation from a governing rule MUST be recorded as a time-bounded waiver naming the violated rule, the reason, the compensating control, the owner, the expiry, and the removal plan.

#### Workspace and toolchain baseline

- **FR-016**: The workspace MUST use stable Rust, the Rust 2024 edition, and Cargo resolver 3, as required by the constitution's architecture constraints.
- **FR-057**: The root workspace manifest MUST declare `resolver = "3"` explicitly rather than relying on inheritance from the 2024 edition, because a virtual workspace has no package edition to inherit from and would otherwise fall back to an older resolver. Verification MUST confirm that minimum-supported-version-aware dependency resolution is actually in effect, not merely configured.
- **FR-017**: The workspace MUST declare **Rust 1.94.0** as its minimum supported toolchain version — hereafter the **MSRV**, the canonical term used throughout this feature's artifacts — in a single authoritative location at the workspace level, from which every publishable package's metadata and the support documentation derive by inheritance rather than by restating the value. No second independent declaration of the MSRV may exist.
- **FR-018**: The minimum supported toolchain version MUST be treated as a fixed, explicitly versioned support floor. It MUST NOT be defined as a rolling offset from current stable, and the release of a newer Rust stable MUST NOT invalidate or automatically change it.
- **FR-058**: The minimum supported toolchain version MUST be raised only in a planned minor or major Renvor release, and only after an accepted decision record names the concrete dependency, language, or security requirement that forces the raise. Each raise MUST be documented in the support policy, the changelog, and the release notes.
- **FR-059**: Each declared minimum supported toolchain version MUST remain in effect for at least six months before being raised.
- **FR-060**: The toolchain policy MUST be reviewed quarterly, and the review MUST record its outcome. A review MUST NOT by itself change the declared version; only the process in FR-058 can do that.
- **FR-061**: Rust 1.94.0 MUST be revalidated against the actual persistence dependencies before Phase 006 begins, and that obligation MUST be recorded as a scheduled item with a named owner.
- **FR-019**: Verification MUST run against both the declared minimum supported toolchain version and the current stable toolchain, and both MUST pass. The minimum is pinned to the exact declared version; the stable job tracks the stable channel.
- **FR-020**: A dependency update policy MUST state how updates are proposed, reviewed, and landed, and MUST prohibit unreviewed floating updates in generated output.
- **FR-021**: A lockfile policy MUST state that applications, generators, release tooling, and automation commit lockfiles, while reusable library packages express compatible version requirements.
- **FR-022**: A clean checkout MUST pass a single documented verification sequence covering formatting, linting, tests, and documentation checks. Placeholder content is permitted where runtime capability does not yet exist, but every check MUST execute and pass rather than be skipped.
- **FR-023**: Any verification check that cannot execute MUST fail the run with a stated reason; a skipped check MUST NOT be reported as a pass.
- **FR-024**: Repository ignore rules MUST exclude build output, local environment and configuration files, editor and operating-system artifacts, and secret-bearing files, such that running the full verification sequence on a clean checkout leaves no untracked or modified files.
- **FR-025**: The repository tree and history MUST contain no secret, credential, token, or private key. A scan MUST be executed and its dated result recorded. The scan authorising the first public push MUST cover the exact state being pushed: a scan performed before subsequent content was created does not satisfy this requirement, and MUST be re-run immediately before the push.
- **FR-026**: The workspace MUST contain at least one placeholder internal package sufficient to exercise the packaging rehearsal, containing no runtime framework capability.

#### Repository security and automation

- **FR-027**: The default branch MUST be explicitly named and protected such that every change arrives through a pull request, all required checks pass before merge, and direct pushes are refused for every account including administrators, with no bypass permission granted to any account.
- **FR-051**: While the project has a single maintainer, the number of required human approvals MUST be zero, and the absent independent review MUST be recorded as a waiver under FR-015. The waiver MUST carry an **absolute expiry date** as well as its release condition, expiring on whichever comes first, because a condition alone is not time-bounded and could persist indefinitely. Its compensating controls MUST be those specific to the gap — the full verification sequence passing on every pull request, and the dependency, licence, and secret scanning gates — and MUST NOT cite controls that FR-027 already mandates unconditionally, since a baseline requirement compensates for nothing. The approval requirement MUST be raised to at least one as soon as a second maintainer joins, or the waiver renewed with a fresh justification at its expiry date.
- **FR-028**: Every automated workflow MUST declare least-privilege permissions, defaulting to read-only, with each elevated permission scoped to the job requiring it and justified.
- **FR-029**: Every third-party automation step MUST be pinned to an immutable reference rather than a moving tag or branch.
- **FR-030**: The repository MUST be public from creation, and every security control the hosting platform provides to public repositories — at minimum secret scanning with push protection, code scanning, dependency graph and alerts, and dependency review — MUST be enabled and verified working before the first content push. Cost or plan tier is not an accepted reason for omission, because the public tier provides these controls. Should a control become genuinely unavailable through platform change rather than choice, it MUST be recorded as a **control-unavailability waiver** under FR-015 with reason, compensating control, named owner, and expiry date. The expected number of control-unavailability waivers is **zero**; any occurrence is an exception requiring recorded review rather than a routine allowance. This count is distinct from the single **approval waiver** (W-001) tracked by FR-051.
- **FR-031**: Issue, pull-request, security-reporting, and release templates MUST exist and request the information reviewers need.
- **FR-032**: Release tags MUST be signed, and releases MUST run from a protected environment with named approvers.
- **FR-033**: Publishing to external systems MUST use identity-federated short-lived credentials bound to the approved repository, environment, and workflow. Long-lived registry credentials MUST NOT be stored in the repository or its automation.
- **FR-034**: The one-time bootstrap credential required to create a package's first public release MUST be least-scope, separately approved, never committed, revoked immediately after verification, and its revocation recorded.

#### Documentation platform

- **FR-035**: The prose documentation platform MUST be Docusaurus, and its decision record MUST document the evaluation against versioned output, search, link checking, tested snippets, accessible output, reproducible builds, license, and maintenance status.
- **FR-036**: The documentation platform decision record MUST list the alternatives considered and rejected (mdBook, MkDocs with Material, Zola) with the reason each was rejected, the criteria applied, the decision, the accepted consequences including the Node toolchain dependency, and a named owner.
- **FR-037**: A placeholder documentation set MUST build from a clean checkout, and link checking MUST run and report no broken links.
- **FR-054**: The documentation toolchain MUST be version-pinned with its lockfile committed, and its dependencies MUST be subject to the same dependency, license, and advisory policy as the rest of the project.
- **FR-055**: The documented verification sequence MUST state which checks require which toolchain. A contributor missing a required toolchain MUST receive an actionable failure naming what is missing and how to install it, never a silent skip or a false pass.
- **FR-056**: The prose documentation and the generated crate API documentation MUST cross-link and describe the same contract at the same version, so a reader cannot land on prose and reference material that disagree.

#### Release skeleton and evidence

- **FR-038**: A release rehearsal MUST package the placeholder internal package from a clean checkout and MUST perform zero publish operations against any public registry, failing closed if publication is attempted.
- **FR-039**: The packaged file list MUST be inspected and MUST contain only intended files — no secret, local configuration, build output, or unintended asset.
- **FR-040**: Metadata validation MUST confirm that every package intended for publication declares description, license, repository, homepage, documentation, readme, keywords, categories, minimum supported toolchain version, and included files, and that no such package depends on a path-only dependency.
- **FR-041**: Release documentation MUST state the publication order for the intended package set, the requirement to wait for registry availability before dependents, the immutability of published versions, and the yank-and-replace remedy for a defective release.
- **FR-042**: A phase completion record MUST map every Phase 001 acceptance criterion to dated evidence naming the command or action, the platform, the operator, and the result.
- **FR-043**: Known limitations MUST be recorded with a named owner and a target phase.
- **FR-044**: No capability MUST be described as available unless it exists and has recorded evidence; planned capability MUST be labelled as planned.
- **FR-045**: The release path MUST define how checksums, a software bill of materials, build provenance, and artifact attestations are produced and retained for future releases, and the rehearsal MUST record the evidence that would accompany a real release.
- **FR-046**: Verification and rehearsal runs MUST retain their build evidence for a stated period, so a reviewer can later reconstruct what was verified, on which platform, by whom, and when. The periods are stated concretely and are **Renvor policy decisions, not durations mandated by any external authority**: ordinary continuous-integration logs and temporary workflow artifacts are retained **90 days** (the maximum the hosting platform supports for public repositories); phase-completion and release-rehearsal evidence held as tracked governance records is retained for the **lifetime of the project**; binary release evidence is retained until **the later of seven years after publication or three years after that release's supported lifetime ends**; and the compact integrity and provenance records — release manifest, checksums, software bill of materials, attestation and provenance bundle, and signing metadata — are retained for the **lifetime of the project**. Workflow artifacts are evidence *transport*, never the durable archive. The complete policy, including the independent-archive requirement, is `governance/evidence-retention-policy.md`.

#### Exclusions

- **FR-047**: This feature MUST NOT implement runtime framework capability, including the application kernel, lifecycle, configuration, error taxonomy, command-line behavior, project generation, HTTP or GraphQL surfaces, persistence, authentication, frontend or desktop output, or installable-package machinery. Placeholder content exists only to make verification and packaging executable.

### Key Entities

- **Name Availability Record**: The dated evidence set for every public name. Attributes: item, location checked, date checked, observed status, checker, validity window, resulting decision.
- **Decision Record**: A recorded consequential decision. Attributes: identifier, title, state, context, decision, alternatives considered, consequences, reviewer, review date, superseded-by.
- **Support and Version Policy**: What the project commits to supporting. Attributes: minimum supported toolchain version, tested toolchains, supported operating systems, change rules, notice period, effective date.
- **Dependency and License Policy**: The rules governing what may be depended upon. Attributes: permitted licenses, review-required licenses, prohibited licenses, advisory handling, unmaintained-dependency handling, reviewer outcomes.
- **Repository Protection Baseline**: The intended hosting-platform posture. Attributes: protected branch, required reviews, required checks, automation permission defaults, pinning rule, enabled scanning controls, unavailable controls with waivers.
- **Release Rehearsal Evidence**: Proof the release path works without publishing. Attributes: date, operator, platform, packaged artifact, packaged file list, metadata validation result, publish operations performed (must be zero).
- **Phase Completion Record**: The gate into the next phase. Attributes: acceptance criterion, linked evidence, date, operator, result, open blockers, known limitations with owner and target phase.
- **Waiver Record**: A time-bounded, explicit exception. Attributes: violated rule, reason, compensating control, owner, expiry, removal plan.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the public names in the identity contract, plus the hosting organization/repository and documentation domain, have a dated availability entry with a definite status; the hosting organization and repository are under project control; 0 unconfirmed names appear in any frozen public reference; 0 package-registry names are claimed by publication.
- **SC-002**: A contributor starting from a fresh clone on a supported operating system reaches a fully passing verification run by following the documented setup, without needing undocumented steps or personal assistance.
- **SC-003**: The documented verification sequence completes with 0 failing checks and 0 silently skipped checks, on both the declared minimum supported toolchain and the current stable toolchain.
- **SC-004**: After a full verification run on a clean checkout, the working tree reports 0 untracked and 0 modified files.
- **SC-005**: **Both** required secret scans report 0 findings — the pre-creation scan and the pre-push re-scan — each with its own recorded tool version, date, and scope. A single clean scan does not satisfy this criterion, because the earlier scan predates the content the later one authorises.
- **SC-006**: All six governance documents (license, contribution guide, code of conduct, security policy, support policy, governance) are reachable within 1 link from the root landing document, and a security reporter can locate the private reporting path in under 2 minutes.
- **SC-007**: 100% of automated workflows declare read-only default permissions, and 100% of third-party automation steps resolve to immutable references; 0 unwaived exceptions.
- **SC-008**: 100% of the security controls the public repository tier provides are enabled and verified before the first content push. Two waiver counts are tracked separately: **control-unavailability waivers = 0**, and **approval waivers = exactly 1** (W-001, carrying its compensating controls, owner, expiry date, and release condition). A control-unavailability waiver above zero is an exception requiring recorded review, not a policy violation in itself.
- **SC-009**: 0 decision records are marked accepted without a recorded reviewer and review date, and 100% of the decisions required by this feature are accepted before the phase closes.
- **SC-010**: The release rehearsal produces 1 package artifact for the placeholder package and performs 0 publish operations; the public registry shows 0 new versions after the rehearsal.
- **SC-011**: 100% of Phase 001 acceptance criteria in PLAN.md map to dated evidence in the phase completion record; 0 criteria are unevidenced.
- **SC-012**: The placeholder documentation set builds successfully and link checking reports 0 broken links.
- **SC-013**: 0 runtime framework capabilities are implemented in this feature, confirmed by review against the exclusion list.
- **SC-014**: 0 long-lived registry or publishing credentials exist anywhere in the repository or its automation, and 100% of release-identity controls (signed tags, protected release environment with named approvers, provenance and bill-of-materials plan) are either configured or covered by a dated waiver.
- **SC-015**: 100% of packages intended for publication declare `MIT OR Apache-2.0`; both license texts are present in the repository; 0 packages are unlicensed or declare different terms.
- **SC-016**: The declared minimum supported toolchain version reads exactly `1.94.0` in every location that states it — authoritative source, package metadata, and support policy — with 0 mismatches; the root workspace manifest declares resolver 3 explicitly; and minimum-version-aware dependency resolution is demonstrated in effect rather than assumed.

## Out of Scope

- Any runtime framework capability: application kernel, typed state, provider registry, lifecycle, configuration, error taxonomy, health and readiness, or tracing bootstrap (Phase 002).
- The `renover` executable *(renamed `renvor` on 2026-08-17 by ADR-0010)*, the interactive project wizard, templates, and local runtime commands (Phase 003).
- HTTP routing, middleware, validation, problem details, and API description (Phases 004–005).
- Persistence, migrations, authentication, authorization, frontend, desktop, and installable-package machinery (Phases 006 onward).
- Actually publishing any package to a public registry. The release path is rehearsed without publication; the repository itself is public from creation, which is in scope.
- Selecting or pinning the full runtime dependency set. This feature establishes the policy and the toolchain baseline; runtime dependency selection happens in the phase that first needs each dependency.
- Amending the constitution. It is already ratified at version 1.0.0; amendment requires the separate governance process.

## Assumptions

- **Governance already ratified**: The constitution is ratified at version 1.0.0 (2026-08-11). "Ratify governance" in this feature means publishing and making discoverable the surrounding governance, legal, and security documents and confirming they agree with the ratified constitution — not re-ratifying it.
- **Licensing** *(resolved 2026-08-11 — see Clarifications)*: Renvor's source and documentation are licensed `MIT OR Apache-2.0` at the recipient's choice, contributions are accepted under the same dual terms, and generated project output carries no Renvor licensing obligation. Brand assets and the product name are not covered by this grant and are handled separately from the code license.
- **Minimum supported toolchain** *(resolved 2026-08-11 by maintainer decision — see Clarifications)*: Rust **1.94.0**, declared as a fixed support floor rather than a rolling offset. Newer Rust stable releases do not affect it. Raising it requires an accepted decision record naming a concrete forcing requirement, lands only in a planned minor or major release, and is documented in the support policy, changelog, and release notes. Each declared floor holds for at least six months. The policy is reviewed quarterly without the review itself changing the version. The value is revalidated against real persistence dependencies before Phase 006, since PLAN.md's original justification referenced a database stack that does not exist until then.
- **Supported platforms** *(derived from PLAN.md §17.2, not user-selected — confirm during planning)*: Linux is the primary verification platform for this feature; macOS and Windows are added to the matrix where behavior is platform-sensitive. Broader platform commitments follow in later phases.
- **Ownership** *(derived from the PLAN.md document header, not user-selected — confirm during planning)*: The plan owner (Ahmed Anbar) is the initial and sole maintainer, decision authority for accepting decision records, security contact, release approver for the protected release environment, and holder of the one-time registry bootstrap credential, until a governance document records otherwise. This single-person concentration is the reason the branch-protection waiver in FR-051 exists and why it expires on a second maintainer joining.
- **Repository state**: The repository currently has no configured remote and has never been pushed publicly. Acquiring the hosting organization and repository, and configuring their protections, is therefore in scope for this feature.
- **Publication posture** *(resolved 2026-08-11 — see Clarifications)*: The repository is created public and remains public, so every security control the public tier provides is available free from day one. PLAN.md §19.1 is satisfied by gating the first *content push* rather than by keeping the repository private: verification, licensing, and the security contact are completed locally, and the first thing the public sees is already correct. Package publication remains separately gated and does not occur in this phase.
- **Registry availability checks are point-in-time**: Name availability observed on one date can change. This is why entries carry dates and a validity window rather than being treated as permanent.
- **Inherited constraints are product contracts, not spec-invented choices**: The Rust 2024 edition, Cargo resolver 3, and explicit-MSRV requirements come from the ratified constitution and PLAN.md §8. They appear here because they are binding acceptance criteria for this phase.

### Dependencies

- **PLAN.md** (version 1.1.0) — the program execution authority defining Phase 001 deliverables and acceptance criteria; this specification must satisfy every one of them.
- **The ratified constitution** (version 1.0.0) — governs this specification and takes precedence over PLAN.md on conflict.
- **A source-hosting account with organization-creation rights**, needed to acquire the organization and repository and to configure branch protection and scanning controls.
- **A package-registry account**, needed to verify package-name availability and, later, to own the published names.
- **Control of, or the ability to acquire, the documentation domain**, needed before documentation links are published.
- **A supported Rust toolchain installation** on each verification platform, at both the declared minimum supported version and the current stable version.
