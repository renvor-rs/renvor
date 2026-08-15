# ADR-0006: Host the public sites on the existing k3s cluster, served directly from the origin with Cloudflare as DNS only

| Field | Value |
|---|---|
| **ID** | 0006 |
| **State** | `accepted` |
| **Reviewer** | `Ahmed Anbar — self-review under W-002` |
| **Review date** | 2026-08-15 |
| **Superseded by** | — |
| **Owner** | Ahmed Anbar |

> ## Revision 2026-08-12 — the edge model changed, by maintainer decision
>
> **This record originally specified a proxied Cloudflare edge with Full (strict) TLS,
> Authenticated Origin Pulls, and a Cloudflare redirect rule.** The maintainer has since
> ruled that Cloudflare is **authoritative DNS only** and that the proxy will not be enabled.
>
> D3, D4, D5, D10 and the `www` redirect decision are rewritten below to match. **The
> superseded text is not deleted** — each rewritten decision states what it previously said
> and why it changed, because the earlier reasoning is the evidence for the trade now being
> accepted knowingly rather than by omission.
>
> The corrective decision is tracked as **T110**. **T105 is not reopened and not rewritten**:
> it recorded a real decision, correctly, on the architecture in force at the time.

## Context

A read-only audit of the Hostinger VPS on 2026-08-11 (recorded in
`governance/phase-001-evidence.md` §3u) changed the question this record answers.

**The premise "install Kubernetes on the VPS" was wrong.** Kubernetes is already installed,
already serving production traffic, and already hosting unrelated third-party workloads.

| Fact | Value |
|---|---|
| Host | Ubuntu 26.04 LTS, kernel 7.0.0, x86_64 (KVM), 8 vCPU AMD EPYC 9354P |
| Memory | 31 GiB total, ~17 GiB available, **0 B swap** |
| Disk | 400 GB, 328 GB free (16 % used) |
| Kubernetes | **k3s v1.35.5+k3s1, already running**, single-node control-plane |
| Datastore | **SQLite** (k3s default), not etcd |
| Ingress | **Traefik 3.6.13** via Helm, klipper-lb `svclb` DaemonSet with hostPorts 80/443 |
| TLS | **cert-manager v1.20.2** with a working `letsencrypt-prod` ClusterIssuer, 6 certificates all `True` |
| Load | 28 pods running of 110 capacity; node at **3 % CPU, 42 % memory** |
| Existing namespaces | `attaa`, `codexhub`, `gitlab`, `portfolio`, `cert-manager`, plus system |
| Existing live sites | `ahmedanbar.dev`, `codexhub.ahmedanbar.dev` |
| Also running | Docker (separate from k3s) hosting GitLab CE and a BuildKit builder |
| Firewall | **`ufw` inactive** |
| Addresses | IPv4 `153.92.208.x`, IPv6 `2a02:4780:f:88ec::/48` |
| Backups | **No etcd snapshots (SQLite backend); no restic/borg/duplicity installed** |
| Cloudflare | *(observed 2026-08-11)* `renvor.dev` and `ahmedanbar.dev` both on Cloudflare nameservers; `renvor.dev` had **no A record yet**; `ahmedanbar.dev` resolves **directly to the origin IP** (not proxied). **Superseded 2026-08-12**: the maintainer manually created three DNS-only A records for `renvor.dev`, `docs.renvor.dev`, and `www.renvor.dev` — see `governance/phase-001-evidence.md` §3af. **Re-verified read-only 2026-08-12 (§3ai)**: all three resolve to `153.92.208.119` on both authoritative nameservers, no wildcard, no `AAAA`, **no `CAA` record yet** |

Two consequences dominate every choice below:

1. **This is a shared, live server.** Any change risks unrelated production services that
   belong to other projects. "Additive and reversible" outranks "ideal".
2. **The hard parts are already solved and proven.** Ingress, ACME certificate issuance, and
   LoadBalancer port binding all work today, with six certificates currently valid.

Upstream status verified 2026-08-11 against primary sources:

| Component | Installed | Latest upstream | Licence | Maintained |
|---|---|---|---|---|
| k3s | v1.35.5+k3s1 | v1.36.3+k3s1 (2026-08-04) | Apache-2.0 | yes |
| cert-manager | v1.20.2 | v1.21.1 (2026-07-29) | Apache-2.0 | yes |
| Traefik | 3.6.13 | v3.7.10 (2026-07-31) | MIT | yes |
| cloudflared | not installed | 2026.7.3 (2026-07-23) | Apache-2.0 | yes |

Each installed component is exactly one minor behind current. None is archived or
unmaintained.

## Decision

### D1 — Reuse the existing k3s cluster. Install no Kubernetes distribution.

Two new namespaces, `renvor-landing` and `renvor-docs`, are added to the running cluster.

> **These are Kubernetes namespace names, not repository names.** The companion repositories
> are `renvor-rs/renvor-site`, `renvor-rs/renvor-docs`, and `renvor-rs/renvor-infra` — all
> public on GitHub since D13. *(This read "the private repositories"; corrected 2026-08-15,
> and the namespace-versus-repository point it makes is unaffected.)* The
> namespace `renvor-landing` predates the `renvor-site` repository name and has deliberately
> **not** been renamed here: no namespace exists yet, and renaming cluster objects is a
> deployment decision rather than a documentation one. Whether the namespace should be
> renamed to `renvor-site` for consistency is left open for the deployment phase.

No distribution comparison is needed, because installing k0s, MicroK8s, RKE2, or upstream
kubeadm alongside a running k3s cluster on a single node would contend for ports 80/443,
CNI, and container runtime, and would risk five live namespaces to gain nothing. **The
correct lightweight distribution for this server is the one already running it.**

### D2 — Reuse Traefik and cert-manager. Add no second ingress controller.

The two sites are `Ingress` resources on the existing `traefik` IngressClass, with
certificates issued by the existing `letsencrypt-prod` ClusterIssuer. Both are proven on
this exact server by six currently-valid certificates.

### D3 — Cloudflare is authoritative DNS only. The proxy stays off. *(revised 2026-08-12)*

> **Previously**: "Cloudflare proxied, Full (strict), with origin authentication" — apex and
> `docs` proxied (orange cloud), TLS mode Full (strict), Authenticated Origin Pulls enforced
> at Traefik. **Superseded by maintainer decision, T110.**

- `renvor.dev`, `docs.renvor.dev`, and `www.renvor.dev` are **DNS-only** (grey cloud).
- **The Cloudflare proxy is not enabled**, now or as a planned later step.
- **No Cloudflare Tunnel.** No `cloudflared` daemon anywhere.
- **No Cloudflare Origin CA certificate** and **no Authenticated Origin Pulls.**
- **No wildcard record.** Every hostname is declared explicitly.
- Public TLS is issued **at the origin**, by the existing cert-manager against Let's Encrypt,
  for every deployed hostname.

The request path is therefore:

```text
Browser
  → public DNS from Cloudflare
  → Hostinger origin IP
  → Traefik on the existing k3s cluster
  ├── renvor.dev       → landing service
  ├── docs.renvor.dev  → documentation service
  └── www.renvor.dev   → permanent redirect to renvor.dev

cert-manager
  → Let's Encrypt
  → publicly trusted certificates for all deployed hostnames
```

