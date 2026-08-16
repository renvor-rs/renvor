# ADR-0009: Remove `image-size` from the documentation site by vendoring a no-op replacement

| Field | Value |
|---|---|
| **ID** | 0009 |
| **State** | `proposed` |
| **Reviewer** | *(none — see the acceptance note below)* |
| **Review date** | *(none)* |
| **Superseded by** | — |

> **This record is `proposed` and must not be marked `accepted` in Phase 002.**
>
> Constitution workflow step 4 requires a decision to be captured before it is treated as
> accepted, and `GOVERNANCE.md` defines an independent reviewer as a **person** who did not
> author the record or the change. No such person has reviewed this. **W-004 covers ADR-0007
> only** and confers no authority here, exactly as it confers none over ADR-0008.
>
> The change it describes has landed, because PLAN.md §17.3 forbids accepting a phase with an
> open Critical or High finding and the alternative was to leave two of them open. That is a
> reason to record the decision honestly, not a reason to call it reviewed.

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
the *parent* escapes it either.

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

The replacement mirrors the real package's export map and **throws** from every entry point rather
than returning empty dimensions. A call is therefore loud, not silently dimensionless.

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
- The guard fails the build closed if an image is ever added, so the precondition is enforced
  rather than remembered.

## Alternatives considered

| Option | Why not |
|---|---|
| **Upgrade `image-size`** | No fixed version exists at any release; 2.0.2 is both latest and affected |
| **Upgrade Docusaurus** | 3.10.2 is the latest release and still requires `image-size ^2.0.2` |
| **Pin the `legacy` 1.2.1 line** | The advisory range is `<= 2.0.2`, which includes all of 1.x |
| **Dismiss the alerts** | Prohibited. PLAN.md §17.3 allows no waiver for a High finding, and this project does not dismiss findings it cannot fix |
| **Keep the status quo — exception plus reachability guard** | What Phase 001 did. It leaves two High alerts open indefinitely against an archived upstream, which §17.3 forbids carrying into acceptance |
| **Substitute a maintained sizing package** | Solves a problem this site does not have. Nothing here measures images; adding a *working* dependency to replace an unused one enlarges the graph to no purpose |
| **Remove the documentation site** | Disproportionate |

## Consequences

**The site can no longer measure images.** This is a genuine capability loss, not a free
substitution, and it is the reason this record exists rather than a one-line lockfile note. An
image added to this site would render without `width`/`height` and shift layout as it loads — which
is why the guard refuses the build instead of allowing it.

**Ownership cost.** Six vendored files now track `image-size`'s export map. If a future Docusaurus
imports a subpath the stub does not implement, the build fails loudly at that import rather than
silently — acceptable, and cheaper than the alternative, but it is maintenance this project now
owns. The stub deliberately does **not** implement the `./types/*` subpath; nothing imports it
today, verified by exhaustive search.

**A lockfile reader could misread it.** The replacement is versioned `3.0.0-renvor.1`, outside the
affected range. That is not scanner-gaming — the vulnerable parsers are physically absent from
`node_modules`, verified — but the `"resolved": "vendor/image-size-disabled"` line is the tell, and
it is worth knowing that the name alone looks like an upgrade.

**Exit condition.** Remove the override, the vendored package, **and** the guard together, the
moment a maintained `image-size` ships a fixed release or Docusaurus stops depending on it.
Removing any one of the three without the others is worse than removing none.

## Compliance

| Requirement | How this record satisfies it |
|---|---|
| **PLAN.md §17.3** | No Critical or High finding is carried into acceptance, and none is waived |
| **Constitution principle III / FR-035** | Custom infrastructure is recorded with the alternatives, their measured costs, and an exit condition |
| **Constitution principle XII** | The capability loss and the lockfile-readability hazard are stated rather than implied |
| **Constitution §Workflow #4** | Captured as a **proposed** record; acceptance deliberately withheld, since W-004 does not reach it |
| **Phase 002 FR-040** | The change is recorded in the dependency inventory alongside the resolved graph it produces |
