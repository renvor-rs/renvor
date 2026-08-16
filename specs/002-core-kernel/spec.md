---
description: "Phase 002 feature specification — transport-independent core kernel, errors, configuration, and lifecycle"
---

# Feature Specification: Transport-Independent Core Kernel

**Feature Branch**: `feat/phase-002-core-kernel`
**Feature Directory**: `specs/002-core-kernel`
**Created**: 2026-08-15
**Status**: Draft
**Depends on**: Phase 001 (complete — merged as `664bc03cd51c876c61a3c5fedba61d6e09a44a85`)
**Input**: PLAN.md §20 Phase 002 — "Core kernel, errors, configuration, and lifecycle"

> **Phase 001 shipped no runtime capability.** This is the phase in which Renvor first becomes
> a thing that runs. Everything here is new surface, and the constitution's rules about
> lifecycle determinism, fail-closed defaults, and package-first boundaries bind it directly.

> **Nothing in this phase is published.** No crate, package, image, release, or tag. The four
> transferred deployment gates from Phase 001 — **T102, T108, T109, T111** — remain
> non-completed and are untouched by this phase.

## Clarifications

### Session 2026-08-15

- Q: Which configuration sources must the kernel read from, and in what file formats? (FR-015) → A: Defaults + environment variables + **TOML** files. No JSON and no YAML layer in this phase.
- Q: What compatibility promise does the kernel's public surface carry when this phase lands? (FR-036) → A: **Explicitly unstable** through Phases 002–003. Breaking changes are allowed until a real transport has exercised the surface; the facade re-exports a deliberately narrow surface. FR-035 is preserved unchanged; recorded as new **FR-036**. *(The "through Phases 002–003" window in this answer was **superseded on 2026-08-16**: the window is now event-gated, because Phase 004 — not Phase 003 — carries the first real transport. The rest of this answer stands.)*
- Q: What should the specification record as the kernel's sensitive-data classification and abuse cases, given it has no network surface and no authentication? (FR-037…FR-041) → A: Scope to reality **plus supply-chain abuse cases**, and **classify opaque typed state as sensitive too**. Two sensitive classes: secret-marked configuration values, and opaque author-registered typed state (which may hold credentials the kernel cannot inspect). Abuse cases: secret leakage via any output path, hostile or malformed configuration input, resource exhaustion at startup or drain, and a malicious or compromised dependency introduced by this phase. **Authorization impact is explicitly none in this phase.**
- Q: How should the drain budget be supplied, and what must a budget of zero mean? (FR-006, FR-007, FR-042) → A: A **documented default of 30 seconds**, author-overridable. **Zero means skip the drain and stop immediately**, still reporting any outstanding work as outstanding — it is not "wait forever" and it is not rejected as invalid.
- Q: What must the kernel emit through tracing, and what identifier ties one application run's records together? (FR-029, FR-043) → A: A **span per lifecycle phase**, **structured fields only**, and a kernel-generated **opaque per-run identifier** on every record. The identifier MUST carry no meaning — no hostname, timestamp, counter, or configuration value — and later phases add request identifiers beneath it.

### Session 2026-08-16

- Q: When the same key is set in defaults, in more than one TOML file, and in an environment variable, which value wins? (FR-015, FR-044) → A: **Environment > later file > earlier file > defaults.** Tables are **deep-merged per key**; **arrays replace wholesale** rather than merging element-wise; and a **type mismatch for the same key across layers fails at Validate** rather than being coerced or resolved by last-wins.
- Q: What concrete limits should apply to the number of providers, the number of dependency edges, and the time spent resolving the dependency graph? (FR-039) → A: **1024 providers, 8192 dependency edges**, and resolution bounded by **visited-edge count rather than wall-clock**. Additionally, dependency resolution **MUST run in O(providers + edges)** — each provider and each edge visited a constant number of times — so the edge bound is a real defect detector rather than a proxy for graph size. *(**Refined by maintainer direction later the same day**: the asymptotic phrasing is **no longer the normative bound**, because it is not directly testable. FR-039 now separates the **declared graph-size ceilings** from a **deterministic work budget** and fixes the constant at **2 examinations per accepted provider and 2 per accepted edge** — 2048, 16384, and 18432 total work units at maximum size. Linearity is now a *consequence* of those numbers rather than a claim standing in for them. The substance of this answer is unchanged.)*
- Q: What event or milestone ends the kernel's explicitly-unstable window, given that the first real transport arrives in Phase 004 rather than Phase 003? (FR-036) → A: **Event-gated, not phase-numbered**: unstable until **the first real transport adapter has exercised the surface and its feedback has been applied**, **and** an **accepted decision record that supersedes ADR-0002** closes the window. *(**Refined by maintainer direction later the same day**, and recorded here rather than silently: the disposition of ADR-0002 is **supersession only**, not "amend or supersede". An amendment would leave an accepted record still partly governing a stability guarantee it no longer fully states — the same two-records-disagreeing defect by another route. The closure event is named **without a phase number**; **Phase 004 is where the current roadmap places the first real transport**, and that is rationale for why the window outlives this phase, not a term of the condition.)*

