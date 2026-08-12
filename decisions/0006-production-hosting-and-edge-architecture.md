# ADR-0006: Host the public sites on the existing k3s cluster behind a proxied Cloudflare edge

| Field | Value |
|---|---|
| **ID** | 0006 |
| **State** | `proposed` |
| **Reviewer** | *(pending — see Acceptance gate)* |
| **Review date** | *(pending)* |
| **Superseded by** | — |
| **Owner** | Ahmed Anbar |

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
| Cloudflare | *(observed 2026-08-11)* `renvor.dev` and `ahmedanbar.dev` both on Cloudflare nameservers; `renvor.dev` had **no A record yet**; `ahmedanbar.dev` resolves **directly to the origin IP** (not proxied). **Superseded 2026-08-12**: the maintainer manually created three DNS-only A records for `renvor.dev`, `docs.renvor.dev`, and `www.renvor.dev` — see `governance/phase-001-evidence.md` §3af |

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

### D3 — Cloudflare proxied, Full (strict), with origin authentication

- `renvor.dev` apex and `docs.renvor.dev`: **proxied** (orange cloud).
- `www.renvor.dev`: proxied, with a permanent 301 redirect rule to `https://renvor.dev`.
- TLS mode: **Full (strict)**, which is honest only because cert-manager issues a real
  publicly-trusted origin certificate. Flexible and Full (non-strict) are rejected: the
  first sends plaintext to the origin, and the second accepts any certificate, making the
  padlock meaningless between edge and origin.
- **Authenticated Origin Pulls (mTLS)** enforced at Traefik for the two Renvor hostnames, so
  the origin serves those hostnames only to Cloudflare.

### D4 — Direct origin, not Cloudflare Tunnel — with the bypass closed at the origin

**This is the least obvious decision here, and Tunnel was not rejected lightly.**

| | Cloudflare Tunnel | Proxied direct origin (**chosen**) |
|---|---|---|
| Origin IP exposure | Hidden | **Already public** — the IP serves `ahmedanbar.dev` directly today |
| Inbound ports | None needed | 80/443 already bound and in use by existing sites |
| Origin bypass | Structurally impossible | **Possible unless mitigated** — closed by D3 mTLS |
| New moving parts | `cloudflared` daemon, a new failure domain | None |
| New long-lived credential | **Yes** — a tunnel token stored in the cluster | No |
| Failure mode | Tunnel down ⇒ site down, even though origin is healthy | Edge or origin fault, diagnosable independently |
| Operational ownership | New daemon to monitor, update, and rotate | Reuses what is already monitored |
| Blast radius on this server | New DaemonSet/Deployment alongside 28 production pods | Two Ingress objects |

Tunnel's headline benefit — hiding the origin IP — **buys nothing here**, because the origin
IP is already published in DNS for `ahmedanbar.dev` and is trivially discoverable. Adopting
Tunnel would add a daemon, a long-lived credential, and a new single point of failure to
obtain a property this server has already given up.

The genuine risk Tunnel would have solved is **origin bypass**: an attacker who knows the IP
can send `Host: renvor.dev` straight to Traefik and skip Cloudflare's WAF, rate limiting,
and caching. That is closed at the origin by D3's Authenticated Origin Pulls, which requires
no firewall change and therefore **cannot affect the neighbouring production services**.

**Rejected mitigation: allow-listing Cloudflare IP ranges in `ufw`.** `ufw` is currently
*inactive*; enabling it on a server running five unrelated production namespaces plus Docker
GitLab is a high-blast-radius change whose failure mode is "everything else goes down". mTLS
achieves the same goal at the application layer with no host-level firewall change.

**Recovery procedure if Cloudflare is unavailable:** set the two records to DNS-only
(grey cloud) and temporarily disable the origin-pull requirement in Traefik. This is a
two-step, documented, reversible action — deliberately manual, because automatic failover
that disables origin authentication would be a security regression triggered by an outage.

### D5 — Additional Cloudflare configuration

