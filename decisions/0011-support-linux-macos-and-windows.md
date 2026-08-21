# ADR-0011: Support Linux, macOS, and Windows with explicitly different enforcement levels

| Field | Value |
|---|---|
| **ID** | 0011 |
| **State** | `proposed` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | — |
| **Supersedes** | **ADR-0003** *(on acceptance — see §Supersession)* |
| **Owner** | Ahmed Anbar |

> **This record is `proposed`. It is not authoritative and nothing below is in force yet.**
>
> When it is accepted it will be accepted under waiver **W-002**, and **that review will not be
> independent**. Spec FR-013 requires a recorded **independent** review before acceptance.
> `GOVERNANCE.md` and [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11 define a qualified independent reviewer as a
> **person**, **not the author**, **competent in the subject**, and **able to reject without the
> author's consent**. This project has one maintainer, who wrote this record, so criteria 1, 2,
> and 4 cannot be met by anyone currently available. That is a staffing fact, not a process
> defect, and W-002 is the recorded exception covering it.
>
> **No independent human review of ADR-0011 has occurred.** This review must not be described as
> independent — here, in the evidence packs, in `GOVERNANCE.md`, or in any public document.
>
> **No new waiver is created by this record.** The scope analysis is in §Waiver authority below,
> stated explicitly rather than assumed, because a waiver applied one record wider than it was
> granted is a governance failure dressed as a formality.

## Context

**The maintainer decided on 2026-08-21 to keep macOS and Windows as supported platforms.**
That is a human maintainer decision. It is **not** an independent review and does not
discharge W-008, W-005, or W-003.

The decision needs a record because the repository currently says two different things.

**What the evidence says.** Six platform/toolchain contexts have run on every pull request
since T150: `verify (1.94.0)` and `verify (stable)` on `ubuntu-latest`, and four
`platform (…)` contexts across `macos-latest` and `windows-latest` on both toolchains. All
six pass at the current head.

**What the versioned contract says.** `contracts/support-policy.md` 1.0.0 still carries the
Phase 001 table, in which macOS and Windows are *"Not yet claimed"* for the reason *"No
platform-sensitive code exists to verify"*. That reason stopped being true during Phase 002.
The configuration layer resolves filesystem paths, refuses non-regular files **by type from an
open descriptor**, opens files with a platform-specific flag (`O_NONBLOCK` on unix), and reads
`OsString` environment names that are arbitrary bytes on unix and WTF-8 on Windows. Those are
precisely the parts most likely to differ between platforms, and until T150 they were verified
on exactly one.

**What the human-facing documents say.** `SUPPORT.md` and `docs/docs/support-policy.mdx`
already list all three platforms as supported, and already state that only the two Linux
contexts are branch-protection-required. So the public documentation moved and the versioned
contract did not.

That divergence is the problem this record fixes, and it cannot be fixed by editing the
contract directly: `contracts/support-policy.md` §Change control says it *"changes only through
a superseding ADR with an impact analysis"*, and the record that set it is **ADR-0003**. A
superseding ADR is therefore the instrument the contract itself names, and this is that
instrument.

**A second, narrower force.** The documents currently resolve their disagreement in three
different directions — `docs/docs/support-policy.mdx` points at `SUPPORT.md` as *"the current
public platform policy"*, `governance/phase-003-evidence.md` says the same, and
`SUPPORT.md` closes by describing **itself** as a contract that changes only through a
superseding decision record. Two documents both claiming to be the sole authority is worse
than either one being wrong, because a reader who checks one and stops has no way to know
they picked the non-authoritative copy.

## Decision

**Linux, macOS, and Windows are supported platforms.** Each carries passing evidence at the
exact head being claimed, and the enforcement level of each claim is stated rather than left
to be inferred from the presence of a CI job.

### The six contexts

Six platform/toolchain contexts run on **every** pull request:

| # | Context | Platform | Toolchain |
|---|---|---|---|
| 1 | `verify (1.94.0)` | Linux (`ubuntu-latest`) | pinned MSRV 1.94.0 |
| 2 | `verify (stable)` | Linux (`ubuntu-latest`) | current stable channel |
| 3 | `platform (macos-latest, 1.94.0)` | macOS | pinned MSRV 1.94.0 |
| 4 | `platform (macos-latest, stable)` | macOS | current stable channel |
| 5 | `platform (windows-latest, 1.94.0)` | Windows | pinned MSRV 1.94.0 |
| 6 | `platform (windows-latest, stable)` | Windows | current stable channel |

The stable channel is **whatever the stable channel resolves to when CI runs**. This record
does not name a stable version number, because a number written here would be stale on a
schedule nobody controls and would then be a false claim sitting in an accepted record.

### Required is not the same as running

**Only contexts 1 and 2 are required by branch protection.** `main`'s required-status-check
list is exactly:

```
verify (1.94.0)   verify (stable)   security   docs
```

`security` and `docs` are required and are **not** platform contexts. The four `platform (…)`
contexts are **not** in that list. They run on every pull request, their failure is visible,
and a maintainer is expected to act on it — but branch protection alone would not block a
merge on them. **The macOS and Windows claims therefore rest on review practice, not on an
enforced gate**, and every public document that states the claim must also state that.

Making them required is a repository-settings change. It is deliberately **not** part of this
record; see §Alternatives, option 2.

### What each job actually runs

| Job | Runs |
|---|---|
| `verify` (Linux) | The **complete** verification sequence defined by `contracts/verification-sequence.md` — all eleven steps |
| `platform` (macOS, Windows) | `cargo test --workspace --all-features -- --test-threads=1` and `cargo check -p renvor --no-default-features --all-targets` — **and nothing else** |

`platform` is a **separate job from `verify` on purpose.** Adding an `os` dimension to
`verify`'s matrix would have renamed its contexts to `verify (ubuntu-latest, 1.94.0)` and
silently emptied the branch-protection rule, which matches contexts by name.

The platform jobs deliberately omit gitleaks, lychee, the commit-history scan, and the
documentation build. Those are properties of the **repository**, not of the platform; running
a link check against github.com three times learns nothing three times.

### What a support claim means, and what it does not

**A supported-platform claim requires passing evidence at the exact head being claimed.** Not
at an earlier head, not on a branch, not "it worked last week".

**Support does not imply that every platform-specific behaviour has received independent
human review.** No behaviour in this repository has. W-003, W-005, and W-008 are open for
exactly that reason, and this record does not narrow, discharge, or weaken any of them.

### Known evidence limitations, which remain visible

These are carried forward from `governance/phase-003-independent-review-packet.md` and
`SUPPORT.md` rather than dropped now that the platforms are claimed:

- **Two behaviours are `#[cfg(unix)]`-gated** and therefore exercised on Linux and macOS only:
  the FIFO refusal, and the test that drives the non-Unicode environment-name path. The FIFO
  case cannot arise on Windows in that form. The **non-Unicode name** case *can* — a Windows
  environment name is WTF-8 and may contain unpaired surrogates — so what is unix-gated is the
  **test**, not the code path. The bound is a statement about reading, not about measurement,
  and is recorded as the narrower claim it is.
- **`a_destination_whose_state_cannot_be_established_fails_closed` is `#[cfg(unix)]`**, so the
  fail-closed destination check has no Windows-specific test.
- **Windows has had no adversarial review at all.** Every advisory review of Phase 003 ran on
  macOS. CI exercises Windows and is green, but CI runs the tests the *author* wrote; it cannot
  notice a test nobody wrote.
- Several path rules in `crates/renvor-cli/src/paths.rs` — reserved device names, trailing dot
  or space — are enforced on **every** platform but were **reasoned from Windows behaviour that
  was never observed on Windows**.

Claiming Windows as supported while these gaps are open is a deliberate, recorded trade: the
tests that exist pass there on both toolchains, and that is what the claim asserts. It asserts
nothing more.

### Supersession

**On acceptance, this record supersedes ADR-0003**, and ADR-0003's `Superseded by` field is set
to ADR-0011. **ADR-0003's decision body is preserved unchanged.** Superseding a record does not
entitle anyone to rewrite what it said; the Phase 001 evidence that cites it must remain
checkable.

Everything still valid in ADR-0003 is carried into this record rather than left to be inherited
by implication:

| Carried forward from ADR-0003 | Status here |
|---|---|
| **MSRV is Rust 1.94.0, a fixed floor**, not N-3, N-4, or any offset from current stable | **Unchanged.** This record does **not** raise, lower, or reinterpret the MSRV |
| A new Rust stable release does not invalidate, shorten, or trigger review of the MSRV | **Unchanged** |
| Declared once at `[workspace.package] rust-version`; members inherit; no second declaration | **Unchanged** |
| `clippy.toml` `msrv = "1.94.0"`; `rust-toolchain.toml` pins the channel with explicit `rustfmt` and `clippy` components | **Unchanged** |
| **Current stable channel is also tested**, on a job that moves while the MSRV job stays pinned | **Unchanged**, and now stated without a version number |
| **Rust 2024 edition**, declared explicitly on every package | **Unchanged** |
| **Cargo resolver 3**, declared explicitly in the virtual workspace | **Unchanged** |
| The five rules for raising the MSRV — planned minor/major only, accepted decision record, documented in three places, six-month dwell time, passing run at the exact version | **Unchanged** |
| Quarterly policy review that records an outcome and by itself changes nothing | **Unchanged** |
| **Scheduled Phase 006 MSRV reassessment** against the actual persistence dependencies (FR-061), owner Ahmed Anbar | **Unchanged and still owed** |
| Dependency and lockfile rules; `deny.toml` authoritative for licences and sources; wildcards denied; Dependabot across `cargo`, `github-actions`, `npm` | **Unchanged** |
| Security-advisory response windows, incorporated by reference to `governance/dependency-advisory-policy.md` | **Unchanged**, and still incorporated by reference rather than by copy |

**What ADR-0003 said about platforms was one row of a Phase 001 table and is the only thing
this record changes.** ADR-0003 is superseded rather than amended because the contract it set
admits no other mechanism.

### One authority

**`contracts/support-policy.md` is the sole normative current authority** for supported
toolchains, supported platforms, the MSRV floor, and the rules for changing them.

- `SUPPORT.md` is the **human-facing summary** and links to the contract.
- `docs/docs/support-policy.mdx` is a **summary** and links to the contract.
- Every other tracked mention is a pointer or a historical record.
- **Any disagreement resolves in favour of `contracts/support-policy.md`.**

Historical evidence stays historical. `governance/phase-001-evidence.md` and
`governance/phase-002-evidence.md` describe what was true in those phases and are **not**
rewritten to match the current claim.

## Waiver authority

**This record is covered by W-002, and by nothing else.** The reasoning is set out rather than
assumed.

W-002's live scope, read from `governance/waivers.md`:

| Where | What it says |
|---|---|
| Waiver row, Reason | *"no genuinely independent review of a **Phase 001 decision record** is available"* |
| Axis table | `W-002 \| decision record (FR-013) \| **Phase 001**` |
| Ruling of 2026-08-11 | *"The reviewer field of **every Phase 001 decision record**…"* |
| W-004's own row | *"**W-002 covers Phase 001 decision records only** and does not reach a Phase 002 ADR"* |

The question is therefore exactly one thing: **is ADR-0011 a Phase 001 decision record?**

It is, and the Phase 001 contract says so itself. `contracts/support-policy.md` identifies
itself as **"Feature: Phase 001 — governance foundation | Satisfies: FR-017 – FR-021 | Set by:
ADR-0003"**, and closes with:

> *"This contract changes only through a **superseding ADR** with an impact analysis covering
> published packages, documentation, the compatibility matrix, and any downstream consumer
> relying on the current promise."*

**A superseding ADR is the instrument the Phase 001 contract names for this purpose.** This
record is that instrument, amending that contract, in the FR-013 domain, superseding a Phase 001
record, and deciding nothing outside the Phase 001 support contract. Its phase attribution
follows its subject matter — which is how the ledger's own axis table attributes every other
record — and not the calendar date on which it was typed.

**This is the same argument ADR-0010 made, on the same contract clause pattern, and it was
accepted.** ADR-0010 changed `contracts/public-identity.md`, a Phase 001 contract with an
identically worded superseding-ADR clause, and was covered by W-002 without a new waiver.
ADR-0006 is the earlier precedent: a Phase 001 record accepted under W-002 on **2026-08-15**,
days after Phase 001's implementation work had finished.

**The counter-argument, stated so it is on the record rather than omitted.** One could read
"Phase 001 decision record" **temporally**, as *"a record written during Phase 001"*. On that
reading ADR-0011 falls outside W-002 and would need a new waiver — W-009.

That reading is **rejected**, for four reasons:

1. It would also have excluded **ADR-0006** and **ADR-0010**, both of which were written after
   Phase 001's implementation finished and both of which were accepted under W-002.
2. It makes every Phase 001 contract's own *"superseding ADR"* clause **unusable** without a
   fresh waiver each time — a contract would name a change mechanism that governance forbids
   anyone to use.
3. The ledger scopes waivers by **subject and phase**, not by authorship date. Its axis table
   has a "Phase" column populated from what each record decides, not from when it was typed.
4. It would inflate the waiver count with an entry that adds no new control, no new owner, and
   no new expiry — W-009 would restate W-002 verbatim against a different record number.

**The reading is recorded, not hidden, so a future independent reviewer can overturn it if they
disagree.** If they do, the remedy is a waiver granted then, not an acceptance defended now.

**No other waiver confers authority here, and none is borrowed:**

| Waiver | Why it confers nothing here |
|---|---|
| **W-003** | Phase-level (`PLAN.md` §6.1 step 10), Phase 001. **A phase-level waiver does not authorise accepting a decision record** |
| **W-004** | Scoped to **ADR-0007 alone**, Phase 002 |
| **W-005** | Phase-level, Phase 002. Same axis objection as W-003 |
| **W-006** | Scoped to **ADR-0009 alone**, Phase 002 |
| **W-007** | **Retired as a burned identifier** and must not be used. It appears fifteen times in this repository, every one asserting that it does not exist |
| **W-008** | Phase-level, Phase 003. Same axis objection as W-003 |

**No new waiver is created. The active waiver set is unchanged by this record**, the
approval-waiver count stays exactly **1** (W-001), and the control-unavailability count stays
**0**. W-002's four compensating controls apply unchanged, and the acceptance gate below records
each against measured evidence rather than intent.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **1. Linux supported; macOS and Windows tested but unclaimed** — keep the Phase 001 table, keep running the four jobs | This is the status quo, and it is **the option that produces the current contradiction**. Four jobs already run on every pull request and already gate nothing but review practice; withholding the claim does not make them cheaper, does not make the code more portable, and does not reduce any obligation — it only means users cannot rely on evidence the project is already producing and paying for. It also leaves `SUPPORT.md` and the versioned contract asserting opposite things, which is worse than either being wrong alone. The Phase 001 *reason* for withholding — *"no platform-sensitive code exists to verify"* — has been false since Phase 002. |
| **2. Support all three and make all six contexts required immediately** | The right end state, and **not this change**. Making a context required is a **repository-settings change**, and this workflow is explicitly barred from changing repository settings. More importantly, requiring a context is a promise about *merge blocking* that should be made deliberately and verified live, not bundled into a documentation record — the same reasoning that kept `platform` out of `verify`'s matrix. Recorded as the intended direction, with the honest cost stated in §Consequences: until it happens, four of the six contexts are enforced by review practice. |
| **3. Keep the current contradictory documents** — leave the contract at its Phase 001 table and let readers work it out from the pointers | A reader who opens the **versioned normative contract** — the document the repository tells them is authoritative — is told macOS and Windows are *"not yet claimed"*, which is false. Resolving that by adding more pointers has already been tried: three documents currently carry a parenthetical explaining that the contract's table is not current. **A contract that needs a footnote in three other files to stop misleading people is a contract that is wrong.** Constitution principle XII forbids leaving a known-false statement standing because correcting it is procedurally inconvenient. |
| **4. Remove the macOS and Windows CI jobs** — narrow the promise to what is enforced | Deletes real evidence to resolve a documentation inconsistency. The four jobs have found platform behaviour worth knowing about, and the code they exercise — path resolution, file-type refusal, `OsString` handling — is exactly the code most likely to differ. Removing them would return the project to verifying platform-sensitive behaviour on one platform, which is the defect T150 was created to fix. It also lowers a public promise for no user's benefit. |

## Consequences

**Positive:**

- **The public promise matches the evidence that is already running.** Six contexts have been
  passing on every pull request; the documents now say so, once, in the place that governs.
- **Users get a clear three-platform statement** with the enforcement level of each claim
  attached, rather than a table they must cross-check against a workflow file.
- **The current documents and the versioned contract converge.** One normative authority, two
  summaries that link to it, and a stated rule that disagreements resolve in the contract's
  favour.

**Negative — accepted costs:**

- **macOS and Windows become lasting compatibility commitments.** A platform claim is a promise.
  Withdrawing one later requires governance — a superseding ADR and an impact analysis — not a
  quiet edit, which is the point of putting it in a contract and also its price.
- **Four of the six contexts remain dependent on review practice rather than branch-protection
  enforcement.** A merge could proceed with a red `platform (windows-latest, stable)` if nobody
  looked. This is stated in every public document that carries the claim precisely because it is
  the kind of distinction that decays: adding a job *feels* like adding a gate, and it is not one
  until the protection rule names it.
- **CI cost and maintenance obligations increase**, and they increase permanently. Two additional
  runners on every pull request, on both toolchains, with the macOS and Windows toolchain
  installs that implies. A flaky platform test now costs maintainer attention on a job that
  cannot block the merge it is delaying.
- **A platform claim may need withdrawal through governance if the evidence stops passing.**
  If Windows breaks and cannot be fixed quickly, the honest response is a superseding ADR that
  withdraws the claim — not leaving a green-looking table above a red job. That is a slower
  remedy than the situation may feel like it deserves.

**What is locked in:** `contracts/support-policy.md` becomes the single normative statement of
supported platforms, and every summary resolves in its favour.

**To reverse this** — to withdraw a platform, or to return the contract to a Linux-only claim —
requires a superseding ADR with an impact analysis, on the same terms this record used.

## Impact analysis

Required by `contracts/support-policy.md` §Change control for any change to the support
contract.

| Surface | Impact | Status |
|---|---|---|
| **Published packages** | **None.** Nothing is published — neither `renvor` nor `renvor-cli` exists on crates.io | No impact |
| **Tags and releases** | **None.** `renvor-rs/renvor` has **0** tags and **0** releases | No impact |
| **Installed downstream users** | **None known, because nothing is published.** There is no installed user whose platform assumption could break. This is the cheapest moment this claim will ever be made | No impact |
| **Documentation surfaces** | `SUPPORT.md`, `docs/docs/support-policy.mdx`, `README.md`, `CONTRIBUTING.md`, `RELEASING.md`, `GOVERNANCE.md`, and `.github/ISSUE_TEMPLATE/bug_report.yml` each carry a support or platform statement | Converged in this change; each points at the contract |
| **Compatibility matrix** | The platform table moves from **one claimed platform** to **three**, each with its contexts and its enforcement level named | Rewritten in `contracts/support-policy.md` 1.1.0 |
| **CI job names and the branch-protection distinction** | **No workflow change.** `ci.yml` already produces all six contexts. What changes is that the distinction between *running* and *required* is now stated normatively instead of only in prose | No CI change |
| **Branch protection** | **Unchanged.** The required list stays `verify (1.94.0)`, `verify (stable)`, `security`, `docs`. Changing it is out of scope for this record — see §Alternatives, option 2 | No settings change |
| **MSRV** | **Unchanged at 1.94.0.** This is an **additive compatibility commitment**, not an MSRV break, which is why the contract goes to **1.1.0** and not 2.0.0 | No change |
| **Future downstream consumers** | Gain a three-platform promise with its enforcement level disclosed, before the first release rather than retrofitted after one | Improved |
| **Security** | **None.** No trust boundary, capability, or dependency changes. The `security` check remains required | No impact |
| **Rollback / withdrawal process** | Withdrawing a platform requires a **superseding ADR** with an impact analysis, a contract version bump, and synchronisation of both summaries. Evidence of the failing platform is recorded rather than the claim being quietly dropped | Stated here so a future withdrawal has a named route |

## Compliance

| Authority | How this record satisfies it |
|---|---|
| **FR-013** | State, reviewer, and date recorded; acceptance gated on W-002's four compensating controls; the non-independence of that review is stated prominently and not softened |
| **FR-017 – FR-021** | The support contract this record governs continues to satisfy them; the MSRV declaration, testing, dependency, and change rules are carried forward unchanged |
| **FR-061** | The scheduled Phase 006 MSRV revalidation is carried forward with its named owner, not dropped in the supersession |
| **Constitution principle V** | The support policy is a release contract; it is changed through the mechanism the contract itself names |
| **Constitution principle X — no claim exceeds measurement** | Every platform claim names the contexts that carry it; the enforcement gap is stated; the `#[cfg(unix)]`-gated behaviours and the absence of Windows adversarial review are recorded as limits rather than smoothed over; **no stable version number is asserted**, because the stable channel is resolved by CI and not by this document |
| **Constitution principle XII** | A known-false statement — the Phase 001 platform table presented as current policy — is corrected rather than left standing behind footnotes |
| **`contracts/support-policy.md` §Change control** | Changed by a superseding ADR carrying the impact analysis that contract requires |
| **PLAN.md §17.2** | macOS and Windows enter the matrix in the phase that introduces platform-sensitive behaviour. That phase was 002; the jobs landed at T150; this record makes the claim the jobs already support |

## Acceptance gate

**This record is `proposed`. Acceptance is a separate, later commit**, and the sequence is the
point. W-002's controls 3 and 4 cannot honestly be marked met before the CI run that satisfies
control 3 exists. Recording acceptance in the same commit that proposes a decision asserts that
controls passed before they were run — an error made once already in this project, in Phase 002,
and deliberately not repeated.

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template **before** acceptance | ✅ **Met 2026-08-21** — four alternatives, each with a stated rejection reason, including the status quo and the option this record declines to take; three benefits and **four accepted costs** recorded, not only benefits |
| 2 | Verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) | ✅ **Met 2026-08-21** — see §What control 2 found |
| 3 | All required CI and security checks passing | ⏳ **Not yet met** — pending the Wave-A CI run on the exact head that carries this record. **Will be completed with the measured result, or this record does not advance** |
| 4 | A dated review record stored with the ADR | ⏳ **Not yet met** — recorded at acceptance, dated with the date the review was actually performed |