**Cloudflare is not in the HTTP request path at all.** It answers DNS queries and nothing
else. No statement anywhere in this repository may describe Cloudflare as protecting,
caching, filtering, or terminating Renvor HTTP traffic while these records are DNS-only.

Full (strict) and Flexible are no longer alternatives to weigh: with the proxy off there is
no edge TLS leg to configure. The browser negotiates TLS directly with Traefik.

### D4 — The origin is directly exposed, and that is accepted, not mitigated *(revised 2026-08-12)*

> **Previously**: "Direct origin, not Cloudflare Tunnel — with the bypass closed at the
> origin", where D3's Authenticated Origin Pulls closed the origin-bypass hole. **With the
> proxy off there is no bypass to close, because there is no edge to bypass.** The control
> and the risk it answered both disappear together.

Cloudflare Tunnel remains rejected, and the reasoning below is unchanged and still sound:

| | Cloudflare Tunnel | Direct origin (**chosen**) |
|---|---|---|
| Origin IP exposure | Hidden | **Already public** — the IP serves `ahmedanbar.dev` directly today |
| Inbound ports | None needed | 80/443 already bound and in use by existing sites |
| New moving parts | `cloudflared` daemon, a new failure domain | None |
| New long-lived credential | **Yes** — a tunnel token stored in the cluster | No |
| Failure mode | Tunnel down ⇒ site down, even though origin is healthy | Origin fault only, diagnosable directly |
| Operational ownership | New daemon to monitor, update, and rotate | Reuses what is already monitored |
| Blast radius on this server | New DaemonSet/Deployment alongside 28 production pods | Two Ingress objects |

Tunnel's headline benefit — hiding the origin IP — **buys nothing here**, because the origin
IP is already published in DNS for `ahmedanbar.dev` and is trivially discoverable.

**What has genuinely changed is the honest description of the exposure.** Under the previous
decision the origin was reachable directly but that path was *intended to be* closed by mTLS.
Now there is no edge, so:

- **the origin IP is public and is the only server answering**, for every Renvor hostname;
- **no WAF, no edge rate limiting, no bot management, and no DDoS absorption** stands in
  front of Traefik;
- **abuse traffic reaches the origin**, and the origin is shared with five unrelated
  production namespaces plus Docker GitLab;
- resource limits and NetworkPolicy (D8) stop being defence-in-depth and become the
  **primary** containment for a Renvor workload under load.

**Rejected mitigation, still rejected: allow-listing Cloudflare IP ranges in `ufw`.** It was
rejected before because `ufw` is inactive on a shared production host. It is now also
*meaningless*: with the proxy off, traffic does not arrive from Cloudflare ranges.

**There is no recovery procedure to document for a Cloudflare outage of the request path**,
because Cloudflare is not in it. A Cloudflare failure degrades DNS resolution only, and
that is the same exposure every domain on any authoritative provider carries.

### D5 — Where each edge concern now lives *(revised 2026-08-12)*

> **Previously**: "Additional Cloudflare configuration" — HSTS, caching, security headers,
> rate limiting, and an origin-bypass test were all placed at the Cloudflare edge. **With the
> proxy off, every one of those except the DNS-layer items must move to the origin or be
> struck.** They do not survive by being written down in the wrong place.

**Still at Cloudflare — these are DNS-layer and do not require the proxy:**

| Item | Decision | Status |
|---|---|---|
| DNSSEC | Enable at the registrar/Cloudflare | **Not verified** — separate authorised action |
| CAA | Records permitting only the ACME CA that cert-manager uses; `iodef` to the security contact | **Absent** — confirmed read-only 2026-08-12, no `CAA` record exists on `renvor.dev`. Required before certificate issuance is constrained in any meaningful way |

**Moved to the origin — Traefik and the workload now own these:**

| Item | Decision | Owner |
|---|---|---|
| HSTS | **Only after** every deployed hostname serves valid TLS and has survived at least one renewal cycle. Enabling it early makes a TLS mistake unrecoverable for the `max-age`. No preload until then | Traefik response headers |
| Cache | Hashed static assets: long `max-age`, `immutable`. HTML: short TTL with revalidation, so a rollback is visible immediately. **There is no edge cache — every request is an origin request**, so cache headers now govern browser behaviour only | Workload response headers |
| Security headers | CSP, `Referrer-Policy`, `X-Content-Type-Options`, `frame-ancestors`. **CSP was validated 2026-08-14 against the exact tree `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`** (site pull request #3, merge `206cefdff74399d96f723a75d961fb8d700e0fd5`): a 434-byte candidate policy ran in full **Enforcement** — negative control 3/3, matrix 48/48 across three engines, both routes, both viewports, both themes, and both motion settings — with **zero application CSP violations**. GSAP ran without `unsafe-inline` or `unsafe-eval`; `Outfit Variable` and `Geist Mono Variable` were fetched from same-origin `/assets/fonts/...` resources under `font-src 'self'`, and although the candidate policy also allowed `data:` fonts, r4 did not establish that allowance as necessary. The policy does carry one hashed style-attribute allowance using `unsafe-hashes`, and `data:` for images and fonts. **T101 — RESOLVED 2026-08-14**, evidence §3at. **The middleware itself is still not written, configured, or enabled**, and the candidate policy's hashes are artifact-bound | Traefik middleware |
| Rate limiting | Both sites are static, so sustained POST volume is abuse by definition. **This is no longer defence-in-depth behind an edge; it is the only rate limit that exists** | Traefik middleware |

**Struck entirely:**

| Item | Why |
|---|---|
| Origin-bypass assessment and post-deployment bypass test | There is no edge to bypass. The test verified that the origin refused non-Cloudflare traffic; the origin must now **accept** it, so the test would assert the opposite of the intended behaviour |
| Cloudflare WAF, bot management, DDoS absorption | Not in the request path. **No document may claim these protect Renvor traffic** |

### D6 — Delivery: narrowly scoped deployment workflow, not GitOps

A GitOps controller (Flux/Argo CD) is rejected **for now**: it is a continuously running
in-cluster component with broad permissions, added to a shared server, to manage two static
sites. The complexity is not yet earned.

Instead, each deploying repository **will have** a deployment workflow that authenticates with
a narrowly scoped credential and updates a single Deployment's image **by digest**. *(Corrected
2026-08-15 — this read "each private repository **has** a deployment workflow", which was
wrong twice: no Renvor repository is private under D13, and **no deployment workflow exists in
any of them**. This is the design, not the state.)* This is
revisited if the number of deployed properties grows beyond about five, or when a second
maintainer joins.

### D7 — Images and supply chain: GitHub Container Registry *(decided 2026-08-12, T099)*

> **Previously**: "Private registry … image-pull authentication by a narrowly scoped,
> rotatable credential held as a cluster secret". **The registry is now chosen, and the
> pull-credential requirement is deliberately removed** — see the publication model below.

**Registry: GitHub Container Registry (`ghcr.io`).** The alternative was the GitLab registry
already running on this host, and it is rejected on two grounds recorded in full under
*Alternatives considered*.

**Publishing authentication — the workflow's own token, not a stored credential:**