| Item | Decision |
|---|---|
| DNSSEC | Enable at the registrar/Cloudflare |
| CAA | Records permitting only the ACME CA that cert-manager uses; `iodef` to the security contact |
| HSTS | **Only after** both hostnames serve valid TLS and have been verified for at least one renewal cycle. Enabling it early makes a TLS mistake unrecoverable for the max-age duration. No preload until then. |
| Cache | Hashed static assets: long `max-age`, `immutable`. HTML: short TTL with revalidation, so a rollback is visible immediately rather than after a cache lifetime |
| Security headers | CSP, `Referrer-Policy`, `X-Content-Type-Options`, `frame-ancestors` — **CSP must be validated against the V7 landing implementation**, which uses GSAP and variable web fonts and may require explicit `script-src`/`font-src` entries |
| Rate limiting | On the origin-facing paths; both sites are static, so any sustained POST volume is abuse by definition |
| Origin-bypass assessment | Documented above; mitigated by D3, re-tested after deployment by attempting a direct `Host:`-header request to the origin IP and confirming it is refused |

### D6 — Delivery: narrowly scoped deployment workflow, not GitOps

A GitOps controller (Flux/Argo CD) is rejected **for now**: it is a continuously running
in-cluster component with broad permissions, added to a shared server, to manage two static
sites. The complexity is not yet earned.

Instead, each private repository has a deployment workflow that authenticates with a
narrowly scoped credential and updates a single Deployment's image **by digest**. This is
revisited if the number of deployed properties grows beyond about five, or when a second
maintainer joins.

### D7 — Images and supply chain

- Private registry; images referenced **by digest**, never by tag.
- Signed images with SBOM and provenance attestation; vulnerability scan before promotion.
- Minimal static-content base image; the sites are static HTML, CSS, JS.
- Image-pull authentication by a narrowly scoped, rotatable credential held as a cluster
  secret, never in Git.

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

### D10 — Monitoring, logs, updates

`metrics-server` is already present. Renvor adds no monitoring stack in the first
deployment; probes plus Cloudflare edge analytics are the initial signal, and a metrics
stack is only justified once there is something to observe. Log retention uses existing
node journald policy. Component updates follow upstream releases through the private
repositories' dependency policy, applied deliberately — never automatically on a shared
production host.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Install a fresh lightweight distribution (k0s, MicroK8s, RKE2, kubeadm) | Would contend for ports 80/443, CNI, and the container runtime with a live cluster serving five namespaces. All risk, no benefit. |
| Skip Kubernetes; serve static files from nginx or Caddy on the host | Genuinely simpler for two static sites, and a reasonable choice on an empty server. Rejected here because it would need ports 80/443 that Traefik already owns, so it would mean dismantling or fronting the existing ingress — more disruptive than adding two Ingress objects to it. |
| Cloudflare Pages / Workers, no VPS at all | Removes the origin entirely and would be a strong choice for two static sites. Rejected because the maintainer's stated requirement is Kubernetes on the owned VPS, and because the framework will later need a real origin for dynamic examples. Recorded as the most credible alternative if operational load becomes a problem. |
| Cloudflare Tunnel | See D4 — solves a problem this origin does not have, at the cost of a daemon, a long-lived credential, and a new failure domain. |
| `ufw` allow-list of Cloudflare ranges | High blast radius on a shared server whose firewall is currently inactive; mTLS achieves the same at the application layer. |
| GitOps controller (Flux, Argo CD) | Continuously running broad-permission component for two static sites. Not yet earned; revisit at scale. |
| Flexible or Full (non-strict) Cloudflare TLS | Flexible sends plaintext to the origin. Full (non-strict) accepts any certificate, making origin TLS decorative. cert-manager already provides a real certificate, so strict costs nothing. |

## Consequences

**Accepted costs:**

- **Renvor becomes a tenant on a server it does not exclusively own.** A Renvor
  misconfiguration can affect `ahmedanbar.dev`, `codexhub`, `attaa`, and GitLab. This is the
  single largest risk of this decision and the reason for per-namespace isolation, strict
  resource limits, and NetworkPolicy.
- **Origin bypass is mitigated, not eliminated.** mTLS at Traefik is an application-layer
  control. A misconfiguration there re-opens direct origin access silently, so the
  post-deployment bypass test is mandatory, not optional.
- **Single node, no high availability.** Node loss takes both sites down. Acceptable for a
  prerelease project's marketing and documentation sites; not acceptable later for anything
  transactional.
- **Manual Cloudflare configuration** is not in version control initially, so it can drift
  from what `renvor-infra` documents.
