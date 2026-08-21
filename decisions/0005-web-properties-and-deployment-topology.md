# ADR-0005: Separate the public framework from three private website and deployment repositories

| Field | Value |
|---|---|
| **ID** | 0005 |
| **State** | `accepted` |
| **Reviewer** | `Ahmed Anbar — self-review under W-002` |
| **Review date** | 2026-08-12 |
| **Superseded by** | **ADR-0006 D13** *(2026-08-15)* — **partial: repository visibility, plus one deployment-status sentence.** Everything else in this record still stands, and this record is **not** in state `superseded` |
| **Owner** | Ahmed Anbar |

> ## Partial supersession 2026-08-15 — the visibility model changed; the separation did not
>
> **This record remains `accepted` and its central decision is unchanged**: the framework, the
> landing page, the documentation site, and the deployment configuration live in **four
> separate repositories**. Every one of the four forces in Context still holds, and the
> repository-boundary reasoning is why the split survives the change.
>
> **What is superseded is the visibility column and the sentences that depend on it.** ADR-0006
> **D12** (2026-08-14) first replaced this model, and **D13** (2026-08-15) replaced D12. **All
> four repositories are now public on GitHub and canonical there. No Renvor repository is
> private.**
>
> **Read the following as describing 2026-08-12, not today. They are preserved unedited:**
>
> - the title, "*…three **private** website and deployment repositories*";
> - the Decision table's `Visibility` column marking `renvor-site`, `renvor-docs`, and
>   `renvor-infra` **Private**;
> - "***Private source, public sites.** All deployed properties are publicly reachable. Only
>   the source of the three website and deployment repositories is restricted.*" — **both
>   halves are false today.** No repository source is restricted, and **no Renvor property is
>   deployed at all** — measured 2026-08-15, `renvor.dev`, `docs.renvor.dev`, and
>   `www.renvor.dev` each resolve to the shared origin and return **HTTP 404**, while HTTPS
>   fails validation against a public trust store because Traefik serves its default
>   self-signed certificate. **Something answers; no Renvor content is served.** That sentence
>   is the origin of a claim corrected in `PLAN.md` §26.1 and §26.4 and in ADR-0006 D7 and D13
>   on 2026-08-15; the evidence ledger carries a global qualifier in its Status line, and
>   **§3al still contains the uncorrected original wording**, annotated there;
>   *(**Superseded 2026-08-17.** The 2026-08-15 measurement above stands as a dated observation and
>   is deliberately not rewritten — but it no longer describes the internet. `renvor.dev` returns
>   **200** over a valid Let's Encrypt certificate and `www.renvor.dev` permanently redirects to
>   it. **`docs.renvor.dev` is unchanged**: still 404, still Traefik's default self-signed
>   certificate, still not deployed. So "no Renvor property is deployed at all" is now false, while
>   "something answers; no Renvor content is served" remains true of the documentation hostname
>   alone. See [`governance/deployment-evidence.md`](../governance/deployment-evidence.md).)*;
> - "*`renvor-infra` has the strongest case for remaining private permanently*" — **reversed by
>   D13**, which weighed public GitHub, private GitHub, and private self-hosted GitLab and
>   chose public. D13's alternatives review is where that reversal is argued.
>
> **ADR-0006 was `proposed` when this banner was written and was accepted on 2026-08-15**, once
> T106 closed. This banner records an observed live fact and the maintainer's direction; the
> supersession it describes now also rests on an accepted record, reviewed under W-002 as a
> **non-independent** self-review.

## Context

Renvor needs two public websites — a landing page at `renvor.dev` and documentation at
`docs.renvor.dev` — served from a Hostinger VPS, with DNS and edge at Cloudflare. The
question is where their source lives.

Four forces:

1. **The public framework repository is a governance artifact.** Phase 001 spent its entire
   budget making `renvor-rs/renvor` reviewable: a verified publish set, a fail-closed
   verification sequence, a licence policy, secret scanning. Every additional concern added
   to it dilutes that and enlarges the surface a reviewer must read to trust the framework.