## User Scenarios & Testing *(mandatory)*

The "user" throughout is **an application author building a service on Renvor** — not an end
user of that service.

### User Story 1 - An application starts, or refuses to, with a reason (Priority: P1)

An author assembles an application from configuration and providers, calls a single entry
point, and either gets a running application or a diagnostic that names what was wrong and
what to do about it. There is no third outcome in which the application runs while something
required is missing.

**Why this priority**: This is the phase's reason to exist. Every other story assumes an
application that boots deterministically. Constitution principle IV makes the lifecycle order
and its failure semantics normative, and principle VI forbids silent fallbacks — a start that
"mostly worked" is the failure this story exists to make impossible.

**Independent Test**: Build an application with a deliberately invalid configuration value and
a deliberately missing provider dependency; confirm each produces a distinct, actionable error
and that no listener, task, or provider is left running afterwards.

**Acceptance Scenarios**:

1. **Given** a valid configuration and a well-formed provider set, **When** the author starts
   the application, **Then** providers initialise in dependency order, the application reaches
   Ready, and the observed phase sequence is exactly `Load → Validate → Register → Boot → Ready`.
2. **Given** a configuration value that fails validation, **When** the author starts the
   application, **Then** startup stops at Validate, no provider is booted, no listener is
   opened, and the error names the offending key, the constraint it violated, and the source
   the value came from.
3. **Given** three providers where the third fails during Boot, **When** the author starts the
   application, **Then** the two already-initialised providers are shut down **in exact reverse
   order**, the failure is reported with the failing provider identified, and the process does
   not reach Ready.
4. **Given** a provider set containing a dependency cycle, **When** the author starts the
   application, **Then** registration fails before any provider is booted and the diagnostic
   names every provider in the cycle.
5. **Given** a provider that depends on a capability nobody registered, **When** the author
   starts the application, **Then** registration fails and the diagnostic names both the
   dependent and the missing dependency.

---

### User Story 2 - An application stops without losing work or hanging (Priority: P2)

An author signals shutdown. The application refuses new work, allows in-flight work a bounded
period to finish, stops providers in reverse order, and reports honestly whether the drain
completed or was cut short.

**Why this priority**: Shutdown correctness is worth less than startup correctness only
because an application must start before it can stop. Constitution principle IV requires
bounded, observable drain; an unbounded drain is a hang, and an unreported forced stop is a
silent data-loss report.

**Independent Test**: Start an application with a task that outlives the drain budget; confirm
the drain deadline is enforced, that the forced stop is reported as such rather than as a clean
shutdown, and that providers still stop in reverse order.

**Acceptance Scenarios**:

1. **Given** a running application with no in-flight work, **When** shutdown is requested,
   **Then** the phase sequence is exactly `Drain → Stop`, providers stop in **reverse actual
   initialisation order**, and the result reports a clean drain.
2. **Given** in-flight work that finishes inside the drain budget, **When** shutdown is
   requested, **Then** that work completes, and only then do providers stop.
3. **Given** in-flight work that exceeds the drain budget, **When** shutdown is requested,
   **Then** the drain ends at its deadline, the result **explicitly reports that work was
   still outstanding**, and shutdown proceeds rather than hanging.
4. **Given** shutdown has begun, **When** new work is submitted, **Then** it is rejected with
   an error that says the application is shutting down — it is not silently dropped and not
   silently accepted.
5. **Given** a provider that fails during Stop, **When** shutdown proceeds, **Then** the
   remaining providers are still stopped, and every failure is reported rather than the first
   one masking the rest.

---

### User Story 3 - Configuration is typed, layered, and explains itself (Priority: P3)

An author declares the shape of their configuration once. Values arrive from built-in defaults,
TOML files, and environment variables; each source is decoded against that declared shape before
the layers are combined in a documented precedence order. When a value is wrong or missing,
the error says which key, which layer it came from, and what was expected.

**Why this priority**: Configuration is the most common startup failure and the most common
source of accidental secret disclosure. It must be settled before anything reads it.

**Independent Test**: Supply the same key from two layers and confirm the documented precedence
wins; supply an out-of-range value and confirm the error names key, constraint, and source
layer; supply a typed field from the environment as text and confirm it decodes, while an
undecodable text for the same field fails at Validate naming key, layer, and expected type.

