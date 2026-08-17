# Deployment — Current-State Evidence

**Subject**: the first production deployment of a Renvor property.
**Captured**: 2026-08-17 (UTC throughout; the maintainer's local zone is UTC+03:00, so some
timestamps fall on 2026-08-18 locally).
**Source of truth**: the live internet, the live cluster, and the GitHub API — read directly, not
inferred from a plan.

## Why this file exists rather than an edit to the older ledgers

`governance/phase-001-evidence.md` and `governance/phase-002-evidence.md` are **dated records of
what was observed on the day they were written**, and several of their statements were true then
and are false now. Rewriting them would destroy the only account of what was actually known at the
time, which is the point of an evidence ledger.

So the older statements stay, each gaining a dated superseding note that names this file. **This
file is authoritative for present-tense deployment facts. The older ledgers remain authoritative
for what was true on their own dates.**

## 1 — What is deployed

| | |
|---|---|
| Property | `https://renvor.dev` — the landing site |
| Source | [`renvor-rs/renvor-site`](https://github.com/renvor-rs/renvor-site) at `b2051d871d53fb8ce9a745cbd18d127bdbdba795` |
| Deployment configuration | [`renvor-rs/renvor-infra`](https://github.com/renvor-rs/renvor-infra) at `07bda7ad59c0e82bc441e4cb400d290cd60a882d` |
| Image | `ghcr.io/renvor-rs/renvor-site@sha256:56446da7c16e155396114e185206837710eee1587d3b58ef8e5ecca96ddb84af` |
| Reconciled by | Flux v2.9.4 — only `kustomize-controller` and `source-controller`, both image `v1.9.4` |
| Reconciler identity | `flux-system/renvor-reconciler` — **not** cluster-admin |
| Namespaces | `renvor-site` (2 pods) and `renvor-site-staging` (1 pod) — counted live |
| Registry credential in the cluster | **none.** Enumerated: all 4 ServiceAccounts and all 3 Pods across both namespaces have `imagePullSecrets` **absent** — not empty, absent |

The **same digest** runs in both namespaces, and it is the digest whose provenance, SBOMs, and
attestations were verified before promotion. All three pods report `ready=true` with **0 restarts**.

Three Secrets exist, all created and owned by cert-manager, none carrying application data:
`letsencrypt-prod-account-key` (Opaque), `letsencrypt-staging-account-key` (Opaque), and
`renvor-dev-tls` (`kubernetes.io/tls`) — all in `renvor-site`, none in staging. **Only names and
types were read; no Secret value was read, printed, logged, or committed at any point.** The
staging account key is a residue of issuing against Let's Encrypt's staging directory first, which
is where the two self-inflicted issuance blockers were found without consuming a production rate
limit.

## 2 — What the internet actually returns

Captured **2026-08-17T21:01:50Z**.

```
GET https://renvor.dev/     status=200  time=0.789s  ip=153.92.208.119  http/2
TLS                          TLSv1.3, TLS_AES_128_GCM_SHA256, Verify return code: 0 (ok)
issuer                       C=US, O=Let's Encrypt, CN=YE1
subject                      CN=renvor.dev
serial                       058D7E8F39697BB7B71E2AC333E7FBD58291
notBefore                    Aug 17 18:12:33 2026 GMT
notAfter                     Nov 15 18:12:32 2026 GMT
subjectAltName               DNS:renvor.dev, DNS:www.renvor.dev
```

### 2.1 Redirects are method-specific, and that is recorded rather than averaged

An earlier summary of this deployment said "301, preserving path and query". **That is true of
`GET` and false of `HEAD`**, and the difference is stated here rather than generalised away:

| Method | Request | Response | `Location` |
|---|---|---|---|
| `GET` | `http://renvor.dev/plan?x=1` | **301** Moved Permanently | `https://renvor.dev/plan?x=1` |
| `HEAD` | `http://renvor.dev/plan?x=1` | **308** Permanent Redirect | `https://renvor.dev/plan?x=1` |
| `GET` | `https://www.renvor.dev/plan?x=1` | **301** | `https://renvor.dev/plan?x=1` |
| `HEAD` | `https://www.renvor.dev/plan?x=1` | **308** | `https://renvor.dev/plan?x=1` |
| `GET` | `http://www.renvor.dev/plan?x=1` | **301** | `https://www.renvor.dev/plan?x=1` |
| `HEAD` | `http://www.renvor.dev/plan?x=1` | **308** | `https://www.renvor.dev/plan?x=1` |

Both codes are permanent and both preserve path and query, so the redirect **contract** holds under
either method. What differs is the status code, and a record that said only "301" would be wrong
half the time.

`http://www.renvor.dev` reaches the apex in **two** hops — to `https://www.` first, then to
`https://renvor.dev` — because scheme upgrade and host canonicalisation are separate middlewares.
The final effective URL preserves both path and query.

### 2.2 `docs.renvor.dev` is deliberately not deployed

| Scheme | Result |
|---|---|
| `http://docs.renvor.dev/` | **404** |
| `https://docs.renvor.dev/` | **404** only with certificate validation bypassed; against a public trust store the handshake **fails**, because the origin presents `CN=TRAEFIK DEFAULT CERT` |

A previous summary of this deployment recorded "404 on both schemes". That is imprecise: over HTTPS
there is no valid certificate for this hostname at all, so an ordinary client never reaches a status
code. Corrected here.

`renvor-rs/renvor-docs` remains **commit-empty** — 0 commits, 0 branches — and no route, Certificate,
or IngressRoute names this hostname.

## 3 — The reconciliation boundary, stated precisely

Both Flux Kustomizations report `Ready=True` at `main@sha1:07bda7ad59c0e82bc441e4cb400d290cd60a882d`
with `serviceAccountName: renvor-reconciler`.

**This is soft multi-tenancy, and the limit is recorded rather than glossed.** Upstream Flux binds
`kustomize-controller` to `cluster-admin`, and that binding is retained, because Kubernetes
impersonation requires the impersonator to already hold the rights it delegates. What is constrained
is **what the public repository can cause the controller to do**: repository-driven applies run as
`renvor-reconciler`, which may **write 10** resource types in two namespaces — `certificates`,
`deployments`, `ingressroutes`, `issuers`, `limitranges`, `middlewares`, `networkpolicies`,
`resourcequotas`, `serviceaccounts`, `services` — may additionally **read** exactly two more,
`pods` and `replicasets`, which `wait: true` health checks require, may read no Secret in any
namespace, and is denied every cluster-scoped resource. Counted from the live Role, not from the
manifest that produced it.

A malicious or mistaken commit to the public repository is contained by that boundary. **A
compromise of the `kustomize-controller` process itself is not.** Hard multi-tenancy would require a
separate cluster, and this project does not have one.

## 4 — Co-tenants

Renvor is a guest on a shared single-node k3s cluster. Pod counts, before and after the deployment:

| Namespace | 2026-08-17T08:38:49Z (before) | 2026-08-17T21:0xZ (after) |
|---|---|---|
| `codexhub` | ≥755 | 768 |
| `attaa` | ≥161 | 168 |
| `cert-manager` | ≥96 | 99 |
| `portfolio` | ≥93 | 95 |
| `kube-system` | not separately captured | 5 |
| `gitlab` | not separately captured | 0 |

The cluster's single node reports `MemoryPressure=False DiskPressure=False PIDPressure=False
Ready=True`. *(The node's hostname is deliberately omitted. Phase 001 limitation **R-17** records
that this repository already publishes more operational detail about this shared third-party host
than its own minimisation standard allows; a first draft of this file named the node, and that name
appears nowhere else on `main`. Adding it would have widened an exposure the project has already
flagged as needing narrowing, to buy nothing — the condition flags are the fact worth recording.)*

**These counts went up, and that is stated rather than smoothed.** The "before" figures are lower
bounds: the capture was truncated at thirty rows and grouped by phase, so `Running` pods in the
smaller namespaces are not included. The observable increase is in `Completed`, `Evicted`, and
`ContainerStatusUnknown` pods — these namespaces accumulate terminated pods continuously, and 524
evictions were already recorded on this node before any Renvor object existed.

**So "co-tenants unchanged" is not claimed from these numbers, because these numbers do not
establish it.** What is established, and by what:

| Claim | Evidence |
|---|---|
| Repository-driven reconciliation cannot write in any co-tenant namespace | `SubjectAccessReview` against `system:serviceaccount:flux-system:renvor-reconciler` — **re-run live on 2026-08-17 while writing this record**, see §4.1 |
| Renvor created objects only in `renvor-site`, `renvor-site-staging`, and the hand-applied bootstrap in `flux-system` | the applied manifests, and `targetNamespace` on both Kustomizations |
| Renvor installs no distribution, adds no second ingress controller, and upgrades nothing | it uses the existing Traefik 3.6.13 and cert-manager v1.20.2 through their public APIs, and creates only namespaced objects |
| TLS is issued by a namespace-scoped `Issuer`, not a `ClusterIssuer` | `apps/renvor-site/overlays/production/issuer.yaml` — editing a cluster-wide issuer would put every other certificate on this shared host at risk of a Renvor mistake |

### 4.1 The boundary, re-proven rather than cited

`kubectl auth can-i` is not used here: it gives a **false positive** on subresources — `create
serviceaccounts/token` answers *yes* against a plain `serviceaccounts` rule while the authoritative
`SubjectAccessReview` answers `allowed=false`. Every result below is a `SubjectAccessReview`.

| Probe | Result |
|---|---|
| `secrets` × 8 verbs (`get list watch create update patch delete` and each namespace) across `renvor-site`, `renvor-site-staging`, `flux-system`, `kube-system`, `cert-manager`, `gitlab`, `codexhub`, `attaa`, `portfolio`, `default` | **80 / 80 denied** |
| `create` on `namespaces`, `nodes`, `customresourcedefinitions`, `clusterroles`, `clusterrolebindings` | **5 / 5 denied** |
| **Positive control** — `create deployments` in `renvor-site` | **allowed** |
| **Negative control** — `create deployments` in `gitlab` | **denied** |

The two controls matter: without the first, a boundary that denied *everything* — including the
work it is supposed to do — would satisfy every denial above and be indistinguishable from a
correct one.

An earlier draft of this section asserted that "no unrelated workload was created, modified,
restarted, or deleted". **That was an overclaim** — it is a statement about everything that happened
on a busy shared host over thirteen hours, and pod counts cannot support it. The narrower claims
above are the ones the evidence actually carries.

## 5 — Required checks on `renvor-infra`: a missed deadline, corrected late

**This section exists because the obligation was missed. It is not a record of a gate that ran.**

`governance/phase-001-evidence.md` limitation **R-9** and its recurring-obligation row required that
`renvor-rs/renvor-infra` acquire CI **and that its checks be required**, with the deadline stated as
*"Before the first manifest is merged"*.

What actually happened, from the Git and GitHub API record:

| Event | Time (UTC) |
|---|---|
| First manifest merged (PR #1, `d87c6bd`) — **and `infra-ci.yml` added in the same commit** | 2026-08-17T16:31:44Z |
| PRs #2–#7 merged | 2026-08-17T17:59:38Z … 19:24:37Z |
| `required_status_checks` added to ruleset `20889836` | **2026-08-17T20:42:25Z** |

So the obligation was **half met on time and half missed**:

- **CI existed at the first manifest** — the workflow and the manifests arrived in one commit, so
  there was never a manifest in this repository that no workflow examined.
- **The checks were not *required*.** Ruleset `20889836` carried no `required_status_checks` rule at
  all until 2026-08-17T20:42:25Z. **All seven pull requests — #1 through #7 — merged with `validate`
  advisory**, and because the ruleset also sets `required_approving_review_count: 0`, any of them
  could have merged with CI red.

The correction was made on **2026-08-17T20:42:25Z** under explicit maintainer authorisation, adding
exactly one rule and changing nothing else. The complete rule set now in force:

| Rule | Parameters |
|---|---|
| `required_status_checks` | `validate` (GitHub Actions, app id 15368), `strict_required_status_checks_policy: true`, `do_not_enforce_on_create: false` |
| `pull_request` | `required_approving_review_count: 0`, `required_review_thread_resolution: true`, merge methods `squash`, `rebase` |
| `required_signatures`, `required_linear_history`, `non_fast_forward`, `deletion` | — |

`enforcement: active`, `bypass_actors: []`, `current_user_can_bypass: never`, target
`~DEFAULT_BRANCH` — all four verified unchanged by read-back after the change.

**`required_approving_review_count` remains 0**, deliberately: this is a single-maintainer
repository, and raising it to 1 would block all work rather than obtain a reviewer. That gap is the
same one waivers W-001 through W-006 already record; **it is not closed here and must not be
described as closed.**

## 6 — Transferred Phase 001 gates, reconciled

The four gates transferred out of Phase 001 are `001-T102`, `001-T108`, `001-T109`, and `001-T111`.
Their status after this deployment:

### 001-T102 — re-verify the server audit immediately before deployment

**Substance met contemporaneously; the gate itself was never run as a gate. Recorded as
resolved-late, NOT as a gate that ran.**

A full read-only audit of the shared host was performed on **2026-08-17, beginning 08:37:14Z** —
before any Renvor object existed on the cluster. The **first** Renvor object was created at
**18:02:14Z**, which is the cluster's own `creationTimestamp` on the `renvor-site`,
`renvor-site-staging`, and `flux-system` namespaces, not a reading of a command log. The gap is
therefore **9 h 25 min**. It covered the subject matter T102 names: k3s and kubelet versions, node capacity, allocatable and
conditions; Traefik and cert-manager versions and endpoints; cluster-wide pod phase counts and
per-namespace distribution; eviction reasons; `kubectl top nodes`; host memory, disk and inode
usage; the CNI and a direct probe of NetworkPolicy *enforcement* rather than merely its API
presence; existing GitOps controllers; warning events; systemd timers; and a search for any running
backup process.

**What was not done**: it was not executed *as* T102, no contemporaneous T102 record was written,
and **9 h 25 min is not "immediately before"** by any reading. T102's own text warns that this host
is shared with workloads "whose facts change without notice", which is exactly why the gate says
*immediately*.

*(An earlier draft of this section put the gap at "roughly four and a half hours", derived from the
first mention of `kubectl apply` in the working log. **That was wrong** — the match was prose inside
a commit message, not a command. The figure above is taken from the cluster's own object
timestamps, which cannot be misread that way.)*

**Therefore**: the audit's substance is on the record and is cited above; the **process gate was
missed** and is recorded as such. No claim is made that T102 ran.

### 001-T108 — documentation deployment gate, `image-size`

**Still open.** The two `image-size` advisories were removed from the documentation site's resolved
dependency graph by Phase 002's T151 under ADR-0009, and the fail-closed image-input guard at
`docs/scripts/check-image-inputs.mjs` remains in force. But T108's two unverifiable compensating
controls — absence from the production runtime container and absence from the runtime SBOM —
concern the **documentation** image, and **no documentation image exists**, because
`docs.renvor.dev` is not deployed. Nothing in this deployment discharges them.

### 001-T109 — the `uuid` advisory `GHSA-w5hq-g745-h8pq`

**Unaffected by this deployment, and the row is left alone.** Phase 002's T151 removed this advisory
from the **documentation** site's resolved dependency graph along with the two `image-size` ones.
What T109 itself still requires is a **reassessment dated 2026-09-11**, and that has **not**
occurred. The landing deployment touches a different repository, a different toolchain, and a
different dependency graph, so nothing here brings that date forward or discharges it.

### 001-T111 — CAA records for `renvor.dev`

**Both blocking preconditions are now satisfied, and the records still do not exist.**

T111 required two things before the DNS change: cert-manager HTTP-01 issuance proven for the
affected hostnames, and separate maintainer authorisation of the exact records.

| Precondition | Status |
|---|---|
| HTTP-01 issuance proven | **Met.** A Let's Encrypt certificate for `renvor.dev` and `www.renvor.dev` was issued via HTTP-01 on 2026-08-17 (`notBefore` 18:12:33Z) and is being served with `Verify return code: 0 (ok)` |
| Maintainer authorisation of the exact records | **Met.** Granted 2026-08-17 for exactly three apex records |

The authorised records:

```
renvor.dev. CAA 0 issue     "letsencrypt.org"
renvor.dev. CAA 0 issuewild ";"
renvor.dev. CAA 0 iodef     "mailto:admin@ahmedanbar.dev"
```

**They were not created.** Verified 2026-08-17T21:01:50Z against **both** authoritative nameservers
and a recursive resolver:

| Resolver | `renvor.dev` CAA |
|---|---|
| `coco.ns.cloudflare.com` (authoritative) | **no records** |
| `earl.ns.cloudflare.com` (authoritative) | **no records** |
| `1.1.1.1` (recursive) | **no records** |

**The blocker is a missing capability, not a missing decision.** `renvor.dev` is delegated to
Cloudflare, and this environment has no authenticated Cloudflare access: no
`CLOUDFLARE_API_TOKEN`, `CF_API_TOKEN`, `CLOUDFLARE_API_KEY`, `CF_API_KEY`, `CLOUDFLARE_EMAIL`, or
`CLOUDFLARE_ACCOUNT_ID` in the environment; no `~/.cloudflared`, `~/.cloudflare`, or `~/.wrangler`
configuration; no `flarectl`, `cloudflared`, or `wrangler` on `PATH`; and no Cloudflare MCP server
configured. The Hostinger DNS API cannot reach the zone either — `renvor.dev` is **not registered at
Hostinger** (API: *"Domain is not registered at Hostinger"*), and its nameservers are
`coco.ns.cloudflare.com` and `earl.ns.cloudflare.com`.

No credential was requested, printed, or created.

**Publishing the intended records here discloses nothing.** Checked rather than assumed: the CA is
already disclosed by every TLS handshake (`issuer=C=US, O=Let's Encrypt, CN=YE1`); the `iodef`
address already appears in six files on `main`, including `SECURITY.md` and `README.md`; and the
**absence** of CAA is queryable by anyone — a public resolver returns zero records today. An
attacker learns nothing from this section they could not obtain with `dig` and a browser.

**T111 therefore remains open.** The ordering it protects has, however, been established and is
worth recording plainly: **issuance happened first.** Adding these records now cannot lock out the
issuer the deployment depends on, because that issuer has already succeeded and its certificate is
live until 2026-11-15.

## 7 — What this deployment does not establish

Recorded so that no later reader infers more than was measured:

- **No crate is published.** `crates.io` holds no `renvor`, `renvor-core`, `renvor-config`, or
  `renvor-cli`. Nothing here changes that.
- **No tag and no GitHub release exists** in any of the four repositories.
- **`docs.renvor.dev` is not deployed**, and T108 is not discharged.
- **HSTS is not enabled.** It is close to irreversible once a browser caches it, so it waits until a
  certificate renewal has been observed to succeed.
- **The framework repository's `platform` checks are not required.** `renvor-rs/renvor` requires
  `verify (1.94.0)`, `verify (stable)`, `security`, and `docs`; the four `platform` matrix jobs run
  on every pull request but are **advisory**. Changing that is a repository-settings change and was
  not authorised.
- **`renvor-rs/renvor` does not require signed commits** (`required_signatures: false`), even though
  every commit is signed by convention. Only `renvor-infra` enforces it, by ruleset. This restates
  Phase 001 limitation R-16 rather than closing it.
- **The independent-review gap is untouched.** Every review supporting this deployment was an
  advisory agent review, explicitly **NON-INDEPENDENT**. W-001 through W-006 remain active.