2. **Website source carries material the framework licence does not cover.** Brand assets
   are not automatically `MIT OR Apache-2.0`. Publishing them in a repository whose licence
   files say otherwise makes a licensing claim nobody decided to make.
3. **Deployment configuration describes the attack surface of a live server.** Ingress
   hostnames, namespace layout, and image references are a map of the origin. That map does
   not need to be public for the *websites* to be public.
4. **The three properties change at different rates for different reasons.** A landing copy
   fix, a documentation version bump, and a cluster change have nothing in common except
   the server they land on.

## Decision

Four repositories under `renvor-rs`:

| Repository | Visibility | Source of truth for |
|---|---|---|
| `renvor` | **Public** | Framework source, crate metadata, rustdoc inputs, governance, releases |
| `renvor-site` | **Private** | V7 landing page and approved V7 brand assets → `renvor.dev` |
| `renvor-docs` | **Private** | Production documentation site → `docs.renvor.dev` |
| `renvor-infra` | **Private** | Kubernetes manifests, ingress, TLS, operational runbooks |

**Private source, public sites.** All deployed properties are publicly reachable. Only the
source of the three website and deployment repositories is restricted.

**Dependency direction is one-way.** The website repositories consume the framework through
published versioned artifacts. The framework depends on none of them: a clone of
`renvor-rs/renvor` alone must build, test, package, and publish the crates. A build that
requires a private repository is a defect, not a configuration step.

**No unversioned copying.** A website repository may not contain copied framework source.
Synchronisation happens through a release tag, a published crate, or a digest-addressed
artifact, recorded in automation.

**Full detail is in PLAN.md Section 26**, which this record decides.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Everything in the public framework repository** (monorepo) | Puts brand assets under a repository that declares `MIT OR Apache-2.0`, making an unintended licensing claim. Publishes deployment configuration describing the live origin. Couples every website copy fix to the framework's full verification sequence, which is slow by design. Enlarges what a reviewer must read to trust the framework. |
| **Everything public, in four public repositories** | Solves coupling but not the two disclosure problems: brand licensing still needs its own decision, and `renvor-infra` would publish an ingress and namespace map of a server that also hosts unrelated production workloads. |
| **Two repositories — public framework, one private "web" repository** | Fewer moving parts, but re-creates the coupling inside the private repository: a landing copy change would run documentation and cluster checks, and a cluster change could ship a landing change. Failure isolation is the point of the split. |
| **Websites in the framework repository, deployment private** | Splits the smallest concern and leaves the two largest coupled. Brand licensing problem remains unsolved. |
| **Documentation in the framework repository, landing and deploy private** | Superficially attractive — documentation is closest to the code. Rejected because the production documentation site carries versioning, search indexing, a Node toolchain, and a release cadence that has no reason to gate a crate release. The *rustdoc* API reference stays in the framework, which is the part that genuinely belongs there. |

## Consequences

### Contribution and transparency — the real cost

**An outside contributor cannot fix a typo on the website.** They can read the rendered
page, but not the source, and cannot open a pull request against it. This is a genuine
transparency loss and the strongest argument against this decision.

It is accepted for now because the project has one maintainer and no outside contributors,
so the cost is currently zero and the disclosure benefit is immediate. **It stops being
free the moment the project has contributors**, and the decision must be revisited then —
most likely by making `renvor-site` and `renvor-docs` public once brand licensing is
settled and deployment configuration has been separated from them. `renvor-infra` has the
strongest case for remaining private permanently.

This record should be reviewed for that reason at the first outside contribution, not on a
calendar.

### Release coupling and failure isolation

- A broken landing build cannot block a crate release.
- A framework release cannot break the documentation site, because the site pins the
  artifact it builds from and upgrades deliberately.
- A cluster outage does not affect crates.io availability, and vice versa.
- The cost: a framework change that *should* update the documentation now requires a second
  pull request in a second repository. Version-pinned consumption makes that visible rather
  than silent, but it is real friction and will occasionally be forgotten.

### Documentation source of truth

- **rustdoc API reference**: generated from the framework, from an immutable tag or the
  published crate. The framework owns it.