**Two of four controls are met. This record remains `proposed`.**

### What control 2 found

The governance checklist was run against the **local ignored working copy** of
`specs/001-governance-foundation/checklists/governance.md`. That copy is **byte-identical to the
immutable source** at `01327b1ee61b73ebbd4f9198c04d651b38367ba8` — SHA-256
`c91f812367e5800738d38df6962f1647f292e2186c4a5a47bdaa3a8527e70feb` for both — so running it
locally is not a weaker check than reading it from the pinned commit. **Nothing under `specs/`
was staged, republished, or restored to Git** by this change; the directory remains ignored at
`.gitignore:85` and untracked.

**79 items, 79 checked, 0 unchecked**, counted mechanically from the pinned blob rather than
read off a summary line.

Three items were re-examined specifically, because they are the ones this decision could have
invalidated:

| Item | Question | Outcome |
|---|---|---|
| **CHK019** | Is the product-versus-executable naming distinction required to be *justified* in the decision record? | **Unaffected.** This record decides nothing about naming |
| **CHK050** | Is a response window defined for security advisories? | **Still passes.** The window is carried forward *by reference* to `governance/dependency-advisory-policy.md`; this record does not copy it, so the two cannot drift |
| **CHK034** | Are the pinned minimum and the **floating stable channel** distinguished, so *"tested toolchains"* cannot be read as two fixed versions? | **This is the finding.** See below |

