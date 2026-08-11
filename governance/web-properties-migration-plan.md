# Web Properties Migration Plan

**Status**: Planning only — nothing has been created, pushed, or deployed
**Decided by**: [ADR-0005](../decisions/0005-web-properties-and-deployment-topology.md) (topology), [ADR-0006](../decisions/0006-production-hosting-and-edge-architecture.md) (hosting and edge)
**Authoritative plan section**: `PLAN.md` §26
**Owner**: Ahmed Anbar

This document defines what must be true before each private repository is created and each
site is deployed. Every item is a precondition, not a description of existing state.

---

## 1. `renvor-rs/renvor-landing` — the V7 landing page

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
| `https://docs.renvor.dev/getting-started` | **Host does not resolve** — no A record |
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

Two separate decisions are needed, recorded before `renvor-rs/renvor-landing` exists:

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

## 3. `renvor-rs/renvor-deploy` — Kubernetes deployment configuration

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

### 4.1 Unresolved — registry choice

GitHub Container Registry (pairs with GitHub OIDC, no long-lived credential) versus the
GitLab registry already running on the VPS (no external dependency, but couples Renvor to
another project's service and requires a long-lived pull credential).

**Not decided.** Must be settled before the private repositories are created.