**Acceptance Scenarios**:

1. **Given** a key defined in defaults and overridden by an environment variable, **When**
   configuration loads, **Then** the environment value wins and the resolved source is
   reportable.
2. **Given** a required key absent from every layer, **When** configuration loads, **Then**
   loading fails naming that key, and the application does not proceed to Register.
3. **Given** a value of the wrong type or outside its allowed range, **When** configuration
   loads, **Then** the error names the key, the expected constraint, and the layer the value
   came from.
4. **Given** a field declared as an integer and supplied only by an environment variable
   carrying the text `8080`, **When** configuration loads, **Then** it decodes to the integer
   `8080` and loading succeeds — decoding text into a declared type is **not** a cross-layer
   conflict.
5. **Given** that same integer field supplied by an environment variable carrying the text
   `eighty-eighty`, **When** configuration loads, **Then** **Validate** fails and the error
   names the key, identifies the environment layer as the source, and states the expected type.
6. **Given** a key that decodes to a table in a TOML file and to a scalar in an environment
   variable, **When** the decoded layers are merged, **Then** **Validate** fails naming the key
   and **both** layers — the shapes are not coerced into one another, and the higher-precedence
   layer does not simply win.
7. **Given** a configuration field marked secret, **When** the configuration is formatted for
   logs, errors, or debug output, **Then** the value is **redacted in every one of those
   paths** and the field name remains visible.

---

### User Story 4 - Failures are diagnosable without leaking secrets (Priority: P4)

An author reads an error and learns what failed, where, and what to try. A reader of the logs
learns the same thing without learning any credential.

**Why this priority**: Constitution principle VI requires redaction and fail-closed handling;
principle IV requires actionable diagnostics. An error taxonomy retrofitted after adapters
exist inherits their shapes instead of governing them.

**Independent Test**: Construct an error carrying a secret-bearing value; confirm the secret
appears in neither the human-readable representation, the diagnostic representation, the causal
chain, nor any structured log or trace field, while the error remains identifiable.

**Acceptance Scenarios**:

1. **Given** an error caused by another error, **When** the author inspects it, **Then** the
   full causal chain is available and each link is attributable.
2. **Given** an error carrying a redacted value, **When** it is displayed, debugged, or
   recorded as a tracing field, **Then** the value never appears in any of them.
3. **Given** any kernel error, **When** the author matches on it, **Then** its category is
   inspectable programmatically rather than only by reading the message text.

---

### User Story 5 - Health and readiness answer different questions (Priority: P5)

An operator asks two distinct questions — "is this process alive?" and "should it receive
work?" — and gets two independent answers.

**Why this priority**: Conflating them causes an unready-but-alive process to be killed, or a
draining process to keep receiving work. Both are outages caused by the primitive, not the
application.

**Independent Test**: Drive the application to a state where it is alive but not ready and
confirm the two answers differ.

**Acceptance Scenarios**:

1. **Given** an application that has not yet reached Ready, **When** both are queried, **Then**
   health may report alive while readiness reports not-ready.
2. **Given** an application that has entered Drain, **When** readiness is queried, **Then** it
   reports not-ready **while health still reports alive**.
3. **Given** a registered readiness contributor that reports unhealthy, **When** readiness is
   queried, **Then** overall readiness is not-ready and the failing contributor is identifiable.

---

### User Story 6 - The kernel is testable without a transport (Priority: P6)

An author writes tests that start a real application, inject a failure at a chosen lifecycle
phase, and assert on the observed order — without an HTTP client, a port, or a database.

**Why this priority**: Constitution principle IX requires real-boundary verification. If the
kernel can only be exercised through an adapter, then every later phase inherits untestable
foundations.

**Independent Test**: Use the harness to force a failure at each lifecycle phase in turn and
assert the resulting rollback order, with no network or filesystem dependency.

**Acceptance Scenarios**:

1. **Given** the test harness, **When** an author injects a failure at a named phase, **Then**
   that phase fails deterministically and the observed rollback is assertable.
2. **Given** a test that needs time to pass, **When** it uses the harness clock, **Then**
   deadlines and drain budgets are exercised without real waiting.
3. **Given** the examples shipped with this phase, **When** they are compiled and run, **Then**
   they use ordinary language constructs, declare their state explicitly, and rely on **no
   global mutable state**.

---

### Edge Cases

- A provider fails during rollback, while the kernel is already rolling back another failure.
- Shutdown is requested **before** the application reaches Ready.
- Shutdown is requested twice concurrently.
- A cancellation signal arrives during Boot rather than during Ready.
- Two providers declare the same state type — a duplicate registration.
- A provider's dependency graph is valid but its declared order conflicts with registration
  order.