**Control 2 found a real defect, and it is CHK034.** The checklist asks that the floating
stable channel be distinguishable from a fixed version. Four tracked documents were stating it
as a **fixed number** — `contracts/support-policy.md`, `SUPPORT.md`, and
`docs/docs/support-policy.mdx` each carried *"current stable, 1.97.1 at time of writing"*, and
`PLAN.md` §8.1 carried it in a planning snapshot.

A version number written into a document does not float. It is correct on the day it is typed
and silently false afterwards, and the reader has no way to tell which day they are on. That is
precisely the read CHK034 exists to prevent, and the documents had drifted into it.

**This is what drove §6 of this change**, and it was found by consulting the checklist rather
than by re-reading the draft — which is the case for having a control that looks at a different
artefact than the author was. The remedy is durable wording (*"the current stable channel,
resolved and recorded by CI at run time"*) rather than a newer number, because a newer number
would reintroduce the same defect with a later expiry date.

`PLAN.md` §8.1 is left unchanged: it is explicitly a **dated registry snapshot** from
2026-08-11, and a snapshot is allowed to name the version that was current when it was taken.
It does not claim to be current policy.

### Still owed

**W-002 does not close on acceptance.** When the first qualified independent reviewer becomes
available they re-review this record in full — including the alternatives it rejects and the
Phase 001 scope argument in §Waiver authority — alongside every other record accepted under
W-002.

Until then the underlying problem is unchanged: there is one maintainer, and no second person
qualifies as independent. **The maintainer decision of 2026-08-21 that this record implements is
also not an independent review**, and nothing here should be read as making it one.

### Review history

- **2026-08-21, proposed.** Controls 1 and 2 completed and recorded above. Controls 3 and 4
  deliberately left open, with the reason stated, rather than pre-filled.