- GitHub Actions publishes using the **short-lived `GITHUB_TOKEN` minted per workflow run**.
- The image-publishing job declares **least privilege: `contents: read` and
  `packages: write`**, and nothing else. Permissions are set on the job, not the workflow, so
  no other job inherits write access to packages.
- **No personal access token, deploy token, repository secret, or long-lived registry
  credential is created.** A credential that does not exist cannot leak, cannot be rotated
  late, and cannot outlive the person who made it.

> **This is not OIDC, and describing it as OIDC would be wrong.** `GITHUB_TOKEN` is an
> installation token that Actions injects into the run and revokes when the run ends. OIDC is
> a separate mechanism in which a workflow exchanges a signed identity token with an external
> provider for temporary credentials — it is how the crates.io trusted-publishing path works,
> and it is *not* what authenticates to GHCR. The two are easy to conflate because both avoid
> a stored secret; the distinction matters when someone later tries to configure a trust
> relationship that GHCR neither needs nor offers here.

**Pull authentication — none, by design:**

- The **production deployment image is publicly pullable.** GHCR package visibility is
  independent of repository visibility. *(Corrected 2026-08-15 — this concluded "so the source
  repositories stay private while the built artefact is public". **All four repositories are
  public under D13**, so that clause no longer describes anything. The independence itself is
  the load-bearing fact and is unchanged: it is what lets the image be public regardless of
  what the source does, and it would still hold if a repository became private again.)*
- Consequently **the k3s host needs no `imagePullSecret`**, and no registry credential is
  stored in the cluster at all.
- The image **will contain** only the built static site — HTML, CSS, JS **intended to be**
  served publicly at `renvor.dev`. **Once that site is deployed**, making the image public
  will disclose nothing that visiting the site would not. *(Corrected 2026-08-15 — this read
  "already served publicly at `renvor.dev`" and "visiting the site", both present tense.
  **No Renvor site is deployed.** Measured 2026-08-15, all three hostnames resolve to the
  shared origin and return **HTTP 404** over plain HTTP, and over HTTPS once validation is
  bypassed; against a public trust store the handshake fails on Traefik's default self-signed
  certificate — consistent with D11's note below. **Something answers; no Renvor content is
  served.** The argument is sound as a property of the design and unsound as an observation,
  so it is stated as the former.)*
- The trade is accepted knowingly: image *contents* and pull *counts* become public, and the
  image cannot be used as a private distribution channel. Both are acceptable for a static
  marketing and documentation site, and neither would be acceptable for an image carrying
  configuration, credentials, or unreleased material.

**Addressing:**

- Images are referenced **by immutable digest (`@sha256:…`), never by a mutable tag alone.** A
  tag may accompany a digest for human readability, but the digest is what deploys.
- Signed images with SBOM and provenance attestation; vulnerability scan before promotion.
- Minimal static-content base image; the sites are static HTML, CSS, JS.

**Nothing was configured in this pass.** No package, no workflow, no image, no infrastructure
change. Only the decision is recorded.

### D8 — Workload security baseline (all Renvor workloads)

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 10001
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  capabilities: { drop: ["ALL"] }
  seccompProfile: { type: RuntimeDefault }
```

Plus: explicit CPU/memory requests and limits on every container; readiness and liveness
probes; `automountServiceAccountToken: false`; a dedicated ServiceAccount and Namespace per
site; NetworkPolicy restricting ingress to Traefik and egress to DNS only; no privileged
containers; no host filesystem mounts; no hostNetwork.

**Memory limits are not optional on this host: swap is 0 B**, so memory pressure produces an
OOM kill rather than degradation — and the victim could be a neighbouring production pod.

### D9 — Backup and disaster recovery

The audit found **no backup tooling and no etcd snapshots** — k3s uses SQLite here, so the
familiar `k3s etcd-snapshot` path does not apply.

For the Renvor properties specifically this is low-severity, because both sites are
**stateless**: recovery is redeploying a known digest from a private registry, and the
manifests live in `renvor-infra`. Recovery therefore depends on GitHub and the registry,
not on server state.

**The absence of a general server backup is nonetheless recorded as a pre-existing risk
affecting the neighbouring workloads.** It is outside Renvor's remit, and this record does
not claim to fix it, but it must not go unstated: `/var/lib/rancher/k3s/server/db/state.db`
holds all cluster state for five production namespaces with no observed snapshot schedule.

### D10 — Monitoring, logs, updates *(revised 2026-08-12)*

> **Previously**: "probes plus Cloudflare edge analytics are the initial signal". **Edge
> analytics do not exist without the proxy.** Removing the proxy removed the only traffic
> visibility this record had planned for the first deployment.

`metrics-server` is already present. Renvor adds no monitoring stack in the first
deployment. **The initial signal is now Kubernetes probes and Traefik access logs only** —
there is no external vantage point reporting on requests that never reach the cluster.

This is a real reduction in observability, recorded rather than glossed: an outage in which
the origin is unreachable produces **no signal at all**, because nothing outside the origin
is watching. A metrics stack or an external uptime check is correspondingly more valuable
than it was under the previous decision, and is the first thing to add once there is
something deployed to observe.

Log retention uses existing node journald policy. Component updates follow upstream releases
through the companion repositories' dependency policy, applied deliberately — never
automatically on a shared production host. *(Read "the private repositories'" before
2026-08-15; no Renvor repository is private under D13 and the policy is unchanged.)*

### D11 — Traefik serves the permanent `www` redirect *(revised 2026-08-12; supersedes the T105 decision)*

**`www.renvor.dev` → `https://renvor.dev`, HTTP 301, preserving path and query string,
implemented as a Traefik router and redirect middleware on the origin.**

> **Previously recorded as a second section numbered `D6`** — a numbering collision with the
> delivery decision above, corrected here to `D11`. Its content said **Cloudflare** served
> the redirect as an edge rule. That is no longer possible: **a Cloudflare redirect rule
> applies only to a proxied record**, and D3 keeps every record DNS-only.

The rejected alternative and the chosen one have swapped places, and the original reasoning
is worth preserving because it names exactly what this now costs:

| | Cloudflare edge rule *(previously chosen, now impossible)* | Traefik on the origin *(chosen)* |
|---|---|---|
| Works while the origin is down | **Yes** | **No** — a redirect is only as available as the origin |
| Certificate needed for `www` | None | **Yes** — cert-manager must issue for `www.renvor.dev`, a hostname serving no content |
| In version control | No — manual edge configuration, prone to drift | **Yes** — a manifest in `renvor-infra`, reviewed like any other change |
| Requires the proxy | **Yes** — disqualifying under D3 | No |

**What this costs, stated plainly:** the redirect now consumes a certificate and an Ingress
for a hostname whose only job is to redirect, and it cannot answer while the origin is down.
Both were the stated reasons for choosing Cloudflare originally. They are accepted now
because the alternative does not exist under a DNS-only edge.

**What it gains:** the redirect becomes reviewable, version-controlled infrastructure rather
than a manual edge rule that no repository describes.

Consequences:

| Consequence | Detail |
|---|---|
| Certificate | cert-manager **must** issue a Let's Encrypt certificate for `www.renvor.dev`, or the redirect serves a TLS error instead of a redirect |
| Current visitor experience | Unchanged and still broken: `www.renvor.dev` resolves to the origin and receives the Traefik **default self-signed certificate**, so a browser shows a warning. Expected until deployment |
| Availability | The redirect shares the origin's fate. There is no independent path |
| Status | **Not implemented.** No Traefik router, middleware, Ingress, or certificate exists for `www`. Creating them is a separate authorised action, and belongs to `renvor-infra` |