- Configuration is valid at Load but the values are mutually inconsistent (each key legal,
  the combination not).
- A drain budget of zero, with work in flight — must stop immediately **and** report the work as outstanding (FR-042).
- A readiness contributor that panics rather than returning an error.
- Tracing initialisation is attempted twice in one process.
- A configuration file is malformed, truncated, or unexpectedly large.
- Registered state carries a credential the author did not mark secret, and an error mentions that state.
- A provider set is large enough to make dependency resolution the dominant startup cost.

## Requirements *(mandatory)*

### Functional Requirements

**Lifecycle**

- **FR-001**: The kernel MUST implement the phases `Load → Validate → Register → Boot → Ready → Drain → Stop` and MUST NOT expose an ordering in which a later phase runs before an earlier one.
- **FR-002**: The observed phase sequence of any run MUST be inspectable by a test without instrumenting the kernel's internals.
- **FR-003**: Required configuration and required dependencies MUST be validated **before** readiness is announced.
- **FR-004**: A failure during Boot MUST shut down every already-initialised provider **in exact reverse initialisation order**, and MUST report the originating failure.
- **FR-005**: A failure **during** rollback MUST NOT abort the remaining rollback; every rollback failure MUST be reported alongside the original failure.
- **FR-006**: Shutdown MUST reject new work, drain within an explicit bound, and stop providers in reverse order. The bound MUST have a **documented default of 30 seconds** and MUST be overridable by the author; see FR-042 for the zero case.
- **FR-007**: A drain that does not complete within its budget MUST be reported as incomplete, naming that outstanding work remained. Reporting it as clean is prohibited.
- **FR-008**: Requesting shutdown more than once MUST be safe and MUST NOT run Stop twice for any provider.
- **FR-009**: Shutdown requested before Ready MUST still roll back whatever was initialised, in reverse order.
- **FR-042**: The drain budget MUST be supplied as an author-overridable value with a **documented default of 30 seconds**. A budget of **zero MUST mean "skip the drain and stop immediately"** — it MUST NOT mean wait indefinitely, and it MUST NOT be rejected as invalid configuration. **A zero budget MUST still report outstanding work honestly**: if work was in flight when shutdown began, the result MUST report it as outstanding exactly as a timed-out drain would, so that choosing an immediate stop never silently reads as a clean one.

**State and providers**

- **FR-010**: The kernel MUST provide typed application state retrievable by type, with a compile-time or startup-time error — never a runtime panic in ordinary use — when a requested type was never registered.
- **FR-011**: Registering two values of the same state type MUST be an error that names the type.
- **FR-012**: Providers MUST declare their dependencies, and the kernel MUST initialise them in dependency order.
- **FR-013**: A dependency cycle MUST be detected before any provider is booted, and the diagnostic MUST name every provider in the cycle.
- **FR-014**: A missing dependency MUST be detected before any provider is booted, naming both the dependent and the missing capability.

**Configuration**

- **FR-015**: Configuration MUST be typed and layered across exactly three source kinds — **built-in defaults**, **TOML files**, and **environment variables** — with the precedence order defined in FR-044. **JSON and YAML sources MUST NOT be supported in this phase**; adding one is a separate reviewed decision.
- **FR-016**: A configuration error MUST identify the key, the violated constraint, and the source layer.
- **FR-017**: Invalid configuration MUST prevent Boot; no listener, task, or provider may start.
- **FR-018**: Fields declared secret MUST be redacted in **every** output form the kernel can produce — the human-readable representation, the diagnostic representation, error output, and structured log and trace fields. The field name MUST remain visible; the value MUST NOT.
- **FR-044**: Configuration resolution MUST proceed as **two distinct steps — decode each source, then merge the decoded layers** — and the requirement separates them because conflating them is what makes the word "coercion" ambiguous.

  **Step 1 — per-source decoding.** Every source MUST be decoded against the **declared typed configuration schema before any precedence merging occurs**. Decoding a **textual environment-variable value into its declared type** — the text `8080` into an integer field, the text `true` into a boolean field — is an ordinary and **permitted** part of this step and **MUST NOT be treated as cross-layer coercion**: the target type comes from the author's own declaration, not from another layer's value, so nothing is being reconciled between layers. An environment value that **cannot** be decoded into its declared type MUST **fail at Validate**, naming the **key**, the **source layer**, and the **expected type**.

  **Step 2 — precedence and merging over already-decoded layers.** Precedence MUST be, from lowest to highest: **built-in defaults**, then **TOML files in the order they were supplied** (a later file overrides an earlier one), then **environment variables**. Merging MUST follow three rules. **(a) Tables merge per key** — a later layer overrides only the keys it sets, leaving sibling keys from lower layers intact, so a small override file need not restate a whole section. **(b) Arrays replace wholesale** — a later layer's array MUST replace the lower layer's array entirely and MUST NOT be concatenated or merged element-wise, because element-wise merging makes it impossible to *remove* an entry. **(c) Incompatible structural shapes for the same key across layers MUST fail at Validate** — a key that decoded to a table in one layer and to an array or a scalar in another MUST be reported, naming the key and both layers. Such a conflict MUST NOT be coerced into a common shape and MUST NOT be resolved by last-wins.

  The resolved source layer for every key MUST remain reportable (FR-016).

