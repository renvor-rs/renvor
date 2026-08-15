# Web Properties Migration Plan

**Status**: Partly executed, **nothing deployed**. All four repositories exist, are public on
GitHub, and are canonical there. `renvor-rs/renvor-site` has commits;
`renvor-rs/renvor-docs` is **commit-empty**; `renvor-rs/renvor-infra` **has one commit**,
`aa52237f4af421e089c31cfe306faa5db7c25e08`, published 2026-08-15. **No site is deployed and no
image is published.** *(Status line corrected 2026-08-15. It previously read "the
infrastructure repository has no commits on either host", which stopped being true when
`renvor-infra` was published, and it pointed at §5 as current when §5 is now superseded. An
earlier line reading "Planning only — nothing has been created, pushed, or deployed" was
accurate when written and is superseded too.)*
**Decided by**: [ADR-0005](../decisions/0005-web-properties-and-deployment-topology.md) (topology), [ADR-0006](../decisions/0006-production-hosting-and-edge-architecture.md) (hosting and edge; **D12 revised the topology 2026-08-14; D13 supersedes D12 2026-08-15**)
**Authoritative plan section**: `PLAN.md` §26
**Owner**: Ahmed Anbar

> **How to read this document.** **§6 is the only section that states current state.**
> **Sections 1–5 are dated history** and are retained **unrewritten, apart from supersession
> banners added at section heads**, as the record of what was planned, verified, and decided at
> the time. Where §1–5 describe the repositories as private or as unbuilt, read them as
> describing 2026-08-11 to 2026-08-14; **no Renvor repository is private** and all four exist.
> No sentence inside §1–5 has been altered to match today.

This document defines what must be true before each site is deployed, and defined what must
be true before each repository was created. **Items in §1–4 are preconditions rather than
descriptions of existing state, except where a subsection is explicitly dated as an
observation** — §1.2 "What V7 actually is — verified 2026-08-11" is one such. **§5 and §6 are
descriptions of state**, dated 2026-08-14 and 2026-08-15 respectively; §6 is the current one.
**ADR-0006 was accepted on 2026-08-15**, once T106 closed.

---

## 1. `renvor-rs/renvor-site` — the V7 landing page

### 1.1 Source of truth

| Item | Path today |
|---|---|
| Landing implementation | `Branding/landing-v7` |
| Brand identity | `Branding/brand-v7` |

Both are currently inside the framework working tree but **excluded from the framework
publish set** (`Branding/` is ignored by `.gitignore` under the T009 decision). They are not
in any commit of `renvor-rs/renvor` and must never be.

### 1.2 What V7 actually is — verified 2026-08-11

An audit of `Branding/landing-v7` established the following, which corrects several
assumptions:

| Property | Observed |
|---|---|
| Framework | **Docusaurus 3.10.2** with `docs: false, blog: false` — a single-page site, not a bespoke React app |
| Language | TypeScript 5.9.3, `tsc --noEmit` strict typecheck script present |
| Package manager | **pnpm 11.21.0**, `pnpm-lock.yaml` (11,872 lines), `pnpm-workspace.yaml` |
| Node requirement | **`engines.node >= 24.0`** |
| Animation | GSAP 3.15.0 with `@gsap/react` 2.1.2 |
| Icons / fonts | `lucide-react`, self-hosted `@fontsource-variable` (Outfit, Geist Mono) |
| Source files | 4: `src/components/LandingPage.tsx`, `src/pages/index.tsx`, `src/pages/index.module.css`, `src/css/custom.css` |
| Static assets | 3 V7 SVGs (mark, dark mark, favicon) |
| Package name | `renvor-runtime-spectrum` — **should be renamed** to something matching the repository |

### 1.3 ⚠️ Node version conflict — must be resolved before migration

| Repository | Node policy |
|---|---|
| `renvor-rs/renvor` (framework, Phase 001 T065) | **22** (`.nvmrc`) |
| `Branding/landing-v7` | **≥ 24** (`engines.node`) |