### D12 — Hybrid source-control topology: GitHub for the three public properties, private self-hosted GitLab for infrastructure *(decided 2026-08-14; supersedes the all-GitHub, all-private repository model in ADR-0005 and `PLAN.md` §26.1)* — **SUPERSEDED 2026-08-15 by D13**

> **This decision is superseded and is retained as dated history, not as current state.**
> It was accepted into `main` on 2026-08-14 and was the operative topology until
> 2026-08-15, when **D13** replaced it with all-public GitHub. The text below is preserved
> **byte-for-byte as it was accepted on 2026-08-14**, because it is the evidence for why the
> hybrid topology was chosen, and because the reasoning it records — that infrastructure
> configuration describes the shape of a live system — survives the change of host. Only this
> heading and this banner were added; **no word inside the decision body, its tables, or its
> consequences was edited, qualified, or annotated**, so `git diff` against the 2026-08-14
> text shows no change within it. **Read every statement below as describing 2026-08-14, not
> today.**
>
> **Statements below that are false today are corrected here rather than inside the preserved
> text. The list is illustrative, not exhaustive** — every statement in this decision describes
> 2026-08-14, and any of them may have been overtaken:
>
> - "*T113 and T114 stay open*" — **T114 was cancelled on 2026-08-15, not passed.** T113
>   remains open. This is the statement a reader is most likely to act on.
> - "*Infrastructure configuration moves to a private, self-hosted GitLab instance*", and the
>   table row "*`renvor-infra` | **Private self-hosted GitLab** at `gitlab.ahmedanbar.dev` |
>   **Private** | …*" — **the move never
>   happened.** Per the T114 cancellation record, the cutover was abandoned before it ran.
>   *(Scope: this rests on the project's own dated record, not on a fresh inspection — the
>   GitLab instance was deliberately not accessed. No claim is made about its contents.)*
> - "*Infrastructure repository is not yet canonical … GitLab is not canonical for
>   infrastructure until T114 passes*" — true on 2026-08-14. **Superseded 2026-08-15 by
>   D13**: `renvor-rs/renvor-infra` is public on GitHub and canonical there, and GitLab is
>   canonical for nothing.
> - "*Branch protection, required checks, and pull-request review stay on GitHub*" — this
>   described where those controls live, not that every repository had them. It did not then
>   and does not now. See D13 and `PLAN.md` §26.1 for the observed per-repository state.

**Application source, review, and CI live on GitHub. Infrastructure configuration moves to a
private, self-hosted GitLab instance.** The four repositories no longer share one host or one
visibility.

| Repository | Host | Visibility | Role |
|---|---|---|---|
| `renvor-rs/renvor` | GitHub | **Public** | Framework source, releases, governance. Unchanged |
| `renvor-rs/renvor-site` | GitHub | **Public** *(changed 2026-08-14)* | Landing source, review, CI |
| `renvor-rs/renvor-docs` | GitHub | **Public** *(changed 2026-08-14)* | Production documentation site. **Commit-empty** — see below |
| `renvor-infra` | **Private self-hosted GitLab** at `gitlab.ahmedanbar.dev` | **Private** | Kubernetes manifests, ingress and TLS configuration, runbooks |

**Why the split.** The three application properties benefit from being public: their CI is
reviewable, their dependency and secret scanning are available, and nothing in them is
confidential — the landing page's entire content is intended to be read by strangers.
Infrastructure configuration is the opposite. It describes the shape of a live system, and
keeping it on an instance the maintainer controls removes it from a third party's blast
radius without pretending it is secret from that third party.

**This changes where infrastructure source lives. It changes nothing about where images go.**
**D7 stands unmodified**: public application images remain planned for **GitHub Container
Registry**, published with the workflow's short-lived `GITHUB_TOKEN`. The GitLab Registry is
**not** used, and the two grounds on which it was rejected under T099 — the long-lived
cross-system publishing credential, and a registry that is unavailable in exactly the
recovery scenario D9 depends on it for — are unchanged by this record and still hold. Moving
infrastructure *source* to that host does not move the *registry* to it.

**`renvor-rs/renvor-docs` is public but deliberately commit-empty.** It has no commits, and it
gets none until two independent conditions are met: its **licence is decided** — the website
code licence and brand-asset terms question recorded in the migration plan §1.8, which
`renvor-site` settled under T098 and this repository has not — and **T108 permits migration**,
because the documentation toolchain carries the unresolved `image-size` advisories. Until
both hold, `framework/docs` stays authoritative and nothing is copied. Creating the empty
public repository now reserves the name and makes the intent legible; it does not start the
migration, and **T108 is not altered by this record.**

**GitHub remains the review and CI surface for all three application properties.** Branch
protection, required checks, and pull-request review stay on GitHub. GitLab hosts no
application CI, and no GitLab CI, runner, registry, or Pages feature is enabled for
`renvor-infra`.

Consequences:

| Consequence | Detail |
|---|---|
| Two hosts to operate | Account, access, and backup concerns now exist in two places rather than one. The GitLab side is a single-tenant instance the maintainer administers |
| GitLab administrators retain access | Instance administrators inherently retain administrative access to every project. With a single-maintainer instance this is the same person, but it is a property of the deployment, not an absence of one |
| Infrastructure repository is not yet canonical | The GitHub `renvor-infra` repository is preserved, private, and empty as a temporary recovery placeholder. **GitLab is not canonical for infrastructure until T114 passes** |
| Backup surface changed | Infrastructure history would live on the same VPS the infrastructure describes — the failure mode D9 warns about. **T114 exists precisely to close this** and requires encrypted off-VPS backup with a proven restore before cutover |
| Public site source | `renvor-site` source is now world-readable. The secret and metadata audits recorded in the evidence ledger were run before the change to establish that nothing sensitive was exposed by it |
| Status | **Not complete.** This record stays `proposed`; T113 and T114 stay open; Phase 001 is not complete; no Renvor 1.0 claim is made or implied |

### D13 — Public GitHub is canonical for all Renvor repositories *(decided 2026-08-15; supersedes D12)*

**All four Renvor repositories are public on GitHub, and GitHub is canonical for every one of
them.** The hybrid topology recorded in D12 on 2026-08-14 is superseded. Private self-hosted
GitLab is no longer part of the Renvor source-control topology.

| Repository | Host | Visibility | Canonical | State |
|---|---|---|---|---|
| `renvor-rs/renvor` | GitHub | Public | **Yes** | Framework source, releases, governance |
| `renvor-rs/renvor-site` | GitHub | Public | **Yes** | Landing source, review, CI |
| `renvor-rs/renvor-docs` | GitHub | Public | **Yes** | **Commit-empty**, unchanged — still gated on its licence decision and T108 |
| `renvor-rs/renvor-infra` | GitHub | **Public** *(changed 2026-08-15)* | **Yes** | **Reserved for** Kubernetes deployment configuration and public operational documentation. **Currently a `README.md`, a `.gitignore`, and the brand mark — three files, no manifest**, at signed commit `aa52237f4af421e089c31cfe306faa5db7c25e08` |

