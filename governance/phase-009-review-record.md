# Phase 009 — Review Record

**Companion to**: [`phase-009-evidence.md`](phase-009-evidence.md)
**Phase**: 009 — Authentication, sessions, tokens, and policies

**No independent human review of this phase has occurred, and none is claimed.** Closure rests on
[W-020](waivers.md), the **ninth consecutive** phase-level waiver of the same rule for the same
reason. Everything below is maintainer-commissioned and **advisory**, not independent.

## 1. What was commissioned, and what came back

| Agent | Purpose | Disposition |
|---|---|---|
| requirements validator | 86 FRs and 15 SCs read against the code at `723a6a4` | **DELIVERED.** 69 SATISFIED / 10 PARTIAL / 3 NOT SATISFIED / 4 NOT VERIFIABLE, plus 12 findings |
| security validator | attack nine published claims at `723a6a4` | **DELIVERED.** Nine claims attacked, **four broke** |
| Codex review | closing review at `f37712a`, re-offered at `bb5c349` | **NOT PERFORMED.** Idle, one follow-up, idle again. Not re-rolled |
| `standards-researcher` | NIST SP 800-63B-4, RFC 9700/9106/8725, ASVS, cookie spec | **DELIVERED** after one follow-up |
| `package-researcher` | crates for cookies, CSRF, tokens, blocklist, policy | **NOT PERFORMED.** Idle twice |
| `boundary-researcher` | map Renvor's extension points | **NOT PERFORMED.** Idle twice |
| `f3-validator` | cross-check an earlier fix | **NOT PERFORMED.** Idle twice |

**Four of seven delegated agents returned nothing.** An empty response is recorded as NOT PERFORMED,
never as a pass — an idle agent and a finished one are indistinguishable from outside, and treating
silence as "no findings" manufactures assurance out of nothing.

The consequence is stated rather than softened: **the third review is missing**, and coverage rests
on two. Those two found **fourteen** things, four of which broke security claims, so the marginal
value of a third pair of eyes on this code was demonstrably not zero.

## 2. The finding that matters most

The security review found **`Admitted` derived `Copy`** — so one counted attempt admitted unlimited
calls, and FR-063's whole structural claim was false along with three shipped sentences.

The commit *immediately before* that batch, `705d34d`, had removed exactly this derive from
`Authorized` for exactly this reason. `Admitted`'s own documentation said it reused that shape.
**It reused the shape and not the correction.**

Fixed by removing `Copy` **and** `Clone` — `Clone` too, because it is a one-line way back for anyone
with a borrow-check error — and pinned by a `compile_fail` doctest with a compiling control plus a
test asserting three calls cost three counted attempts.

## 3. Findings dispositioned by change

Nine were fixed in code: the `Copy` derive; the network axis keying on a full IPv6 address rather
than the `/64`; an unauthenticated, unprotected logout; registration as an Argon2id amplifier and
timing oracle; one token repository serving two tables; `PresentedCredentials` printing both
credentials through a derived `Debug`; a test application that discarded the HTTP method; a malformed
body rendering as `401`; and empty `invalidParams` lists.

Eleven were recorded and **not** fixed, each with the reason and an owner — including FR-011 having
no production caller and a password reset revoking nothing. They are carried in
[`phase-009-limitations.md`](phase-009-limitations.md), not closed.

## 4. What the reviews did not catch

Four further defects were found **after both reviews reported**, by the repository's own gates:

1. an error code with no declared HTTP status;
2. a committed OpenAPI snapshot that had gone stale;
3. ten credential-handling diagnostics that printed what they asserted about;
4. a publishable-package count that disagreed with its own table — batch J added
   `renvor-auth-http` to both lists a publication *reads* and neither list a human reads.

A fifth was found by the gate's step 8 only after step 4 was fixed: two `gitleaks` matches that had
been in the branch for a day, invisible because **a fail-fast gate reports its first failure and
nothing after it**.

A sixth was a defect in the **verification apparatus itself**, and it fails in the opposite
direction from the other five. Step 4's end-to-end route relay invokes `cargo run`, which *compiles
before it runs*, and the relay's 300-second deadline starts at `spawn` — so a cold build was charged
against a budget written to bound how long a binary takes to **answer**. The step immediately before
it builds the workspace `--all-features`, which produces `renvor-auth` with `features = ["tokens"]`;
the example resolves it with `features = []`, a different cargo unit `--all-features` cannot produce.
The cache miss was therefore structural, not a race. The gate reported `TransportNotWired` against a
transport that was wired correctly — measured at **221 seconds of build and 1.9 seconds of answer**
sharing one budget, while the example binary, invoked directly, answered instantly with exit 0.

That one is worth separating from the others. The usual assurance failure is a gate that passes what
it should have caught; this was a gate that **rejected what was correct**. It is not the milder
error. A gate that cries wolf is precisely what trains a maintainer to re-run until green — the
habit that would have hidden every one of the five defects above. It was reproduced under
instrumentation before anything was changed, fixed by compiling before relaying with the deadline
left untouched at 300 seconds, and pinned by two guards whose discrimination was proved by three
negative controls, one of which fails if a future maintainer answers a recurrence by widening the
deadline.

That ordering is the honest summary of this phase's assurance: the reviews found what review is good
at, the gates found what gates are good at, the gates' own failure found what neither was looking
for, and none of it substitutes for the independent human review that W-020 waives.

## 5. The reviewed heads, and why the dispositions still stand

The two delivered reviews ran at `723a6a4`; the Codex review was offered at `f37712a` and re-offered
at `bb5c349`. The closing head is `0090c678`. Seven files changed between `bb5c349` and `0090c678`,
of which three are under `crates/*/src/`:

| File | `mod tests` opens | lines added |
|---|---|---|
| `renvor-auth/src/abuse.rs` | 1092 | 1815–1827 |
| `renvor-auth/src/service.rs` | 556 | 834–847 |
| `renvor-cli/src/commands/routes.rs` | 354 | 755–888 |

**Every added line sits inside a `#[cfg(test)]` module. Zero production lines changed since the
reviewed head**, so the recorded dispositions apply to the production surface unchanged and no
re-review was commissioned. That is checkable rather than asserted: `git diff bb5c349 0090c678 --
'crates/*/src/*'` against the boundaries above. The other four changed files are `.gitleaks.toml`,
`RELEASING.md`, `xtask/src/main.rs`, and the committed OpenAPI snapshot — none of them a consumer
surface. Codex remains **NOT PERFORMED**; that gap is not narrowed by this reconciliation.