These are different repositories, so different policies are legitimate — but the landing
repository must carry its **own** `.nvmrc` pinning Node 24, and CI must use it. Inheriting
the framework's Node 22 policy would fail the frozen install. This machine runs Node 22.12.0
by default; V7 validation required installing Node 24 separately.

### 1.4 Migration inclusion list

**Include:**

```
docusaurus.config.ts   package.json   pnpm-lock.yaml   pnpm-workspace.yaml
tsconfig.json          README.md      PRODUCT.md
src/                   static/img/    (V7 SVGs only)
```

Plus, added during migration: `.nvmrc` (Node 24), `.gitignore`, `LICENSE` for website code,
brand-asset usage terms, and CI workflows.

**Exclude — must not be committed:**

| Excluded | Reason |
|---|---|
| `node_modules/` | Reconstructed from the frozen lockfile |
| `.docusaurus/` | Build cache |
| `build/` | Build output |
| `inspection/` (6 PNG screenshots) | Local inspection artifacts. **Not retained** — they are not release evidence, and retaining them would ship an undated visual record that drifts from the live site |
| Temporary files, local caches | Never source |
| Credentials, `.env*` | Never, in any repository |
| `Branding/brand-v1`…`v6`, `landing-v1`…`v6` | Superseded. Only V7 migrates |

### 1.5 Properties that must be preserved and re-verified after migration

| Property | Verified 2026-08-11 in place |
|---|---|
| Frozen install (`pnpm install --frozen-lockfile`) | ✅ succeeded, lockfile unchanged |
| Strict typecheck (`tsc --noEmit`) | ✅ exit 0 |
| Production build | ✅ SUCCESS |
| Reduced-motion handling | ✅ 2 CSS `prefers-reduced-motion: reduce` blocks, **and** GSAP guarded by `matchMedia('(prefers-reduced-motion: no-preference)')` — animation is opt-in on motion preference, not merely dampened |
| Light/dark themes | ✅ `defaultMode: 'light'`, `respectPrefersColorScheme: true`, switch enabled |
| Responsive layout | ✅ 7 `@media` rules; breakpoints 1120 / 996 / 720 px |
| Canonical metadata | ✅ `url: 'https://renvor.dev'` — correct canonical domain |
| Sitemap | ✅ generated at build |
| V7 logo/favicon assets | ✅ 3 SVGs present |

### 1.6 ⛔ BLOCKING — release-state honesty audit failed

**The V7 landing page must not be deployed publicly in its current state.**

Audited 2026-08-11 against `PLAN.md` §26.6. Two independent failures:

**(a) It presents unavailable capabilities in the present tense.** Visible copy includes:

> "Everything needed to start." · "Questions adapt to previous answers. Invalid combinations
> stop before generation." · "Authentication is generated across the stack." · "Add RBAC and
> future capabilities to existing projects through a versioned package contract." · "Build
> the complete application."

Commands `renover new` and `renover add` are shown as usable. **Neither exists.** The
`renvor` crate is not published (`crates.io/api/v1/crates/renvor` → HTTP 404), so no
`renover` binary is installable by anyone.

**(b) It contains no development-status disclosure.** A search for `pre-release`,
`in development`, `alpha`, `beta`, `preview`, `coming soon`, or `not yet` across `src/`
returns **zero matches**. `PLAN.md` §26.6 requires public content to state clearly that
Renvor is in development until REST 1.0 ships.

**Dead or misleading CTA targets:**

| Target | Reality 2026-08-11 |
|---|---|
| `https://crates.io/crates/renvor` | **HTTP 404** — crate not published |
| `https://docs.renvor.dev/getting-started` | **Host did not resolve** — no A record. **Updated 2026-08-12**: the host now resolves (DNS-only A record to the origin), but nothing is served — the origin presents only the Traefik default certificate and the path still does not exist |
| `https://github.com/renvor-rs/renvor` | Exists but **empty** — nothing pushed yet |