**Errors**

- **FR-019**: Kernel errors MUST expose a programmatically inspectable category, not only a message.
- **FR-020**: Kernel errors MUST preserve the causal chain.
- **FR-021**: Every error output form MUST redact secret-bearing values; no error path MUST emit one.
- **FR-022**: Silent fallbacks are prohibited. A required capability that is unavailable MUST fail the operation rather than degrade it.

**Cancellation and deadlines**

- **FR-023**: The kernel MUST provide a cancellation signal that propagates to running work.
- **FR-024**: Cancellation arriving during any phase MUST be handled without leaving a provider half-initialised.
- **FR-025**: Deadlines MUST be explicit and bounded; an unbounded wait is prohibited in any kernel-owned path.

**Health, readiness, and tracing**

- **FR-026**: Health and readiness MUST be independently queryable and MUST be able to disagree.
- **FR-027**: Entering Drain MUST make readiness report not-ready while health continues to report alive.
- **FR-028**: A failing readiness contributor MUST be individually identifiable.
- **FR-029**: Tracing initialisation MUST be explicit, MUST be safe to attempt more than once in a process, and MUST NOT be performed implicitly as a side effect of building an application.
- **FR-043**: The kernel MUST emit **one span per lifecycle phase**, so that FR-002's observed phase sequence is assertable from the telemetry itself rather than only through a test-only channel. All emitted data MUST use **structured fields** — never interpolated message strings — and every record MUST carry a kernel-generated **run identifier** that is the same for the lifetime of one application run. **The run identifier MUST be opaque**: it MUST NOT encode a hostname, a timestamp, a process identifier, a counter, or any configuration value, because an identifier that encodes facts becomes an unreviewed disclosure channel. Opacity MUST be guaranteed **by construction**: identifiers MUST be produced at **exactly one generation site**, from **cryptographically secure random bytes and no other input**, and the production wiring of that site MUST use the **operating-system CSPRNG**. Verification of this guarantee MUST be deterministic rather than statistical (SC-019). Later phases MAY nest their own identifiers beneath it; this phase MUST NOT define request-level identifiers. **Redaction under FR-018 and FR-037 applies to every emitted field without exception.**

**Testing and examples**

- **FR-030**: A test harness MUST allow failure injection at each named lifecycle phase.
- **FR-031**: The harness MUST allow deadline and drain behaviour to be exercised without real elapsed time.
- **FR-032**: Examples MUST compile, MUST run, and MUST use ordinary language constructs with **no hidden global mutable state** — no ambient singleton, no implicit initialisation, and no state reachable without being declared.

**Scope discipline**

- **FR-033**: This phase MUST NOT implement HTTP, GraphQL, persistence, authentication, CLI, project generation, frontend, or desktop capability.
- **FR-034**: This phase MUST NOT publish any crate, package, image, release, or tag.
- **FR-035**: Any custom infrastructure chosen over a maintained ecosystem package MUST be justified by an accepted ADR recording the packages evaluated, their concrete shortcomings, the ownership cost, and an exit strategy.
- **FR-036**: The kernel's public surface MUST be declared **explicitly unstable**, and that instability MUST be stated in the surface's own published documentation rather than only in this specification. Breaking changes MUST be permitted without a compatibility procedure while the window is open. **The window is closed by an event, not by a phase number**, and closing it requires **both** of the following, with **no phase number forming part of either condition**: **(a)** **the first real transport adapter has exercised the surface and its feedback has been applied**; and **(b)** an **accepted decision record that supersedes ADR-0002** closes the window explicitly. **Supersession, not amendment**: ADR-0002 is the accepted record governing facade stability, and amending it would leave an accepted record still partly governing a guarantee it no longer fully states — reproducing the two-records-disagreeing defect the requirement exists to prevent. A superseding record replaces the governing statement outright and leaves exactly one authority. *(Roadmap rationale, not part of the condition: the current roadmap places the first real transport in **Phase 004**, which is why the window is expected to outlive this phase. An earlier draft made the closure condition itself phase-numbered and ended it at Phase 003, which was self-contradictory — it justified instability as lasting until a real transport had exercised the surface while freezing the surface one phase before that transport existed. An event-named gate also survives roadmap renumbering; a phase-numbered one does not.)* The facade MUST re-export a **deliberately narrow** surface — it MUST NOT re-export an item merely because that item is public in an implementation crate — so that later narrowing is possible without a breaking change. **No semantic-versioning compatibility promise MUST be made or implied for this surface in this phase.**

