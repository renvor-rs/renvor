# ADR-0009: Remove `image-size` from the documentation site by vendoring a no-op replacement

| Field | Value |
|---|---|
| **ID** | 0009 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-006 |
| **Review date** | 2026-08-17 |
| **Superseded by** | — |

> **Accepted under waiver W-006, granted by Ahmed Anbar on 2026-08-17.**
>
> **No independent human review of ADR-0009 has occurred.** `GOVERNANCE.md` defines an independent
> reviewer as a **person** who did not author the record or the change; this project has one
> maintainer, who authored both. W-006 waives that requirement for **this record and nothing else**.
> **W-004 covers ADR-0007 alone** and confers no authority here, exactly as it confers none over
> ADR-0008; W-005 is phase-level and never authorises accepting a decision record.
>
> **W-006 waives *who reviews*. It waives nothing about *what must be true*** — not a security
> finding, not a CI or acceptance gate, not Phase 002's phase-level review, not any other record.
> Its bar is deliberately higher than `PLAN.md` §17.3: every Critical, High, **and Medium** finding
> against this record had to be fixed before acceptance, where §17.3 stops at High.
>
> **This record is the third explicit reviewed exception in Phase 002**, exceeding the waiver
> ledger's expected maximum of two per phase. That departure is recorded as one in
> `governance/waivers.md`, not absorbed by stretching an existing waiver.

## Context

Two **high**-severity advisories affect `image-size`, a transitive dependency of the documentation
site:

| Advisory | Severity | Affected | First patched |
|---|---|---|---|
| `GHSA-w3rx-r6r6-pgpr` | high (CVSS 7.5) | `<= 2.0.2` | **none** |
| `GHSA-5p2g-fcmc-qvqq` | high (CVSS 7.5) | `<= 2.0.2` | **none** |

Both are infinite-loop denial of service in the ICNS, JXL, and HEIF parsers.

**There is no version to upgrade to.** `image-size` 2.0.2 is simultaneously the affected ceiling
and the latest published release, and the upstream repository is archived. `@docusaurus/mdx-loader`
requires `image-size ^2.0.2`, and Docusaurus 3.10.2 is itself the latest release, so no upgrade of
the *parent* escapes it either. *(Every claim in this paragraph was re-verified against the npm
registry and the GitHub API on 2026-08-17, by two advisory reviews independently.)*

PLAN.md §17.3 prohibits accepting a phase with an open Critical or High finding, and no waiver for
one is available — security release blockers are explicitly never waived. Phase 001 recorded this as
`001-T108`, a **documentation deployment gate**, with a reassessment due 2026-09-11. It has been
open since.

Constitution principle III and FR-035 require an accepted decision record when custom
infrastructure is chosen over a maintained package. A vendored replacement package is custom
infrastructure, however small, which is why this record exists.

## Decision

Redirect `image-size` to a **local no-op package** at `docs/vendor/image-size-disabled`, through an
npm `overrides` entry paired with a root `file:` dependency, so that no published `image-size`
tarball is installed at all.