**GitHub is the source, review, and future CI surface for all four.** No Renvor process reads
from, writes to, or authenticates against a GitLab instance.

**Protection is required of every repository; it is not yet present on every repository.**
*(Corrected 2026-08-15 — a first draft of this decision asserted that branch protection,
required checks, and pull-request review already lived on GitHub "for every repository". That
was false for two of the four and is retracted.)* Observed 2026-08-15:

| Repository | `main` protection | Required status checks |
|---|---|---|
| `renvor-rs/renvor` | classic protection — pull request, strict checks, administrators included, conversation resolution, force push and deletion blocked | **4** — `verify (1.94.0)`, `verify (stable)`, `security`, `docs` |
| `renvor-rs/renvor-site` | classic protection — same controls | **5** — `build`, `accessibility`, `links`, `dependencies`, `container` |
| `renvor-rs/renvor-infra` | ruleset `20889836` — pull request, signed commits, linear history, conversation resolution, force push and deletion blocked, zero bypass actors | **none** — no CI exists in the repository |
| `renvor-rs/renvor-docs` | **none** — commit-empty, so no `main` branch exists to protect; no protection and no ruleset are configured | **none** — no commits, no workflows |

**Closing the two gaps is future work and is not claimed by this record.** `renvor-infra`
cannot have required checks until it has CI; `renvor-docs` cannot have a protected branch
until it has a commit, which is itself gated on its licence decision and **T108**.

**`renvor-rs/renvor-docs` remains the public canonical *destination* for the production
documentation site and nothing more.** It is commit-empty, and **`framework/docs` remains the
authoritative documentation content until the separately reviewed migration** permitted by
T108 and its licence decision. Naming a destination is not migrating to it.

**`renvor-infra` publication, 2026-08-15.** The repository was published from a single signed
root commit containing exactly three paths — `.gitignore`, `README.md`, and
`assets/renvor-mark-v7.svg`. The README was rewritten for public release before publication:
the origin IPv4 address, component patch versions, authoritative nameserver names, the
unrelated-namespace inventory, dated server-audit evidence, and the detailed description of
absent edge protections were removed. The brand mark was preserved byte-for-byte. **No
Kubernetes manifest, deployment workflow, GitHub Actions workflow, credential, or licence file
was added.** Gitleaks in redacted mode reported zero findings across all four repositories,
scanning both complete Git history and untracked working-tree files.

| Evidence | Value |
|---|---|
| Initial commit | `aa52237f4af421e089c31cfe306faa5db7c25e08`, signature `verified: true`, zero parents, zero trailers |
| Committed tree | `7aaf7705946b0a91b7571167adf4aef1c4ba89f4` |
| Ruleset | id `20889836`, name `main protection`, enforcement `active`, target default branch, **zero bypass actors** |
| Active rules | pull request required (0 approvals, sole maintainer), conversation resolution required, signed commits required, linear history required, force pushes blocked, branch deletion blocked |
| Merge methods | squash and rebase allowed; merge commits disabled; automatic branch deletion disabled |
| Security features | secret scanning enabled, push protection enabled, vulnerability alerts enabled, dependency graph active |

**D7 stands unmodified.** Public application images remain planned for **GitHub Container
Registry**, published with the workflow's short-lived `GITHUB_TOKEN`. The GitLab Registry was
never used and remains rejected on the two T099 grounds. This record changes where
infrastructure *source* lives; it changes nothing about the registry.

**No Renvor deployment, CI, registry, or recovery process depends on GitLab.** The dependency
D12 would have created was never established, because the cutover it required never happened.

**GitLab itself was not deleted, decommissioned, or modified by this decision.** The
self-hosted instance continues to exist and to serve whatever unrelated purposes its
maintainer chooses. What changed is that Renvor does not use it and does not depend on it.

**Nothing was deployed.** No server, DNS, Cloudflare, Kubernetes, GHCR, or production change
was made by this record. Every deployment gate that was open before it remains open.

#### D13 alternatives considered

**Four** topologies were genuinely available for `renvor-infra` — three active choices and the
option of doing nothing. They are compared on the axes that actually differ between them — the
failure domain of the history, what the repository discloses, what it costs to operate, and
what recovery obligation it creates. **Axes on which all four are identical are named as such
rather than used as false differentiators**: none of them changes the registry decision
(D7/T099 stand under all four), none of them deploys anything, and none of them alters T102,
T106, T108, T111, or T113.

| Option | What it gives | What it costs | Verdict |
|---|---|---|---|
| **A — Public GitHub** *(chosen)* | Git history lives outside the VPS's failure domain by construction, so no backup-and-restore gate is needed to make infrastructure history survivable. One host, one account model, one protection mechanism. Secret scanning and push protection apply. Review is legible to anyone, which matters for a project whose governance claims to be auditable. | **Infrastructure configuration becomes world-readable.** This is the real price, and it is the reason for the pre-publication minimisation recorded above. GitHub is a third party in the blast radius. The repository is a fourth surface to protect, and it currently has no CI and therefore no required checks. | **Chosen.** The disclosure cost is bounded and was reduced deliberately; the recovery benefit is structural rather than procedural. |
| **B — Private GitHub** | Same single-host operating model and the same failure-domain separation as A, without world-readable infrastructure configuration. Strictly closer to A than to C. | Free private repositories do not get the full protection feature set that this project relies on elsewhere — the ruleset and required-check tooling the other repositories use was **only observable and configurable on this account for public repositories**, which is exactly how T113's protection gap was originally discovered on a then-private `renvor-site`. Buying it would mean a paid plan, which is out of scope. It also keeps the review surface closed, which weakens the auditability claim the governance record makes. | **Rejected**, and it is the closest alternative. If disclosure ever proves to cost more than expected, this is the option to revisit — the migration back is a visibility toggle, not a re-hosting. |
| **C — Private self-hosted GitLab** *(the D12 status quo)* | Nothing leaves the maintainer's control. Infrastructure that describes a live system is not handed to a third party. This reasoning was correct on 2026-08-14 and is not repudiated. | **Puts the history of the system on the machine the history describes** — the exact failure mode D9 warns about, and the reason T114 existed. Making it safe required an encrypted off-VPS backup with a *proven* restore, matching refs and hashes, measured RPO/RTO, and a separate approval. **That proof was attempted and never completed.** It also doubles the account, access, and backup surface, and instance administrators inherently retain access. | **Rejected.** Not because the reasoning was wrong, but because the obligation it created was not met and the alternative removes the obligation instead of discharging it. |
| **D — Keep `renvor-infra` unpublished entirely** | Zero disclosure, zero new surface. | The configuration still has to live somewhere to be reviewed, and "on one laptop" is a worse failure domain than either A or C. It defers the decision without answering it. | **Rejected as a non-answer**, recorded because doing nothing was genuinely available. |

**Why A over B specifically.** The two differ only in disclosure. The decisive fact is that
the protection controls this project depends on — rulesets, required signatures, enforced
linear history, secret scanning with push protection — are available on public repositories on
the current plan and were verified active on `renvor-infra` after publication. Choosing B
would have traded a *verified* protection posture for an *unverifiable* one, in exchange for
hiding configuration that had already been minimised of every operationally sensitive value.

