# ADR-0006: Host the public sites on the existing k3s cluster, served directly from the origin with Cloudflare as DNS only

| Field | Value |
|---|---|
| **ID** | 0006 |
| **State** | `proposed` |
| **Reviewer** | *(pending — see Acceptance gate)* |
| **Review date** | *(pending)* |
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

> **These are Kubernetes namespace names, not repository names.** The private repositories
> are `renvor-rs/renvor-site`, `renvor-rs/renvor-docs`, and `renvor-rs/renvor-infra`. The
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

Instead, each private repository has a deployment workflow that authenticates with a
narrowly scoped credential and updates a single Deployment's image **by digest**. This is
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
  independent of repository visibility, so the source repositories stay private while the
  built artefact is public.
- Consequently **the k3s host needs no `imagePullSecret`**, and no registry credential is
  stored in the cluster at all.
- The image contains only the built static site — HTML, CSS, JS already served publicly at
  `renvor.dev`. **Making it public discloses nothing that visiting the site would not.**
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
through the private repositories' dependency policy, applied deliberately — never
automatically on a shared production host.

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
**T110** — and question 4 was resolved on 2026-08-14 — **T101**. **One remains — T106 — and
it still blocks acceptance of this record, which therefore stays `proposed`.**

| # | Unresolved question | Owner | Blocking task |
|---|---|---|---|
| ~~1~~ | ~~GitHub Container Registry versus the VPS GitLab registry, including the credential model~~ | Ahmed Anbar | **T099 — RESOLVED 2026-08-12: GHCR, `GITHUB_TOKEN` publishing, public image, no pull secret. See D7** |
| ~~2~~ | ~~Whether the `www.renvor.dev` redirect is served by Cloudflare or by Traefik~~ | Ahmed Anbar | **T105 — RESOLVED 2026-08-12 (Cloudflare). Superseded 2026-08-12 by T110 → Traefik, see D11.** T105 is not reopened |
| 3 | Maintainer ruling on the shared server's absent backups | Ahmed Anbar | **T106** |
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

### All four W-002 controls are met, and this record still remains `proposed`

W-002 is not the only gate. **A record must not be accepted while it states that material
architecture choices inside its own scope are unresolved.** Accepting it would publish a
decision record whose own text says some of its decisions have not been made — the document
would assert authority it does not have.

**One question remains open**, carrying an owner and a blocking task: **T106** (the backup
ruling). Four are now closed — **T099** (registry), **T101** (CSP compatibility, closed
2026-08-14), **T105** (`www` redirect location), and **T110** (proxy versus DNS-only) — and
their closure does not accelerate the rest.

Acceptance requires the remaining question to be resolved, either in this record or split
into a scoped follow-up record. **This record therefore stays `proposed`.**

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