**Required before deployment** (tracked as T089–T091):

1. Add a prominent, non-dismissible development-status notice.
2. Re-word every present-tense capability claim to state its actual status, or label the
   capability as planned where it appears.
3. Remove or clearly mark installation commands until the crates are publicly installable.
4. Point every CTA at a resolving destination, or remove it.
5. Re-audit immediately before deployment, because release state changes.

This is a gate, not a preference. Deploying (a) would tell every visitor the framework does
something it cannot do.

### 1.7 Required links

- Documentation → `https://docs.renvor.dev`
- Source → `https://github.com/renvor-rs/renvor`
- Security → the `SECURITY.md` private reporting path
- Support → the published support policy
- Versioned release information → once releases exist, not before

### 1.8 ⚠️ Licensing decision required before the repository is created

**Brand assets are not covered by the framework's `MIT OR Apache-2.0` grant.**

`Branding/brand-v7` contains a logo mark, a dark variant, a favicon, and
`BRAND_IDENTITY.md`. Placing them in a repository without an explicit licence either leaves
them unlicensed or implies the framework's permissive grant — which would allow anyone to
use the Renvor mark for anything, including implying endorsement.

Two separate decisions are needed, recorded before `renvor-rs/renvor-site` receives its first content:

1. **Website code licence** — the Docusaurus configuration, components, and CSS.
2. **Brand asset usage terms** — trademark-style terms permitting nominative reference while
   reserving the mark. Explicitly **not** a permissive software licence.

---

## 2. `renvor-rs/renvor-docs` — the production documentation site

### 2.1 Relationship to the Phase 001 `docs/` directory

The Phase 001 `docs/` directory in the framework repository is the **documentation-platform
proof** required by FR-054 and FR-056 — not the production site. Migration timing and the
single-source-of-truth gate are defined in ADR-0005 and `PLAN.md` §26.12. Until that gate
passes, `docs.renvor.dev` does not exist and the Phase 001 site is published nowhere.

### 2.2 Requirements before the site is deployed

| Requirement | How it is satisfied |
|---|---|
| Versioned documentation | Docusaurus versioned docs, one version per published minor release; enabled at the first `0.1.0`, not before (ADR-0004) |
| Local/self-hosted search | `@easyops-cn/docusaurus-search-local` — index built at compile time, ships with the site, no query data leaves the reader's browser |
| Clean production build | `npm ci && npm run build` with `onBrokenLinks: throw` |
| Link checking | lychee over built output with `--root-dir`; exclusions individually justified with removal conditions |
| Accessible navigation and search | Keyboard-reachable navigation and search; verified before deployment |
| Canonical URLs | `url: 'https://docs.renvor.dev'` |
| Sitemap and robots | Sitemap generated; **`robots.txt` must be added** — the Phase 001 site has none |
| API reference integration | Generated from an **immutable framework artifact** (release tag or published crate), never a moving branch; the built tag is recorded in the site |
| Tested code examples | Examples compiled in CI, not pasted |
| Release-version selector | Docusaurus version dropdown once versioning is enabled |
| Support/compatibility status | Visible on every version, sourced from the support policy |
| No undocumented copying | Framework content enters only through a recorded, versioned automation step |
| Reproducible build | Committed `package-lock.json`; `npm ci`, never `npm install` |
| Preview builds | Preview environment separate from production; a preview cannot mutate production |
| Rollback | Redeploy the previous image digest |

### 2.3 Hard constraint — the framework must never depend on this repository

`cargo build`, `cargo test`, `cargo package`, and `cargo publish` must succeed from a clone
of `renvor-rs/renvor` alone. Rust API documentation and crate metadata stay fully
docs.rs- and crates.io-compatible. This private repository is an addition, never a
prerequisite.

---

## 3. `renvor-rs/renvor-infra` — Kubernetes deployment configuration