**This reverses an accepted record, and that is stated rather than glossed.** **ADR-0005**
concluded that "`renvor-infra` has the **strongest case for remaining private permanently**",
and it is still `accepted`. D13 reverses precisely that conclusion. ADR-0005's reasoning was
that deployment configuration "describes the attack surface of a live server" — a map of
ingress hostnames, namespace layout, and image references. **That reasoning is not repudiated;
it is answered on the facts.** The repository contains no such map: it holds a `README.md`, a
`.gitignore`, and the brand mark, and the README was minimised of every operationally
sensitive value before publication. **If and when real manifests are added, this trade must be
re-examined rather than assumed to carry over** — a repository of three descriptive files and a
repository of live ingress and namespace definitions are not the same disclosure. ADR-0005 is
marked partially superseded on visibility only; its four-repository separation stands.

**What the chosen option does not solve.** A does not preserve GitLab metadata, does not give
`renvor-infra` CI or required checks, does not deploy anything, and does not create any backup
guarantee for the VPS itself. Each of those remains exactly as open as it was.

Consequences:

| Consequence | Detail |
|---|---|
| One host, one visibility model | Account, access, and availability concerns exist in one place rather than two. The two-host operating cost D12 accepted is removed |
| Infrastructure configuration is world-readable | This is the substantive trade. It is accepted deliberately: the repository is minimised for public release, carries no manifest yet, and is governed by the no-plaintext-secret rule and an enforced `.gitignore`. **Content minimisation is not a claim that previously published framework history became secret** |
| Failure-domain separation for Git content | Repository content now exists on GitHub and in local working copies, which do not share the VPS's failure domain — the concern D9 and T114 were written about, **avoided** by not putting infrastructure history on the VPS in the first place. *("avoided", not "resolved", corrected 2026-08-15: two copies in two failure domains is **separation, not a backup**. There is no tested restore, no retention policy, and no recovery owner, and the GitHub copy sits in the same single account as everything else. The concern is sidestepped architecturally; it is not discharged by evidence)* |
| GitLab metadata is not preserved anywhere | Local clones plus GitHub protect **Git repository content only**. They do not preserve GitLab issues, variables, users, logs, packages, registry content, or any other GitLab-specific metadata. No claim is made that they do |
| The infrastructure repository is still empty of manifests | Publishing it does not deploy anything and does not close any deployment gate. T102, T106, T108, T111, and T113 are untouched |
| Status | **Accepted 2026-08-15**, once T106 closed — see the Acceptance gate. *(This row read "Not complete. This record stays `proposed` pending T106" when D13 was written on 2026-08-15; T106 was ruled on later the same day.)* **Acceptance is not deployment**: T102, T108, and T111 remain non-completed and transferred, and **no Renvor 1.0 claim is made or implied** |

#### What D13 is, and what authority it does and does not carry

**Separate the two things this record contains.**

**The topology is an observed live fact.** All four repositories are public on GitHub; the
visibility, the commit, the tree, the ruleset, and the security settings were read back from
GitHub after the change and are reproducible by anyone with the URLs. That does not depend on
this record being accepted — it would remain true if this record were rejected tomorrow.

**The choice of that topology is the maintainer's direction, recorded here.** Ahmed Anbar
authorised the visibility change and the abandonment of the GitLab cutover.

**Neither, by itself, made this record normative.** *(This paragraph stated that ADR-0006 was
`proposed` pending T106. **T106 was resolved by maintainer ruling on 2026-08-15** — evidence
§3ay — and this record was accepted the same day. The distinction it drew is preserved because
it still governs how the record should be read.)*

**The topology would remain true even if this record were rejected**, because it is an
observation; and **the record is now `accepted`**, so its decisions may be cited as settled
architecture. Those are two different kinds of authority and the second is the weaker claim:
acceptance rests on a **non-independent self-review under W-002**, which is a recorded
exception, not a substitute for independent review. `PLAN.md` §26.1 continues to record the
topology on the strength of the observation, so nothing there depends on this record's status.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Install a fresh lightweight distribution (k0s, MicroK8s, RKE2, kubeadm) | Would contend for ports 80/443, CNI, and the container runtime with a live cluster serving five namespaces. All risk, no benefit. |
| Skip Kubernetes; serve static files from nginx or Caddy on the host | Genuinely simpler for two static sites, and a reasonable choice on an empty server. Rejected here because it would need ports 80/443 that Traefik already owns, so it would mean dismantling or fronting the existing ingress — more disruptive than adding two Ingress objects to it. |
| Cloudflare Pages / Workers, no VPS at all | Removes the origin entirely and would be a strong choice for two static sites. Rejected because the maintainer's stated requirement is Kubernetes on the owned VPS, and because the framework will later need a real origin for dynamic examples. Recorded as the most credible alternative if operational load becomes a problem. |
| Cloudflare Tunnel | See D4 — solves a problem this origin does not have, at the cost of a daemon, a long-lived credential, and a new failure domain. |
| `ufw` allow-list of Cloudflare ranges | High blast radius on a shared server whose firewall is currently inactive. **Now also meaningless**: with the proxy off, traffic does not arrive from Cloudflare ranges. |
| GitOps controller (Flux, Argo CD) | Continuously running broad-permission component for two static sites. Not yet earned; revisit at scale. |
| **The GitLab registry already running on this host** *(rejected 2026-08-12, T099)* | Genuinely attractive — it exists, it is private, it costs no external dependency, and 328 GB is free. Rejected on two grounds. **First, the publishing credential**: GHCR is reachable from GitHub Actions with the run's own `GITHUB_TOKEN`, so no stored secret exists; publishing to the host's GitLab registry from GitHub Actions requires a **long-lived cross-system credential** held as a repository secret — the exact class of artefact the release process already worked to eliminate. **Second, the recovery loop**: D9 rests on recovery being "redeploy a known digest from a registry", which fails if the registry lives on the machine being restored. A registry on the origin is unavailable in precisely the scenario it is needed. |
| **Proxied Cloudflare edge with Full (strict) and Authenticated Origin Pulls** | **This record's original decision, rejected 2026-08-12 by maintainer ruling (T110).** It is the stronger security posture on the merits — it supplies a WAF, edge rate limiting, DDoS absorption, and a closed origin-bypass path, none of which the chosen architecture has. It is rejected on ownership grounds rather than technical ones: it puts the request path, the redirect, the caching policy, and the security headers inside a vendor console that no repository describes and no review covers. The chosen architecture keeps every one of those in version control, and pays for it in exposure. |
| Flexible or Full (non-strict) Cloudflare TLS | Moot under D3 — with the proxy off there is no edge TLS leg. Recorded because it was a live comparison under the original decision: Flexible sends plaintext to the origin, and Full (non-strict) accepts any certificate, making origin TLS decorative. |

## Consequences

**Accepted costs:**

- **Renvor becomes a tenant on a server it does not exclusively own.** A Renvor
  misconfiguration can affect `ahmedanbar.dev`, `codexhub`, `attaa`, and GitLab. This is the
  single largest risk of this decision and the reason for per-namespace isolation, strict
  resource limits, and NetworkPolicy.
