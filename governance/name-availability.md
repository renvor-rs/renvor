# Name Availability Record

> ## Amended 2026-08-17 — the executable name changed to `renvor` (ADR-0010)
>
> **Everything already in this file is preserved unedited.** It is dated evidence from
> 2026-08-11 and remains the record of what was verified then. The `renover` rows below are
> **historical**: they record a name this project verified and then chose to stop using. They
> are not instruction, and `renover` is no longer an active Renvor name.
>
> The re-verification for the new executable name is in
> **[§Re-verification 2026-08-17 — executable `renvor`](#re-verification-2026-08-17--executable-renvor)**
> at the foot of this file, added rather than substituted so both passes stay comparable.
>
> **Risk movement:** **R-3** (bounded clearance of `renover`) is **retired** — the name it
> qualified is no longer used. Its replacement, the bounded clearance of `renvor`, is recorded
> as **R-3a**. **R-2** (`renovate` confusability) is **reduced but still open**: the typed
> string moves from Levenshtein 3 to 4 from `renovate`.


**Status**: Complete — all ten rows verified 2026-08-11 (T016–T021). **T022 stop gate: PASSED.**
**Satisfies**: spec FR-001 – FR-006, FR-048, FR-049
**Schema**: `specs/001-governance-foundation/data-model.md` §Name Availability Record
**Checked by**: Ahmed Anbar (maintainer)
**Validity window**: 30 days. Every row below expires **2026-09-10** and must be re-verified before the first content push (T053/T054).

> **Stop rule (FR-003).** A status of `held-by-other` or `ambiguous` halts the phase
> until an explicit naming decision is recorded in `decisions/`. No substitute name is
> ever selected automatically, and no partially-renamed state is committed.

> **Verify, do not reserve (FR-049).** Package-registry names are checked but **not**
> claimed by publishing. Those rows terminate at `available`; only the hosting
> organization and repository reach `owned-by-project` in this phase. The residual
> risk that a verified name is taken before first publication is a tracked known
> limitation, not an oversight. **No placeholder crate was published to reserve any name.**

## Rows

All ten items are required. A missing row is a blocker, not an omission.

| Item | Intended value | Location checked | Date checked (UTC) | Status | Checked by | Evidence | Decision |
|---|---|---|---|---|---|---|---|
| Product name | `Renvor` | Derived — see §Product name below | 2026-08-11T16:26Z | `available` (derived) | Ahmed Anbar | No registry governs a product name. Hosting org claim is the operative control. **Trademark search not performed** — see residual risk R-4 | Proceed; R-4 tracked |
| Package prefix | `renvor-` | crates.io search API | 2026-08-11T16:25:29Z | `available` | Ahmed Anbar | `GET https://crates.io/api/v1/crates?q=renvor&per_page=100` → `meta.total = 0`. **No crate on crates.io contains the string "renvor" at all**, so the entire prefix namespace is unoccupied | Proceed |
| Facade package | `renvor` | crates.io API | 2026-08-11T16:25:05Z | `available` | Ahmed Anbar | `GET https://crates.io/api/v1/crates/renvor` → **HTTP 404** `crate 'renvor' does not exist`. Case variants `Renvor`, `RENVOR` also 404 | Proceed; verified not reserved |
| CLI package | `renvor-cli` | crates.io API | 2026-08-11T16:25:06Z | `available` | Ahmed Anbar | `GET https://crates.io/api/v1/crates/renvor-cli` → **HTTP 404**. Underscore-equivalent `renvor_cli` also 404 (crates.io collides `-` and `_`, so both forms are free) | Proceed; verified not reserved |
| Executable | `renover` | Six registries + local PATH + public source search — see §Executable scope | 2026-08-11T16:27:26Z | `available` (**bounded** — see scope) | Ahmed Anbar | crates.io 404; Homebrew formula 404; Homebrew cask 404; npm 404; PyPI 404; Debian sources 0 exact / 0 other; not on local PATH; 0 public `Cargo.toml` declaring it | Proceed; R-3 tracked |
| State directory | `.renvor/` | Derived from the product name | 2026-08-11T16:26Z | `available` (derived) | Ahmed Anbar | Filesystem convention, no registry. Uniqueness follows from the `renvor` name being free | Proceed |
| Environment prefix | `RENVOR_` | Derived from the product name | 2026-08-11T16:26Z | `available` (derived) | Ahmed Anbar | Process-environment convention, no registry. No conflict with any common prefix | Proceed |
| Hosting organization | `renvor-rs` | GitHub REST API | 2026-08-11T16:26:01Z | `available` | Ahmed Anbar | `GET /users/renvor-rs` → **404**; `GET /orgs/renvor-rs` → **404**; case probes `RENVOR-RS`, `Renvor-RS` → 404. **No 301 on any probe**, so the name is not a renamed account holding a redirect | Proceed to T024 |
| Hosting repository | `renvor` **scoped under `renvor-rs`** | GitHub REST API | 2026-08-11T16:26:01Z | `available` | Ahmed Anbar | `GET /repos/renvor-rs/renvor` → **HTTP 404**. This path is distinct from the global account `Renvor` — see §Namespace disambiguation | Proceed to T024 |
| Documentation domain | `renvor.dev` | Public RDAP + maintainer attestation | 2026-08-11T16:26:52Z | **`owned-by-project`** | Ahmed Anbar | `GET https://rdap.org/domain/renvor.dev` → HTTP 200. Registered **2026-08-11T06:48:16Z**, expires **2027-08-11T06:48:16Z**, registrar **CloudFlare, Inc.**, nameservers `coco.ns.cloudflare.com` / `earl.ns.cloudflare.com`, status `add period` + `client transfer prohibited`. Maintainer Ahmed Anbar attests ownership and control | Owned; no acquisition needed |

**Status values**: `unchecked` · `available` · `owned-by-project` · `held-by-other` · `ambiguous`

## Namespace disambiguation — the global `Renvor` account is not a collision

A GitHub **user** account holds the global login `Renvor`:

| Field | Value |
|---|---|
| Canonical login | `Renvor` |
| Account id | 206448205 |
| Type | **User** (`GET /orgs/renvor` → 404, confirming it is not an organization) |
| Created | 2025-04-06T16:33:58Z |
| Public repos | 2 |
| URL | `https://github.com/Renvor` |

**This does not block anything in this phase.** The project's paths are
`github.com/renvor-rs` and `github.com/renvor-rs/renvor`; neither is occupied. The
unrelated account occupies only the *global login* `renvor`, which the project never
requested and does not need.

Recorded for completeness: `GET /repos/renvor/renvor` returns **301**, resolving to
repository id 447414226, now `MetiuMicin/Discord-Test` — a stale rename redirect on that
account, unrelated to this project and touching no path the project will use.

## Executable name — bounded search scope for `renover` *(historical — superseded 2026-08-17)*

**There is no global registry of executable names.** Any claim of exhaustiveness would be
false. What was actually checked, and therefore what the `available` status means:

| # | Source checked | URL / method | Result |
|---|---|---|---|
| 1 | crates.io package name | `GET https://crates.io/api/v1/crates/renover` | HTTP 404 |
| 2 | Homebrew formula | `GET https://formulae.brew.sh/api/formula/renover.json` | HTTP 404 |
| 3 | Homebrew cask | `GET https://formulae.brew.sh/api/cask/renover.json` | HTTP 404 |
| 4 | npm registry | `GET https://registry.npmjs.org/renover` | HTTP 404 |
| 5 | PyPI | `GET https://pypi.org/pypi/renover/json` | HTTP 404 |
| 6 | Debian sources | `GET https://sources.debian.org/api/search/renover/` | 0 exact, 0 other |
| 7 | Public Rust manifests | code search for `name = "renover"` and whole-word `renover`, path `Cargo.toml` | **0 results** |
| 8 | This machine's `PATH` | `command -v renover` | not present |

**Explicitly out of scope** — not checked, and not claimed: Linux distributions other than
Debian, BSD ports, Windows package managers, language ecosystems beyond Rust/npm/PyPI,
private or internal tooling, and any binary distributed outside a package manager. A
collision in those spaces remains possible.

## Residual risks carried forward

These belong in `governance/phase-001-evidence.md` §6 with owners (T083).

| ID | Risk | Owner | Closes in |
|---|---|---|---|
| R-1 | `renvor` and `renvor-cli` are **verified but unreserved**. A third party may claim either between now and first publication. Deliberate — FR-049 forbids placeholder publishing | Ahmed Anbar | The phase performing the first crates.io publication |
| R-2 | **Confusability with `renovate`.** The crate `renovate` (Postgres schema migration, ~6.2k downloads) and the widely used npm `renovate` (Mend dependency bot) sit 1–2 characters from `renover`. Users may typo one for the other in either direction. Not a blocker; ADR-0001 (T026) should acknowledge it when justifying the product-versus-executable split | Ahmed Anbar | ADR-0001 |
| R-3 | The `renover` clearance is **bounded**, not exhaustive — see the scope table above | Ahmed Anbar | Ongoing |
| R-4 | **No trademark or common-law search was performed** for the product name `Renvor`. `contracts/public-identity.md` names one as the verification method for that row; it was outside the authorised scope of this pass. The row is recorded as derived, not as trademark-cleared | Ahmed Anbar | Before first public announcement |
| R-5 | `renvor.dev` is within its registry **Add Grace Period** (`add period` status) and expires 2027-08-11. Renewal is an operational obligation | Ahmed Anbar | T084 recurring obligations |

## Blocking notes

- **T022 stop gate: PASSED 2026-08-11.** No row is `held-by-other`; no row is materially
  ambiguous. The global `Renvor` user account was evaluated and found not to collide with
  any path the project uses. No substitute name was considered or selected.
- No row was filled in by inference. Each names a location actually consulted, an exact
  URL, a UTC timestamp, and the person who consulted it.
- All rows expire **2026-09-10** and are re-verified before the first content push.


## Re-verification 2026-08-17 — executable `renvor`

**Added by ADR-0010. This does not replace the 2026-08-11 pass above; it sits beside it.**

Checked by Ahmed Anbar at **2026-08-17T08:45:34Z**, using the same eight probes, **each paired
with a positive control**. The 2026-08-11 pass had no controls; that is the one methodological
difference between the two, and it is an improvement rather than a discrepancy.

| # | Source checked | Target `renvor` | Positive control | Control result |
|---|---|---|---|---|
| 1 | crates.io package name | HTTP **404** | `serde` | HTTP 200 |
| 2 | Homebrew formula | HTTP **404** | `git` | HTTP 200 |
| 3 | Homebrew cask | HTTP **404** | `firefox` | HTTP 200 |
| 4 | npm registry | HTTP **404** | `react` | HTTP 200 |
| 5 | PyPI | HTTP **404** | `requests` | HTTP 200 |
| 6 | Debian sources | **0** exact, **0** other | `bash` | 1 exact, 20 other |
| 7 | Public Rust manifests | **0** | `serde filename:Cargo.toml` | **829,440** |
| 8 | This machine's `PATH` | not present | `ls` | present |

Also re-verified the same day: `renvor-cli` → **HTTP 404**, `renvor_cli` → **HTTP 404**.

### What the controls caught

**Probe 7's first run was invalid, and only the control revealed it.** Target and control both
returned `0`. A `0` control is impossible for `serde`, so the run was investigated rather than
recorded: GitHub's code-search API had returned **HTTP 403 — rate limit exceeded**, which the
tooling had surfaced as an empty result set. The probe was re-run after the limit reset and the
control then returned 829,440. **Without the control, a rate-limit error would have been
recorded as evidence of availability.**

**Probe 7 is additionally bounded, and this bound was also found rather than assumed.** GitHub
code search returns **0** for `renvor filename:Cargo.toml`, yet `renvor-rs/renvor` is public and
contains **four** such manifests (`renvor`, `renvor-core`, `renvor-config`, `renvor-testkit`).
The index therefore does not cover this repository, so probe 7's `0` **bounds** absence rather
than proving it. An independent second source (grep.app) returned no results either, which
corroborates without removing the bound.

### Scope — unchanged, and still bounded

**There is still no global registry of executable names.** Explicitly out of scope, not checked,
and not claimed: Linux distributions other than Debian, BSD ports, Windows package managers,
language ecosystems beyond Rust/npm/PyPI, private or internal tooling, and any binary
distributed outside a package manager. A collision in those spaces remains possible.

### Risk rows updated

| Risk | State after 2026-08-17 |
|---|---|
| **R-2** — confusability with `renovate` | **Open, reduced.** The typed string moves from Levenshtein **3** (`renover`) to **4** (`renvor`). ADR-0001 itself argued that a distance-3 pair was worse than a distance-4 one; ADR-0010 applies that argument to the executable |
| **R-3** — bounded clearance of `renover` | **Retired.** The name is no longer used by this project |
| **R-3a** — bounded clearance of `renvor` | **Open.** Same bound, same reason, now attached to the name actually in use. Owner: Ahmed Anbar. Ongoing |
| **R-4** — no trademark search for `Renvor` | **Open, unchanged.** ADR-0010 does not alter the product name and performs no trademark search |
| **R-5** — `renvor.dev` renewal | **Open, unchanged.** Expires 2027-08-11 |