- **Component versions are one minor behind upstream** and must not be upgraded casually on
  a shared host.

**To reverse this:** both sites are stateless. Reversal is deleting two namespaces and two
DNS records; nothing neighbouring depends on them.

### D6 — Cloudflare serves the permanent `www` redirect (T105, decided 2026-08-12)

**`www.renvor.dev` → `https://renvor.dev`, HTTP 301, preserving path and query string,
implemented as a Cloudflare rule at the edge.**

The redirect must answer visitors who never reach the origin, and it should not consume an
origin certificate, an Ingress, or a Traefik router to serve a response whose only purpose is
to redirect. Cloudflare answers it before the origin is involved at all.

**Rejected: Traefik.** Keeping the rule in version control is a genuine advantage, and it is
the reason this was a real choice rather than a formality. It loses on two counts: it
requires issuing and renewing a certificate for a hostname that serves no content, and it
cannot answer while the origin is down — which is exactly when a redirect being cheap
matters.

Consequences, recorded so none is discovered later:

| Consequence | Detail |
|---|---|
| Requires proxying | The rule applies only to a **proxied** record. `www.renvor.dev` is DNS-only today, so **the redirect cannot function yet** |
| Current visitor experience | `www.renvor.dev` resolves to the origin and receives the Traefik default certificate, so a browser shows a certificate warning. Expected in the temporary state |
| Traefik | Needs **no** `www` router, and no origin certificate is issued for `www` |
| Failure mode | If Cloudflare proxying is ever disabled, **the redirect stops working**. A Traefik fallback would then be needed — a deliberate trade, recorded here rather than discovered during an outage |
| Status | **The rule has not been created.** Creating it is a separate authorised action |

## Unresolved questions

**Each unresolved question below carries an explicit owner and a blocking task, so none can
be forgotten.** Question 2 was resolved on 2026-08-12. **The remaining three — T099, T101,
and T106 — still block acceptance of this record, which therefore stays `proposed`.**

| # | Unresolved question | Owner | Blocking task |
|---|---|---|---|
| 1 | GitHub Container Registry versus the VPS GitLab registry, including the credential model | Ahmed Anbar | **T099** |
| ~~2~~ | ~~Whether the `www.renvor.dev` redirect is served by Cloudflare or by Traefik~~ | Ahmed Anbar | **T105 — RESOLVED 2026-08-12, see D6** |
| 3 | Maintainer ruling on the shared server's absent backups | Ahmed Anbar | **T106** |
| 4 | CSP compatibility with the V7 landing implementation (GSAP, self-hosted variable fonts) | Ahmed Anbar | **T101** |

1. **Registry choice is not decided.** GitHub Container Registry versus the GitLab registry
   already running on this host. GHCR pairs with the GitHub-based workflow and OIDC; the
   local GitLab registry avoids an external dependency but couples Renvor to another
   project's service. **The private repositories already exist and are empty**, so this no longer blocks their creation; it blocks registry configuration, image publication, deployment workflows, and production deployment.
2. ~~**Whether `www.renvor.dev` redirects at Cloudflare or at Traefik.**~~ **Resolved
   2026-08-12 — Cloudflare.** See D6 below. The rule itself has not been created.
3. **Whether the neighbouring workloads' missing backups should block Renvor deployment.**
   This record says no — Renvor is stateless — but flags it for the maintainer's judgement.
4. **CSP compatibility with the V7 landing page has not been tested** and may require
   explicit allowances for GSAP and self-hosted variable fonts.

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
decision record whose own text says four of its decisions have not been made — the document
would assert authority it does not have.

Four questions remain open, each now carrying an owner and a blocking task: **T099**
(registry), **T105** (`www` redirect location), **T106** (backup ruling), **T101** (CSP
compatibility). Acceptance requires all four resolved, either in this record or split into
scoped follow-up records.

### A second, independent gate specific to this record

The server facts this record rests on were observed on **2026-08-11** on a host shared with
five unrelated production namespaces. They **must be re-verified immediately before any
deployment** — tracked as **T102**, which remains deliberately open and must not be marked
complete in advance. Deploying against a stale audit is the failure mode that gate exists
to prevent.

On acceptance the reviewer field will read exactly
**`Ahmed Anbar — self-review under W-002`**, and the review must not be described as
independent.