Contents, per ADR-0006: namespace, Deployment, Service, Ingress, NetworkPolicy, and
ServiceAccount manifests for both sites; Cloudflare configuration documentation; runbooks
for deployment, rollback, certificate renewal, and edge-outage recovery.

**Never contains:** application source, plaintext secrets, kubeconfig files, registry
credentials, or Cloudflare tokens.

---

## 4. Delivery design common to all three private repositories

| Control | Requirement |
|---|---|
| Branch protection | `main` protected; pull request required; required checks; no bypass, including administrators |
| Approvals | Single-maintainer gap already recorded as W-001; no new waiver is created |
| Workflow permissions | Top-level `contents: read`; elevation only on the job that needs it |
| Third-party actions | Pinned to full 40-character commit SHAs with a trailing version comment |
| Dependency install | Frozen: `pnpm install --frozen-lockfile` / `npm ci` |
| Checks | Typecheck, lint, production build, accessibility, link check, container scan |
| Images | Private registry; signed; SBOM and provenance; digest-pinned |
| Environments | Separate preview and production; production protected with a named approver |
| Credentials | Prefer OIDC short-lived identity. Where unavailable: minimum scope, protected environment secret, recorded rotation schedule, documented revocation, named owner — decided **before** creation |
| Log hygiene | No secret value printed in any log |
| Rollback | Explicit, by previous digest, rehearsed before first production deployment |
| Dependency updates | Reviewable pull requests; no unreviewed floating updates |
| Release evidence | Digest, signature, SBOM, provenance, scan result, and the previous digest for rollback |

### 4.1 ~~Unresolved — registry choice~~ — RESOLVED 2026-08-12 (T099)