- **Prose documentation**: owned by `renvor-docs`.
- **The two are cross-linked and version-stamped**, exactly as the Phase 001 `docs/`
  directory already demonstrates.
- docs.rs and crates.io remain fully supported. The private site never becomes a
  prerequisite for publishing.

### Infrastructure security

Keeping `renvor-infra` private does not make the cluster secure; it removes a free map.
The origin server also hosts unrelated production workloads, which raises the cost of
publishing its topology from "low" to "affects third parties". Security still rests on the
controls in ADR-0006, not on the repository being private.

### Rollback

Each property rolls back independently by redeploying its previous known-good image digest,
recorded in release evidence. Rollback is not a rebuild and not a revert-and-wait.

### Ownership

All four repositories are owned by Ahmed Anbar, which is the same single-maintainer
concentration recorded in W-001 and W-002. Splitting repositories does not split
accountability, and no additional waiver is created by this record.

## Migration timing for the Phase 001 `docs/` directory

**The Phase 001 `docs/` directory stays in the framework repository for the remainder of
Phase 001.** Moving it now would invalidate the completed, dated T064–T069 evidence and
replace a verified artifact with an unverified one mid-phase.

Its status is explicit: **it is the documentation-platform proof required by FR-054 and
FR-056, not the production documentation site.**

To prevent two long-lived sources of truth, the migration has its own gate:

1. `renvor-rs/renvor-docs` is created (separate approval gate) and reproduces the Phase 001
   site's verified properties: local search, clean build, link checking, version stamping.
2. `docs.renvor.dev` serves it and is verified reachable.
3. **In the same change that stands up the replacement**, the framework repository's `docs/`
   directory is removed and verification step 8 is repointed at the API-reference build.
4. The two never coexist as *published* sites. A brief local overlap during the migration
   change is expected; a published overlap is a defect.

Until step 3 lands, `docs.renvor.dev` does not exist and the Phase 001 `docs/` site is not
published anywhere.

## Related decisions still required

**Website code and brand-asset licensing has NOT been decided and is not decided here.**
Brand assets in `Branding/brand-v7` are not covered by the framework's `MIT OR Apache-2.0`
grant. A separate decision record must state the licence for website code and the usage
terms for brand assets **before** `renvor-rs/renvor-site` receives its first content. Creating that
repository without it would repeat the licensing ambiguity this record exists to avoid.

## Compliance

| Authority | How this record satisfies it |
|---|---|
| Constitution — decisions recorded with alternatives | Five alternatives with stated rejection reasons |
| Constitution principle X | The transparency cost is stated plainly, not minimised; the security benefit of privacy is explicitly limited to "removes a free map" |
| FR-009 licence policy | Prevents brand assets from falling under a grant nobody decided |
| FR-054, FR-056 | Documentation platform and version-stamp properties are preserved through the migration gate |
| PLAN.md Section 26 | This record decides that section |

## Acceptance gate

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ **Met** — five alternatives recorded with rejection reasons, and the accepted costs are stated |
| 2 | Verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) | ✅ **Met 2026-08-12** — T086 complete: 77 of 79 items passed, 2 failed as genuine specification gaps (CHK048, CHK050), 0 weakened. No unresolved requirement affects the topology decision. CHK023 passed because the specification explicitly places brand-asset licensing outside the code grant; that separate decision is tracked as T098 and blocks repository creation, not this record. |
| 3 | All required CI and security checks passing | ✅ **Met 2026-08-11** — `verify (1.94.0)` 59s, `verify (stable)` 53s, `security` 43s, `docs` 40s, plus dependency review and CodeQL, all passing on `renvor-rs/renvor` |
| 4 | A dated review record stored with the ADR | ✅ **Met** — this section, dated 2026-08-12 |

**All four controls are met. This record is `accepted`.**

Reviewed by **Ahmed Anbar — self-review under W-002** on **2026-08-12**. This review is
**not independent** and must not be described as such, here or anywhere else. It is a
structured self-review operating under a recorded, time-bounded exception that expires on
2027-02-11 or when a qualified independent reviewer becomes available, whichever is first.