The replacement covers the subpaths this project can reach — `.` and `./fromFile` — and **throws
from every *measuring* entry point** rather than returning empty dimensions. The three
non-measuring members are faithful no-ops: `setConcurrency()` sizes an internal queue that no
longer exists, `disableTypes()` narrows a detector set that is empty, and `types` is `[]` because
the replacement truthfully detects no format. *(Stated precisely because "throws from every entry
point" was measured false for three of six exports.)*

The existing build-time guard `docs/scripts/check-image-inputs.mjs` is retained and re-scoped: it
previously enforced the *reachability* premise behind an advisory exception, and now enforces the
*consequence* of the removal — this site cannot measure an image, so it must contain none.

## Why this is safe here specifically

The vulnerable parsers were **already unreachable**, and the removal makes that structural rather
than circumstantial:

- The documentation tree contains **no image of any format** — six `.mdx`, two `.css`, one `.js`,
  and a favicon. There is nothing for any parser to parse.
- The only importer in the entire dependency tree is
  `@docusaurus/mdx-loader/lib/remark/transformImage/index.js`, which requires exactly one subpath
  (`image-size/fromFile`) and calls exactly one function, inside an existing `try`/`catch`.
- `node_modules/image-size` is a **symlink** to the vendored directory. No `image-size` directory
  containing parser code exists anywhere in the tree, and no bundled copy exists inside any other
  package — verified across all 1,377 installed packages.

### The guard is the only fail-closed control, and the throw is not a second one

An earlier revision of this record claimed the replacement's throw made a call **"loud, not
silently dimensionless."** **That is false, and it was measured false three times independently** —
once by the maintainer and once by each of the two advisory reviews.

`transformImage/index.js` wraps the call in `try { … } catch (err) { console.error(err); logger.warn(…) }`
and **does not rethrow**. Measured end-to-end, with an image present and the guard bypassed:

| Observed | Result |
|---|---|
| Stub error printed to the log | yes |
| `[WARNING] The image at … can't be read correctly` | yes |
| `npx docusaurus build` exit code | **0 — the build succeeded** |
| Emitted markup | `<img … src="data:image/png;base64,…" />` — **no `width`, no `height`** |

So throwing and returning `{}` differ **only in console output**. The rendered artifact is
identically dimensionless and the build passes either way.

**Therefore `check-image-inputs.mjs` is the sole control that can fail a build over an image
input.** It is not a belt beside the throw's braces; it is the whole belt. That is why its coverage
was hardened rather than merely documented — three reviews found five distinct ways past it, every
one now closed and controlled:

| Escape | Status |
|---|---|
| Content roots `i18n/` and `versioned_docs/` were never scanned | **Closed** — the scan is now by exclusion, so a new content root is covered by default |
| Symlinked files and directories were skipped entirely (`Dirent.isFile()` and `isDirectory()` are both false for a link) | **Closed** — any symlink under a scanned root is refused rather than followed |
| Reference-style `![alt][ref]` with a separate `[ref]: ./x.png` definition | **Closed** — definitions are matched |
| Bare ESM `import logo from "./logo.png"` — not a call expression | **Closed** |
| `<Image src="./x.png" />` — only lowercase `<img` was matched | **Closed** — any element with an image `src` |
| Nine raster extensions including `.tga`, which the real package itself parses | **Closed** — the extension set was widened |

**The guard now carries 20 committed controls** (`docs/scripts/check-image-inputs.test.mjs`), run in
CI: fifteen plant a caught form and require a refusal, five require a clean tree, a permitted
favicon, a non-image `src`, and an SVG *file* to pass while an SVG *embed* is refused. Before this,
CI exercised only the passing path — and "ok" is what a guard broken to always exit 0 prints too.

## Alternatives considered

| Option | Why not |
|---|---|
| **Upgrade `image-size`** | No fixed version exists at any release; 2.0.2 is both latest and affected |
| **Upgrade Docusaurus** | 3.10.2 is the latest release and still requires `image-size ^2.0.2` |
| **Pin the `legacy` 1.2.1 line** | The advisory range is `<= 2.0.2`, which includes all of 1.x |
| **Dismiss the alerts** | Prohibited. PLAN.md §17.3 allows no waiver for a High finding, and this project does not dismiss findings it cannot fix |
| **Keep the status quo — exception plus reachability guard** | What Phase 001 did. It leaves two High alerts open indefinitely against an archived upstream, which §17.3 forbids carrying into acceptance |
| **Substitute `image-dimensions@2.5.1`** (MIT, **zero dependencies**, `node >= 18`, published 2026-05-12) | The strongest rejected option, and the one that makes the capability loss *elective*. It would have removed the vulnerable parsers **and kept image sizing**, through the same `overrides` slot — it is ESM-only, but the stub's `imageSizeFromFile` is already `async`, so a CJS wrapper resolves it. Rejected on three grounds: it **parses HEIF**, reintroducing one of the three advisory format families in a different implementation with its own unaudited parser; it does **not** parse ICNS or JXL, so it is not a drop-in for the real export surface; and it returns `undefined` rather than throwing on unsupported input, a silent-degradation mode constitution principle III explicitly disfavours. Against a site with **no images at all**, adding any working parser buys a capability nothing uses and enlarges the attack surface this change exists to shrink |
| **Substitute `probe-image-size@7.4.0`** (MIT, 3 dependencies, published 2026-08-15) | Same reasoning, weaker position: three transitive dependencies rather than zero, and a stream-oriented API that does not match `imageSizeFromFile`'s shape. Published two days before this record and correspondingly unproven here |
| **Remove the documentation site** | Disproportionate |

**Package-first was applied, and its answer was recorded rather than assumed.** The two maintained
candidates above were named and scored only after an advisory review pointed out that the original
table rejected the entire package category in one line without naming a single package — which is
not the evaluation FR-035 and principle III require, whatever the conclusion.

## Consequences

**The site can no longer measure images — and that loss was chosen, not forced.** A maintained,
zero-dependency substitute existed (`image-dimensions`) and would have kept the capability. It was
rejected for the reasons in the table above, all of which are defensible; but the record must not
present an elective trade-off as an ecosystem constraint. An image added to this site would render
without `width`/`height` and shift layout as it loads — which is why the guard refuses the build
instead of allowing it.

**Ownership cost.** Six vendored files now track `image-size`'s export map, and the tracking is
approximate in three disclosed ways:

- The stub deliberately does **not** implement the `./types/*` subpath. Nothing imports it today,
  verified by exhaustive search; `require('image-size/types/png')` fails loudly with
  `ERR_PACKAGE_PATH_NOT_EXPORTED`.
- The stub **adds** a `./package.json` export the real package does not have.
- The real 2.0.2 declares `"bin": {"image-size": "bin/image-size.js"}`. The stub declares none, so
  `node_modules/.bin/image-size` no longer exists. Nothing in the tree invokes it, so the impact is
  nil — but it is an item on this ledger, and it went unrecorded until a review found it.

If a future Docusaurus imports a subpath the stub does not implement, the build fails loudly at that
import rather than silently. That is acceptable, and cheaper than the alternative, but it is
maintenance this project now owns.

**The vendored code is covered by none of the gates that cover a real dependency.** This is the
honest counterweight to "removal from the dependency graph", and it is stated rather than implied:

| Gate | Covers the vendored code? |
|---|---|
| `cargo-deny` (licences, advisories, bans, sources) | **No** — Rust only |
| `dependency-review-action` | **No** — a `link: true` node with no version or registry URL is not a registry package |
| `npm audit` | **No** — and see the discrimination limit below |
| Subresource integrity | **No** — the lockfile entry carries no `integrity` hash |
| CodeQL | **No.** Verified 2026-08-17 against the live configuration: default setup covers **`rust` and `actions` only**. No JavaScript analysis runs on this repository, so the vendored code is examined by no static analysis at all |
| ESLint / Prettier | **No** — the repository configures neither |
| This project's own controls | **Yes** — 20 guard controls plus the stub's behaviour, run in CI |

Enabling JavaScript in CodeQL default setup is free on a public repository and is the remedy; it is
a **repository-settings change deliberately outside this pull request**, and is carried as a named
open item rather than claimed. Adding a `CODEOWNERS` entry was considered and **not** done: with one
maintainer it would route review to the author of the change, which compensates for nothing — the
same rule the waiver ledger applies to compensating controls.

**`npm audit: 0` cannot discriminate, and is not cited as if it could.** The stub is versioned
`3.0.0-renvor.1`, outside the advisory range `<= 2.0.2`, so `npm audit` would report zero regardless
of what the stub contains — correct code, broken code, or the real parsers copied in verbatim. The
number is a consequence of the version string. **The evidence that the parsers are gone is the
resolved graph** — a symlink to a six-file directory, no `image-size` tarball anywhere, no bundled
copy across 1,377 packages — not the audit result.

**A lockfile reader could misread it.** The replacement is versioned `3.0.0-renvor.1`. That is not
scanner-gaming — the vulnerable parsers are physically absent from `node_modules`, verified — but
the `"resolved": "vendor/image-size-disabled"` line is the tell, and the name alone looks like an
upgrade.

**Licence.** The vendored files are original work, not derived from `image-size`, and are declared
`MIT OR Apache-2.0` to match the repository rather than the narrower MIT the first revision carried.

**Exit condition — four artifacts, not three, and the order matters.** Remove **(1)** the
`overrides` entry, **(2)** the root `"image-size": "file:./vendor/image-size-disabled"` dependency,
**(3)** the vendored directory, and **(4)** the guard, *together*, the moment a maintained
`image-size` ships a fixed release or Docusaurus stops depending on it.

The fourth artifact was omitted from the first revision, and the omission is not cosmetic: the
`$image-size` override form resolves its specifier **from** the root dependency, so the two are
coupled and neither can be removed alone. Measured, both partial orders fail —

| Partial removal | Result |
|---|---|
| Dependency removed, override kept | `npm error Unable to resolve reference $image-size` |
| Override removed, dependency kept | `npm ci` fails: `Missing: image-size@2.0.2 from lock file` |

— and note what the second names. **With the override gone, npm's next resolution intent is the
real, vulnerable 2.0.2.** Measured: dropping the `overrides` block and running `npm install`
restores `image-size@2.0.2` nested under `@docusaurus/mdx-loader` — the copy its `require` actually
resolves — and `npm audit` reports **25 vulnerabilities, 19 high**. The `overrides` entry is the
single load-bearing control; the root `file:` dependency protects nothing on its own.

Removing any one of the four without the others is worse than removing none.

## Compliance

| Requirement | How this record satisfies it |
|---|---|
| **PLAN.md §17.3** | No Critical or High finding is carried into acceptance, and none is waived, dismissed, or suppressed. Both HIGH advisories are closed by **removal from the resolved graph** |
| **Constitution principle III / FR-035** | Custom infrastructure recorded with the alternatives — including two **named, scored maintained packages** — their measured costs, the ownership cost, and a four-artifact exit condition |
| **Constitution principle XII** | The capability loss is stated **as elective rather than forced**; the lockfile-readability hazard, the gate asymmetry, and the `npm audit` discrimination limit are stated rather than implied |
| **Constitution §Workflow #4** | Accepted under **W-006**, granted 2026-08-17 for this record alone, after two clean-context advisory reviews with every finding individually dispositioned. The review is a **self-review** and is not independent |
| **Phase 002 FR-040** | Recorded in `governance/phase-002-dependency-inventory.md` §T160, with version, licence, maintenance status, engines, advisories, and the **executable dependency proof** read from the committed lockfile. *(This row previously claimed an inventory entry that did not exist; the entry was written to make the claim true rather than narrowed to make it defensible.)* |

## Review record

Two clean-context advisory reviews were run against this record on 2026-08-17, discharging W-006's
first counted control.

| Review | Scope | Result |
|---|---|---|
| Requirements / package governance | facts, alternatives, export map, lockfile reproduction, guard, compliance table, traceability | **10 findings** — 2 CRITICAL, 5 MAJOR, 3 MINOR |
| Security / supply chain | tarball absence, durability, stub risk, guard bypasses, the try/catch question, `uuid`, gate coverage | **14 findings** — 0 CRITICAL, 5 MAJOR, 9 MINOR |

**Both are NON-INDEPENDENT and ADVISORY** and must never be described otherwise. Both returned
enumerated findings, so both are recorded as **performed**; a review returning nothing would have
been recorded as *not performed*, never as passed.

**All 24 findings are dispositioned individually** — no grouped dispositions — in
`governance/phase-002-evidence.md` §W-006. Every Critical, High, and **Medium** finding is fixed, as
W-006 requires. Three findings were reproduced independently by both reviewers and by the
maintainer; the corrections they forced are in the Decision, "Why this is safe here specifically",
Alternatives, Consequences, and Compliance sections above, and in the guard, its controls, the CI
workflows, and the dependency inventory.

**An earlier draft of the evidence pack recorded this acceptance before the reviews returned.** The
requirements review caught it as a CRITICAL. It was corrected before any of it was committed, and it
is recorded here because a record that reports its own review favourably is exactly what W-006's
compensating controls exist to prevent.