**Sensitive data, abuse cases, and resource bounds** *(required of every specification by the constitution's Security and Privacy Requirements)*

- **FR-037**: The specification recognises exactly **two classes of sensitive data**, and the kernel MUST treat both as unprintable. **(a) Secret-marked configuration values** — declared by the author, redacted per FR-018. **(b) Opaque author-registered typed state** — values the kernel stores and returns but **cannot inspect**, which may carry credentials, tokens, or connection strings. The kernel MUST NOT emit the contents of registered state in any error, log, trace, or diagnostic output; it MAY emit the state's **type name** only. **The kernel MUST NOT assume opaque state is safe to print merely because it was not marked secret.**
- **FR-038**: Hostile or malformed configuration input MUST fail closed. A malformed TOML file, a value of the wrong shape, or an unexpectedly large input MUST produce a bounded, actionable error and MUST NOT start the application, panic the process, or consume unbounded memory or time while parsing.
- **FR-039**: Startup and shutdown MUST be resource-bounded by **concrete numeric ceilings** rather than an unquantified promise. The requirement distinguishes **two separate families of bound**, because a single undifferentiated "resource limit" cannot say whether a rejection means *the author's graph is too big* or *resolution misbehaved*.

  **(a) Declared graph-size ceilings — evaluated at Register, before any traversal begins.** At most **1024 registered providers** and at most **8192 declared dependency edges**. A provider set exceeding either ceiling MUST fail at **Register** with a diagnostic naming the ceiling and the observed count, and MUST be rejected **on the declared counts alone** — it MUST NOT be discovered by running out of traversal budget. A valid acyclic graph **at both ceilings simultaneously — 1024 providers and 8192 edges — MUST succeed.**

  **(b) Deterministic resolution work budget — counted during traversal.** Dependency resolution MUST complete within **at most 2 provider examinations per accepted provider** and **at most 2 edge examinations per accepted edge**. At the maximum accepted graph size this is **at most 2048 provider examinations**, **at most 16384 edge examinations**, and **at most 18432 total work units**. The budget MUST be enforced by **counting examinations, never by a wall-clock deadline**, so the bound is deterministic and identical on a loaded machine, a fast machine, and under a debugger. *(A wall-clock deadline would make the bound a property of the host rather than of the graph — flaky under CI load, silently passing on fast hardware.)* These constant per-item factors are what make resolution linear in providers plus edges; the numbers, not the asymptotic phrasing, are the normative bound.

  **(c) Budget exhaustion is a defect signal, never a diagnostic path.** Every graph within the size ceilings — **cyclic or acyclic** — MUST reach its verdict inside the work budget. **A dependency cycle within the ceilings MUST fail with the cycle diagnostic of FR-013, naming every provider in the cycle, and MUST NOT be reported by exhausting the work budget.** Exhausting the budget therefore indicates a **defect in resolution itself** and MUST be reported as an internal error, distinct from every author-facing diagnostic, so that a resolution bug can never be presented to an author as a large or malformed graph.

  The drain period is bounded by FR-042, and **no kernel-owned path may retry without a bound**.
- **FR-040**: Every external dependency introduced by this phase MUST be recorded with its version, licence, maintenance status, MSRV compatibility, and known advisories before it is adopted; dependency versions MUST be resolvable from a committed lockfile rather than floating. **A compromised or malicious dependency in the configuration, tracing, or error-handling path is an in-scope abuse case**, and its mitigation is the recorded evaluation plus the project's existing advisory and licence gates — **not** an assumption that a popular package is safe.
- **FR-041**: **Authorization impact in this phase is none.** The kernel implements no authentication, no authorization, and no identity. This MUST be stated rather than omitted, so that a later phase adding either cannot mistake silence for a completed analysis.

### Key Entities

- **Application** — the assembled, runnable unit; owns the lifecycle and the state.
- **Application Builder** — the assembly surface; consumes configuration and providers and produces an Application or a diagnostic.
- **Provider** — a unit of capability with declared dependencies and initialise/stop behaviour.
- **Provider Registry** — the ordered, cycle-checked set of providers.
- **Typed State** — values retrievable by type, registered exactly once each. **Sensitive: opaque.** The kernel cannot inspect them and MUST NOT print their contents (FR-037b).
- **Configuration** — a typed value resolved from ordered layers (defaults, TOML files, environment), carrying source attribution and secret markings. **Sensitive: secret-marked fields** (FR-037a).
- **Lifecycle Phase** — one of the seven named phases; observable and assertable.
- **Cancellation Token** — the propagating stop signal.
- **Kernel Error** — a categorised, chainable, redaction-safe failure.
- **Health Probe / Readiness Probe** — two independent answers about process state.
- **Run Identifier** — an opaque, kernel-generated token identifying one application run; attached to every emitted record. Encodes nothing (FR-043).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The lifecycle order `Load → Validate → Register → Boot → Ready → Drain → Stop` is asserted by an automated test, with **0** runs observing a different order.
- **SC-002**: For a partial-start failure at position *n* of *k* providers, the observed shutdown order is the exact reverse of the initialisation order in **100%** of runs.
- **SC-003**: **0** configurations that fail validation result in a started listener, task, or provider.
- **SC-004**: Duplicate state registration and missing state retrieval each produce a distinct, named error; **0** cases produce a panic in ordinary use.
- **SC-005**: Dependency cycles and missing dependencies are each detected before any provider is booted, with the diagnostic naming every implicated provider; **0** cases reach Boot.
- **SC-006**: A drain that exceeds its budget is reported as incomplete in **100%** of runs; **0** are reported as clean. The same holds for a **zero** budget with work in flight: **0** such runs report a clean drain.
- **SC-007**: **0** occurrences of a secret-marked value in any output form the kernel can produce, asserted by a test that fails if a secret appears in any of them.
- **SC-008**: Health and readiness disagree in at least **1** asserted state, proving they are independent.
- **SC-009**: Failure can be injected at **7 of 7** lifecycle phases, and **100%** of those injections are covered by a test.
- **SC-010**: **0** runtime framework capabilities outside this phase's declared scope are implemented, confirmed by review against FR-033.
- **SC-011**: **0** crates, packages, images, releases, or tags are published during this phase, confirmed against the registry with a positive control proving the check can detect a published artifact.
- **SC-012**: **100%** of selected external packages carry a recorded evaluation covering version, maintenance status, licence, MSRV compatibility with 1.94.0, advisories, and feature cost.
- **SC-013**: The full verification sequence passes with **0** failing and **0** silently skipped checks on **both** the project's declared minimum supported toolchain and the current stable toolchain.
- **SC-014**: Every example compiles, runs, and uses **no global mutable state**; **0** examples require a transport, a port, or a database.
- **SC-015**: **0** unbounded waits exist in kernel-owned paths.
- **SC-016**: **0** occurrences of registered-state contents in any error, log, trace, or diagnostic output, asserted by a test that registers a credential-bearing value **without** marking it secret and fails if its contents appear.
- **SC-017**: **100%** of external dependencies introduced by this phase carry a recorded version, licence, maintenance status, MSRV compatibility, and advisory check before adoption; **0** floating versions.
- **SC-018**: **7 of 7** lifecycle phases emit a span, and **100%** of emitted records carry the run identifier; **0** emitted records use an interpolated message string in place of structured fields.
- **SC-019**: The run identifier's opacity is established by **construction** and by a **deterministic** acceptance test — never by a probabilistic sample, because a probabilistic assertion can fail on a correct implementation and therefore cannot gate a release.
  **(a) Construction.** Identifiers are produced at **exactly 1** generation site, from **cryptographically secure random bytes only**; **0** of that site's inputs derive from the host, the clock, the process, a counter, or any configuration value. Verified by review of that single site, which is the only place identifiers are produced.
  **(b) Deterministic acceptance — release-gating.** With a **controlled internal entropy source** supplying a fixed byte sequence, the encoded identifier is a **pure function of exactly those bytes**: the same bytes yield the identical identifier in **100%** of runs, and **0** bytes of the identifier change when the hostname, the system clock, the process identifier, an invocation counter, and the entire configuration are all varied while the supplied entropy is held fixed. This proves that **only supplied entropy reaches identifier encoding**, and it contains **0** probabilistic assertions.
  **(c) Production wiring — release-gating.** **1 of 1** production entropy sources is the **operating-system CSPRNG**, verified by review of the production constructor and by a test asserting that constructor accepts **0** inputs other than that source.
  **(d) Non-gating smoke check.** Any random-sample check of collisions or ordering across generated identifiers is **explicitly labelled non-gating**, and **0** release gates depend on its outcome. *(Removed from the gating set deliberately. A 1000-sample collision or monotonicity assertion is a probabilistic statement about a random source, so it can fail on a correct implementation; a gate that fails by chance teaches a team to re-run gates, which costs more than the check is worth. Note also what no test can do: (b) and (c) establish that nothing but entropy is encoded — they cannot prove non-recoverability, and no black-box test can.)*
- **SC-020**: Configuration decoding and precedence are asserted **separately** and end to end.
  **Decoding.** **100%** of sources are decoded against the declared schema **before** any merge occurs; an environment variable supplying the text `8080` for an integer field and `true` for a boolean field decodes successfully in **100%** of runs and produces **0** cross-layer conflicts; an environment value that cannot decode fails at **Validate** in **100%** of runs, with the error naming **3 of 3** required elements — key, source layer, and expected type.
  **Precedence.** For a key present in **all** layers the environment value wins in **100%** of runs; a later file overrides an earlier one in **100%** of runs; table merges preserve **100%** of sibling keys from lower layers; an array in a higher layer **replaces** rather than extends, with **0** concatenations.
  **Shape conflict.** A key decoding to incompatible structural shapes across layers fails at **Validate** in **100%** of runs, naming both layers, with **0** coercions and **0** last-wins resolutions.
- **SC-021**: Every provider-graph bound is asserted numerically, with graph size and traversal work measured **separately**.
  **Graph size.** An acyclic graph of **1024** providers and **8192** edges succeeds in **100%** of runs; **1025** providers fails at Register naming ceiling **1024** and observed count **1025**; **8193** edges fails at Register naming ceiling **8192** and observed count **8193**; **0** oversize graphs reach traversal.
  **Work budget.** For the maximum accepted graph the observed counters are **≤ 2048** provider examinations, **≤ 16384** edge examinations, and **≤ 18432** total work units, with **0** runs exceeding any of the three; and across at least **3** graph sizes the observed counters stay within **2 × providers** and **2 × edges**, with **0** violations.
  **Cycle handling.** A cycle inside both ceilings fails with the FR-013 cycle diagnostic naming **100%** of the providers in the cycle, in **100%** of runs; **0** such runs report budget exhaustion in place of the cycle diagnostic.
  **0** of these assertions depend on elapsed wall-clock time.
- **SC-022**: The instability window's closure conditions are stated as an **event** plus an accepted **superseding** decision record. Within the normative closure clause of FR-036, **0** phase numbers appear — **2 of 2** phase numbers in FR-036 sit inside the parenthetical explicitly marked as roadmap rationale — and **exactly 1** disposition of ADR-0002 is named: **supersession**. The closure event is stated as one **byte-identical sentence** in **3 of 3** places that state it normatively — the 2026-08-16 clarification record, FR-036, and the Dependencies section — with **0** wording variations between them; this criterion is the fourth location and restates both conditions without introducing a variant sentence. Counting occurrences **outside this criterion**, **5 of 5** uses of "amend"/"amendment" sit inside a clause that rejects amendment, and **0** assert it as the required disposition. Across the specification, **0** documents claim a semantic-versioning promise for this surface while the window is open.

## Assumptions

- The application author uses an async Rust runtime; the kernel does not implement one (constitution principle III forbids creating a custom runtime).
- "Reverse order" means reverse of **actual initialisation** order, which dependency resolution may reorder relative to registration order. Tests assert against observed initialisation order, not declaration order.
- The four transferred Phase 001 deployment gates (T102, T108, T109, T111) remain non-completed throughout and are out of scope here.
- Phase 001's waiver **W-003** remains active: this phase's reviews are advisory and non-independent, and **Phase 001 still requires genuine independent human re-review before any public release**.

## Out of Scope

HTTP, Axum, and Tower adapters; persistence, SQLx, and SeaORM; the `renover` CLI and project
generation; authentication and authorization enforcement; frontend and desktop output;
deployment of any kind; publication of any crate, package, image, release, or tag; and any
change to the four transferred deployment gates.

### Dependencies

- Phase 001, complete and merged.
- ADR-0002 (workspace boundaries and facade stability) — implementation crates sit behind the
  `renvor` facade, which re-exports and does not implement.
- **ADR-0002 will require supersession — not amendment** — before the API-instability window can close (FR-036). An accepted **superseding** record is condition (b) of that closure; condition (a) is that **the first real transport adapter has exercised the surface and its feedback has been applied**. **Neither condition names a phase number.** This phase does **not** supersede ADR-0002; it records the obligation so the later change cannot be made silently.
- ADR-0003 (MSRV, toolchain, and dependency policy) — MSRV 1.94.0, with limitation **R-7**
  noting the floor has never been validated against real dependencies. **This phase is the
  first that adds any, so R-7 becomes live here.**