- **The origin IP is public, and it is the only server answering.** *(revised 2026-08-12 —
  this previously read "origin bypass is mitigated, not eliminated".)* There is no bypass to
  mitigate, because there is no edge. Every request from every client reaches Traefik
  directly.
- **No WAF, no edge rate limiting, no bot management, no DDoS absorption.** These are not
  deferred or partially present — they are **absent from the request path**, and the
  neighbouring production namespaces share the host that absorbs whatever arrives.
- **TLS, redirect behaviour, availability, resource limits, and origin security are entirely
  the operator's responsibility.** No vendor supplies a fallback for any of them.
- **A certificate is consumed for `www.renvor.dev`**, a hostname that serves no content, and
  its renewal can fail like any other (D11).
- **Observability drops to origin-side only** (D10). An outage that prevents traffic reaching
  the origin generates no signal.
- **Single node, no high availability.** Node loss takes both sites down. Acceptable for a
  prerelease project's marketing and documentation sites; not acceptable later for anything
  transactional.
- **Component versions are one minor behind upstream** and must not be upgraded casually on
  a shared host.

**What this buys, since the costs above are substantial:** the entire request path — routing,
redirect, TLS issuance, caching policy, security headers, rate limiting — is described by
manifests in `renvor-infra` and reviewed like any other change. Under the previous decision
a material part of it lived in a vendor console that no repository described, that no review
covered, and that D6's own consequences already flagged as prone to drift.

**To reverse this:** both sites are stateless. Reversal is deleting two namespaces and the
DNS records; nothing neighbouring depends on them. **Re-enabling the proxy later is also
reversible**, but is a decision of its own — it would move TLS termination, caching, and
header policy back out of version control, and must be recorded rather than switched on.

> **The section that stood here has moved.** It was a second decision mislabelled `D6`
> (colliding with the delivery decision) recording **T105 — Cloudflare serves the `www`
> redirect**. It is superseded by **D11** above, which records the same redirect served by
> Traefik, preserves the original Cloudflare-versus-Traefik comparison, and states what the
> change costs. **T105 itself remains complete and is not rewritten** — it decided correctly
> under the architecture then in force.

## Unresolved questions

**Each unresolved question below carries an explicit owner and a blocking task, so none can
be forgotten.** Questions 1, 2 and 5 were resolved on 2026-08-12 — **T099**, **T105**, and
**T110** — question 4 was resolved on 2026-08-14 — **T101** — and **question 3 was resolved on
2026-08-15 — T106**, the maintainer ruling on the shared server's absent backups.
**All five are now closed, and this record is `accepted` as of 2026-08-15.**

| # | Unresolved question | Owner | Blocking task |
|---|---|---|---|
| ~~1~~ | ~~GitHub Container Registry versus the VPS GitLab registry, including the credential model~~ | Ahmed Anbar | **T099 — RESOLVED 2026-08-12: GHCR, `GITHUB_TOKEN` publishing, public image, no pull secret. See D7** |
| ~~2~~ | ~~Whether the `www.renvor.dev` redirect is served by Cloudflare or by Traefik~~ | Ahmed Anbar | **T105 — RESOLVED 2026-08-12 (Cloudflare). Superseded 2026-08-12 by T110 → Traefik, see D11.** T105 is not reopened |
| ~~3~~ | ~~Maintainer ruling on the shared server's absent backups~~ | Ahmed Anbar | **T106 — RESOLVED 2026-08-15.** The absence of shared-cluster backups does not block deployment of Renvor's **stateless** properties; it remediates nothing for the unrelated stateful namespaces; any **stateful** Renvor workload stays blocked; a deployment must be additive, isolated, resource-bounded, digest-addressed, and reversible. Resource-bounding and isolation must be **created, not inherited** — the cluster has zero `ResourceQuota`, `LimitRange`, and `NetworkPolicy` — and **NetworkPolicy enforcement must be verified on this CNI first**. The absence of backups is **total**, so the exemption ends the moment any Renvor workload holds state. Evidence §3ay |
| ~~4~~ | ~~CSP compatibility with the V7 landing implementation (GSAP, self-hosted variable fonts)~~ | Ahmed Anbar | **T101 — RESOLVED 2026-08-14: a 434-byte policy enforced against tree `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`; negative control 3/3, matrix 48/48, zero application violations. See D5 and evidence §3at.** A local harness served the enforcement header; **no production response header was configured or enabled, no Traefik middleware was written, configured, or enabled, and no live-server access or production-infrastructure action occurred** |
| ~~5~~ | ~~Whether the Cloudflare proxy is enabled and the origin authenticated to the edge~~ | Ahmed Anbar | **T110 — RESOLVED 2026-08-12: DNS-only, no proxy. See D3, D4, D5, D10, D11** |

1. ~~**Registry choice is not decided.**~~ **Resolved 2026-08-12 — GHCR (T099).** Publishing
   uses the workflow's short-lived `GITHUB_TOKEN` with `contents: read` and `packages: write`
   on the publishing job only; **this is not OIDC**, and an earlier draft of this record
   described it as such in error. The deployment image is **publicly pullable**, so the
   cluster needs no `imagePullSecret` and no registry credential is stored anywhere. See D7.
   **Registry configuration, image publication, deployment workflows, and production
   deployment all remain blocked** — by the gates below, not by this question.
2. ~~**Whether `www.renvor.dev` redirects at Cloudflare or at Traefik.**~~ **Resolved
   2026-08-12 — Cloudflare (T105). Superseded the same day by T110 — Traefik**, because a
   Cloudflare redirect rule requires a proxied record and D3 keeps every record DNS-only. See
   **D11**. Neither the rule nor the Traefik router has been created.
3. **Whether the neighbouring workloads' missing backups should block Renvor deployment.**
   This record says no — Renvor is stateless — but flags it for the maintainer's judgement.
4. ~~**CSP compatibility with the V7 landing page has not been tested** and may require
   explicit allowances for GSAP and self-hosted variable fonts.~~ **Resolved 2026-08-14 —
   tested, in Enforcement, against an immutable build (T101).** The verified state is the
   exact tree `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`, reached through site pull request
   #3 (merge `206cefdff74399d96f723a75d961fb8d700e0fd5`, base
   `fe0e468e8ed6b54d211423b056e0d44a0669b66c`, audited signed source
   `f8f1786a02c2d921859068fbd487b5d5e57a764c`); the merge added no content beyond that source
   tree. A 434-byte candidate policy was served as `Content-Security-Policy` — enforcing, not
   report-only — and the negative control passed **3/3** while the full matrix passed
   **48/48**, with zero application CSP events. **GSAP ran without `unsafe-inline` or
   `unsafe-eval`.** `Outfit Variable` and `Geist Mono Variable` were fetched from same-origin
   `/assets/fonts/...` resources under `font-src 'self'`. The candidate policy also allowed
   `data:` fonts, but r4 did not establish that allowance as necessary. The policy is not
   allowance-free — it carries one hashed style-attribute allowance using `unsafe-hashes`,
   plus `data:` for images and fonts. See D5 and evidence §3at. **Boundary: this was a local
   enforcement harness, not production and not Traefik. That harness did configure and serve
   the enforcement header; what did not happen is production. No production response header
   was configured or enabled, no Traefik middleware was written, configured, or enabled, and
   no live-server access or production-infrastructure action occurred — no deployment, DNS,
   server, or other infrastructure change was made.**