~~GitHub Container Registry (pairs with GitHub OIDC, no long-lived credential) versus the
GitLab registry already running on the VPS (no external dependency, but couples Renvor to
another project's service and requires a long-lived pull credential).~~ ~~**Not decided.**
Must be settled before the private repositories are created.~~

**Decided 2026-08-12 — GitHub Container Registry (`ghcr.io`)**, recorded as ADR-0006 D7 and
`PLAN.md` §26.4. Publishing uses the workflow's short-lived `GITHUB_TOKEN` with
`contents: read` and `packages: write` **on the publishing job only**; **this is not OIDC**,
and the "pairs with GitHub OIDC" phrasing struck through above was the error T099 corrected.
The deployment image is **publicly pullable**, so the cluster stores no `imagePullSecret`.

**The GitLab registry was rejected and stays rejected.** ADR-0006 D12 moves *infrastructure
source* to a private self-hosted GitLab instance on 2026-08-14, and that does **not** reopen
this question: the two grounds for rejection — a long-lived cross-system publishing
credential, and a registry that is unavailable in exactly the recovery scenario it is needed
in — are untouched by where the manifests are stored.

**Nothing is configured.** No package, image, workflow, or credential exists.

---

## 5. Topology revision — 2026-08-14 (ADR-0006 D12) — **SUPERSEDED 2026-08-15, see §6**

> **This section is dated history, not current state.** It records the hybrid topology that
> was operative from 2026-08-14 until 2026-08-15, when **ADR-0006 D13** replaced it with
> all-public GitHub. **Its body is retained byte-for-byte as written on 2026-08-14** — only
> this heading and this banner were added, and no word inside the section, its table, or its
> subsections was edited or annotated. **§6 states what is current.**
>
> **Statements below that are false today are corrected here rather than inside the preserved
> text. The list is illustrative, not exhaustive** — read every statement in §5 as describing
> 2026-08-14:
>
> - the table row "`renvor-infra` | **Private self-hosted GitLab** | Private | Destination
>   only — **not canonical until T114**" was true on 2026-08-14. **Superseded 2026-08-15**:
>   `renvor-rs/renvor-infra` is public on GitHub and canonical there.
> - "*T102, T106, T108, T111, T113, and T114 all remain open*" — **none of the six is open
>   today.** **T114 was cancelled** and **T106 and T113 were completed** on 2026-08-15;
>   **T102, T108, and T111 were transferred and remain non-completed**, which is not the same
>   as open. See §6.2 and §6.3.

Sections 1–4 above were written when all four repositories were planned as private GitHub
repositories. That model is superseded. **The sections above are retained as the dated record
of what was planned and verified at the time; this section states what is current.**

| Repository | Host | Visibility | Status 2026-08-14 |
|---|---|---|---|
| `renvor-rs/renvor` | GitHub | Public | Unchanged |
| `renvor-rs/renvor-site` | GitHub | **Public** | Source, review, and CI on GitHub |
| `renvor-rs/renvor-docs` | GitHub | **Public** | **Commit-empty.** No README, licence, `.gitignore`, or workflow |
| `renvor-infra` | **Private self-hosted GitLab** | Private | Destination only — **not canonical until T114** |

### 5.1 Why `renvor-docs` stays commit-empty

Two independent conditions, both open:

1. **Licence undecided.** §1.8 required two separate decisions — website code licence and
   brand-asset usage terms — before a repository receives its first content. `renvor-site`
   settled its own under **T098**. **`renvor-docs` has not**, and the documentation site will
   carry framework prose, generated API reference, and brand assets under a different mix than
   the landing page does.
2. **T108 does not yet permit migration.** The documentation toolchain carries the unresolved
   `image-size` advisories. **T108 is not altered by this section.**

Until both hold, **`framework/docs` remains authoritative**, nothing is copied, and
`docs.renvor.dev` does not exist. §2 above continues to define what must be true before the
site is deployed.

### 5.2 Infrastructure cutover is gated on T114

The manifests, runbooks, and edge configuration described in §3 are destined for the private
GitLab project. **They are not there, and they are not to be pushed there yet.** The local
`infra` README and assets remain uncommitted, blocked by content classification, licensing,
and **T114**.

T114 requires an **encrypted off-VPS backup** of the GitLab application and configuration, an
**exact-version isolated restore proof**, **matching repository refs and hashes** between the
original and the restored copy, a recorded **retention policy with RPO and RTO**, and
**separate human approval** before cutover. Until it passes, the GitHub `renvor-infra`
repository is preserved, private, and empty as a temporary recovery placeholder.

The concern is specific rather than procedural: infrastructure history stored on the same VPS
the infrastructure describes is unavailable in precisely the recovery scenario ADR-0006 D9
depends on — the same reasoning that rejected the GitLab registry under T099.

### 5.3 What this revision does not change

- **T099 and GHCR.** Public application images remain planned for GitHub Container Registry.
  The GitLab Registry is not used, and is disabled on the GitLab project.
- **T108.** Untouched.
- **The framework's independence.** §2.3 still holds: `cargo build`, `test`, `package`, and
  `publish` must succeed from a clone of `renvor-rs/renvor` alone.
- **Deployment.** Nothing here deploys anything. T102, T106, T108, T111, T113, and T114 all
  remain open, ADR-0006 remains `proposed`, and Phase 001 is not complete.

---

## 6. Topology revision — 2026-08-15 (ADR-0006 D13, supersedes D12)

**All four repositories are public on GitHub and canonical there.** §5 is superseded and
retained above as dated history. This section states what is current.

| Repository | Host | Visibility | Canonical | Status 2026-08-15 |
|---|---|---|---|---|
| `renvor-rs/renvor` | GitHub | Public | **Yes** | Unchanged |
| `renvor-rs/renvor-site` | GitHub | Public | **Yes** | Source, review, and CI on GitHub |
| `renvor-rs/renvor-docs` | GitHub | Public | **Yes** | **Commit-empty, unchanged** — §5.1 still governs |
| `renvor-rs/renvor-infra` | GitHub | **Public** *(2026-08-15)* | **Yes** | Published at signed commit `aa52237f4af421e089c31cfe306faa5db7c25e08` |

### 6.1 The infrastructure repository was published, not deployed

The local `infra` README and brand asset described in §3 were published to
`renvor-rs/renvor-infra` on 2026-08-15 as a single signed root commit containing exactly
three paths: `.gitignore`, `README.md`, and `assets/renvor-mark-v7.svg`.

The README was **rewritten for public release** before publication. Removed: the origin IPv4
address, component patch versions, authoritative nameserver names, the unrelated-namespace
inventory, dated server-audit evidence, and the detailed description of absent edge
protections. Retained: purpose, high-level architecture, the DNS-only decision, the
additive-and-reversible principle, the workload security baseline, the no-plaintext-secret
rule, and licensing. The brand mark was preserved byte-for-byte. **This is minimisation for a
newly public repository. It is not a claim that previously published framework history became
secret.**

**No Kubernetes manifest, deployment workflow, GitHub Actions workflow, credential, licence
file, CODEOWNERS, or dependency file was added.** Publishing the repository deployed nothing.

Protection, verified by read-back from GitHub: ruleset `20889836` (`main protection`,
enforcement `active`, target default branch, **zero bypass actors**) requiring pull requests
with zero approvals for the sole maintainer, conversation resolution, signed commits, and
linear history, while blocking force pushes and branch deletion. Secret scanning, push
protection, and vulnerability alerts are enabled; merge commits are disabled with squash and
rebase allowed.

### 6.2 The GitLab cutover was abandoned and T114 is cancelled

**§5.2's cutover never happened.** An encrypted off-VPS backup was created on 2026-08-14, but
the **exact-version isolated restore proof never completed and no restore result was
accepted**; **matching repository refs and hashes were never proven**; **no RPO or RTO figure
was measured**; and the **separate cutover approval was never granted**, because the cutover
was cancelled.

On **2026-08-15 the maintainer intentionally deleted** the local Phase 3 and Phase 4 GitLab
backup and evidence directory — **the maintainer's local backup directory** *(absolute path withheld 2026-08-15)*. **None of those local backup
artifacts is preserved.** This statement is scoped to that directory alone and makes no claim
about any unrelated backup held elsewhere.

**T114 is closed as cancelled / not applicable, not as passed.** D13 removes the gate by
removing its subject: infrastructure source lives on public GitHub, so no infrastructure
history sits on the VPS and no GitLab restore is required for Renvor recovery.

**What this does and does not guarantee.** Public GitHub plus local working copies provide
failure-domain separation for **Git repository content**. They do **not** preserve
GitLab-specific issues, variables, users, logs, packages, registry content, or any other
GitLab metadata, and no such claim is made. **No GitLab RPO or RTO guarantee is claimed.**

**Self-hosted GitLab was not deleted, decommissioned, or modified.** It simply is not part of
the Renvor topology, and no Renvor source-control, CI, registry, deployment, or recovery
process depends on it.

### 6.3 What this revision does not change

- **T099 and GHCR.** Public application images remain planned for GitHub Container Registry.
  The GitLab Registry is not used and remains rejected on the original T099 grounds.
- **T108 and the `renvor-docs` licence gate.** Untouched. §5.1 continues to govern.
- **The framework's independence.** §2.3 still holds.
- **Deployment.** Nothing here deploys anything. **T102, T108, and T111 remain non-completed
  and are transferred** to the deployment workflow and Phase 012; **T106 closed 2026-08-15**
  by maintainer ruling and **T113 closed 2026-08-15** on live re-verification; **ADR-0006 was
  accepted 2026-08-15**. No server, DNS, Cloudflare, Kubernetes, GHCR, or production state was
  modified. *(This bullet read "T102, T106, T108, T111, and T113 remain open, ADR-0006 remains
  `proposed`" when written on 2026-08-15; updated the same day as those closed.)*