## Compliance

| Authority | How this record satisfies it |
|---|---|
| Constitution principle X | Every server fact is measured, not assumed; the premise "install Kubernetes" was corrected by observation |
| PLAN.md §16 security baseline | Non-root, dropped capabilities, seccomp, resource limits, NetworkPolicy, signed images, SBOM, provenance |
| PLAN.md §26.4, §26.5 | Digest-pinned images; rollback by previous digest |
| PLAN.md §26.9 | No plaintext secret in any repository; short-lived identity preferred |
| ADR-0005 | `renvor-infra` owns these manifests; the framework repository does not |

## Acceptance gate

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ **Met** — seven alternatives, with Cloudflare Tunnel analysed rather than assumed |
| 2 | Verification against `specs/001-governance-foundation/checklists/governance.md` | ✅ **Met 2026-08-12** — T086 complete; neither failure (CHK048, CHK050) falls inside this record's scope |
| 3 | All required CI and security checks passing | ✅ **Met 2026-08-11** — all four required checks passing on `renvor-rs/renvor` |
| 4 | A dated review record stored with the ADR | ✅ **Met** — this section, dated 2026-08-12 |

### Accepted 2026-08-15 — all four W-002 controls met and every unresolved question closed

*(This heading previously read "All four W-002 controls are met, and this record still remains
`proposed`". The reasoning it recorded is preserved below because it is why acceptance waited.)*

W-002 was never the only gate. **A record must not be accepted while it states that material
architecture choices inside its own scope are unresolved.** Accepting it then would have
published a decision record whose own text said some of its decisions had not been made — a
document asserting authority it did not have.

**All five questions are now closed** — **T099** (registry), **T101** (CSP compatibility,
2026-08-14), **T105** (`www` redirect location), **T110** (proxy versus DNS-only), and
**T106** (the backup ruling, **2026-08-15**, evidence §3ay). The condition for acceptance is
therefore met.

| Acceptance requirement | Status |
|---|---|
| W-002 control 1 — written alternatives-and-consequences review | ✅ seven alternatives for the record, plus **D13's own four-option review** with a "why A over B" and an explicit "what this does not solve" |
| W-002 control 2 — verification against the governance checklist | ✅ T086; **79/79**, with both former failures CHK048 and CHK050 resolved by T103 and T104 |
| W-002 control 3 — all required CI and security checks passing | ✅ `main` requires `verify (1.94.0)`, `verify (stable)`, `security`, `docs`, strict, `enforce_admins: true` |
| W-002 control 4 — a dated review record stored with the ADR | ✅ this section, dated **2026-08-15** |
| Every unresolved question inside the record's own scope closed | ✅ T099, T101, T105, T106, T110 |

**State `accepted`. Reviewer `Ahmed Anbar — self-review under W-002`. Review date 2026-08-15.**

> **This review is NOT independent and must never be described as independent** — not here,
> not in the evidence pack, not in `GOVERNANCE.md`, and not in any public document. It is a
> **structured self-review operating under a recorded exception**, exactly as the T006 ruling
> transcribed in `GOVERNANCE.md` requires. The project has one maintainer and no second person
> qualifies. **When a qualified independent reviewer becomes available, W-002 ends immediately
> and this record is re-reviewed.**

**What acceptance does not confer.** It does **not** authorise a deployment, and it changes no
gate: **T102, T108, and T111 remain non-completed and transferred**, T113 is complete but
proves only the landing repository's own CI and protection, and **no Renvor site is deployed**.
Acceptance makes this record's decisions citable as settled architecture; it does not make any
of them executed.

**D12 added a gate of its own, and D13 cancelled it. Neither moved this record closer to
acceptance.** *(This paragraph rewritten 2026-08-15; it previously recorded T114 as open.)*
The hybrid topology recorded on 2026-08-14 would have put infrastructure history on the same
VPS the infrastructure describes, so it carried **T114** — an encrypted off-VPS backup with a
proven exact-version restore, matching refs and hashes, recorded RPO and RTO, and a separate
human approval — before the GitLab project could be called canonical.

**That cutover was abandoned on 2026-08-15 and T114 is closed as cancelled / not applicable,
not as passed.** An encrypted backup was created; the exact-version restore proof was never
completed and no restore result was accepted; matching refs and hashes were never proven; no
RPO or RTO figure was measured; and the separate cutover approval was never granted, because
the cutover was cancelled. The maintainer then intentionally deleted the local Phase 3 and
Phase 4 backup and evidence directory. **D13 removes the gate by removing its subject**:
infrastructure source is on public GitHub, so no infrastructure history lives on the VPS and
no GitLab restore is required for Renvor recovery. **No GitLab recovery guarantee is claimed.**

**T113 remains open, T106 remains open, this record is not accepted, and Phase 001 is not
complete.**

**T106 cannot close on the current evidence.** A read-only reinspection of the server was
attempted on 2026-08-12 and **failed at authentication** — the SSH profile targets user
`deploy` while the host mapping uses a different user and identity. The 2026-08-11 audit is
retained as **historical evidence, not current proof**. Resolving the credential mismatch
touches a live shared production host and requires separate authorisation.

**T101 changed character under D3 and became more load-bearing, not less.** CSP was
previously a Cloudflare Transform Rule; it is now a Traefik middleware the project must write
and maintain itself, against a landing page using GSAP and self-hosted variable fonts. The
question is the same and the party who has to answer it has changed.

**T101 closed on 2026-08-14 — as a compatibility result, not as a shipped control.** The
landing page was proven compatible with a strict enforced policy: the exact tree
`e7fbc9d1438eaf58dee2c7d634dac4003b8664ec` was served under a 434-byte
`Content-Security-Policy` with a negative control passing 3/3 and the full matrix passing
48/48, producing zero application CSP violations. **GSAP ran without `unsafe-inline` or
`unsafe-eval`.** `Outfit Variable` and `Geist Mono Variable` were fetched from same-origin
`/assets/fonts/...` resources under `font-src 'self'`. The candidate policy also allowed
`data:` fonts, but r4 did not establish that allowance as necessary. **What closed is the
question "is the page compatible"; what did not close is "is the header shipped"** — a local
harness configured and served the enforcement header, but **no production response header was
configured or enabled, no Traefik middleware was written, configured, or enabled, and no
live-server access or production-infrastructure action occurred**. Writing that middleware
remains this project's own work under D5.

**The candidate policy's hashes are artifact-bound.** The candidate policy was validated
against the production build generated from tree
`e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`. Any new production build must be revalidated.
Recompute a particular hash only when the corresponding inline script or style-attribute
bytes change; unrelated output changes do not alter that digest. A stale hash fails closed:
the asset is blocked, not silently permitted.

### A second, independent gate specific to this record

The server facts this record rests on were observed on **2026-08-11** on a host shared with
five unrelated production namespaces. They **must be re-verified immediately before any
deployment** — tracked as **T102**, which remains deliberately open and must not be marked
complete in advance. Deploying against a stale audit is the failure mode that gate exists
to prevent.

On acceptance the reviewer field will read exactly
**`Ahmed Anbar — self-review under W-002`**, and the review must not be described as
independent.
