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

## 0 — Corrections to the first version of this file

This file was merged on 2026-08-17 in `5acd051`. An independent-in-process but **NON-INDEPENDENT**
advisory review of that change reported four defects in it *after* it had merged. All four are
corrected here, in place, and logged below — because leaving a false statement standing and adding a
note beside it would mean the false statement is still what a reader finds first.

| # | What the first version said | Why it was wrong | Where it is now |
|---|---|---|---|
| C-1 | the reconciler "may **write 10** resource types in two namespaces" | that is the **union** of two different Roles; `renvor-site` grants 10 and `renvor-site-staging` grants 6. A boundary described by its union is described as wider than it is, which is the wrong direction for a safety claim | §3.1 |
| C-2 | Renvor "creates only namespaced objects" | true of repository-driven reconciliation, **false of the hand-applied bootstrap**, which creates 15 cluster-scoped objects including 7 CRDs and a `cluster-admin` binding. The sentence did not say which path it meant | §4, §4.2 |
| C-3 | the `cluster-admin` binding "is retained" | accurate about the action, misleading about the provenance: it reads as pre-existing, and Flux was installed **by** this bootstrap | §3, §4.2 |
| C-4 | the digest's "provenance, **SBOMs**, and attestations were verified before promotion" | two SBOMs are generated and **one** is attested; and both attestations are produced *after* the registry push, not before it | §1 |

C-2 is the one that could actually mislead a third party: it is a statement about the blast radius
of this project on a **shared host that other people's workloads run on**, and it understated that
footprint. It is corrected in the direction of a wider disclosure, not a narrower one.

**This section is not a superseding note on a dated ledger.** §§1–7 are a present-tense record of the
current deployment, so a factual error in them is a defect to fix rather than history to preserve.
The four sentences above are quoted so the fix is auditable against `5acd051` without reading the
diff.

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

The **same digest** runs in both namespaces. All three pods report `ready=true` with **0
restarts**.

**What is attested, stated exactly.** `publish-image.yml` generates **two** SBOMs and attests
**one**. The runtime SBOM (`sbom/renvor-site.spdx.json`, scanned from the image) is signed by
`actions/attest-sbom` with the digest as subject; the dependency SBOM
(`sbom/renvor-site-dependencies.spdx.json`, scanned from the source tree) is hashed into the job
summary and uploaded as a workflow artifact, and is **never attested**. Build provenance is attested
separately. So the digest carries **one provenance attestation and one SBOM attestation**, not
"SBOMs" plural.

**And the ordering, which "before promotion" left ambiguous.** Both attestations are created
*after* the push to GHCR — `attest-build-provenance` and `attest-sbom` take
`steps.push.outputs.digest` as their subject, so the digest must already exist in the registry
before either can run. `gh attestation verify` then runs against the registry, after both. What is
true is that all of this completed before the cluster was pointed at the digest; what is not true is
that anything was verified before the image was published.

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
`kustomize-controller` to `cluster-admin`. **That binding exists on this cluster because the Renvor
bootstrap created it** — `cluster-reconciler-flux-system` is part of the upstream Flux v2.9.4
manifest this project applied, and it binds **both** `kustomize-controller` **and** `helm-controller`
to `cluster-admin`. It is retained rather than removed because Kubernetes impersonation requires the
impersonator to already hold the rights it delegates. It is not something Renvor inherited from a
pre-existing installation; see §4.2.

What is constrained is **what the public repository can cause the controller to do**:
repository-driven applies run as `renvor-reconciler`.

### 3.1 The two Roles are not the same Role

There are **two** Roles named `renvor-reconciler`, one per namespace, and they differ. An earlier
version of this section said the identity "may write 10 resource types in two namespaces". **That
figure is the union of the two Roles, and it describes staging as four resource types wider than it
is.** Per namespace:

| Namespace | Writable types (`create` `update` `patch` `delete`) | Count | Read-only (`get` `list` `watch`) |
|---|---|---|---|
| `renvor-site` | `services`, `serviceaccounts`, `limitranges`, `resourcequotas`, `deployments`, `networkpolicies`, `certificates`, `issuers`, `ingressroutes`, `middlewares` | **10** | `pods`, `replicasets` |
| `renvor-site-staging` | `services`, `serviceaccounts`, `limitranges`, `resourcequotas`, `deployments`, `networkpolicies` | **6** | `pods`, `replicasets` |

Staging holds **no** `cert-manager.io` and **no** `traefik.io` grant, deliberately: the staging
overlay renders neither, and Traefik routes cluster-globally, so a staging `IngressRoute` claiming
`Host(renvor.dev)` would contend with production for real traffic. The per-environment hostname
allow-list in CI would not catch that — `renvor.dev` is on the allow-list, just not for that
environment.

Neither Role grants any verb on `secrets` in any namespace, and neither grants any cluster-scoped
resource.

**Provenance of these counts.** They are read from
`clusters/hostinger/flux-system/renvor-tenancy.yaml` at
`07bda7ad59c0e82bc441e4cb400d290cd60a882d` — the manifest that is applied — and from the rendered
output in `rendered/clusters-hostinger-flux-system.yaml`. The first version of this section said
"counted from the live Role, not from the manifest that produced it"; that sentence is withdrawn,
because the count it introduced was wrong in the direction that overstates the boundary, and because
the correction was made from the manifests. The two agree on the rules; the live re-read is recorded
in §4.1 for the authorisation results, not for these counts.

*(The manifest's own header comment says "ten resource types, in two namespaces", and its production
Role carries the comment "Identical rules to staging" while a comment forty lines above correctly
states the opposite. Those two comments in `renvor-infra` are wrong in the same way this section was.
They are reported here and not corrected, because infrastructure changes are outside this change's
authorised scope.)*

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
smaller namespaces are not included. The increase is in **terminated** pods — `Completed` and
`ContainerStatusUnknown` — which these namespaces accumulate continuously.

**`Evicted` is not claimed as part of that increase.** An earlier version of this paragraph listed
it alongside the other two while also citing 524 as the number of evictions already recorded on the
node before any Renvor object existed. Those two statements cannot both be supported by one figure:
524 is a single capture, and nothing here establishes it as either the before value or the after
value of a comparison. What is supported is that **524 evicted pods were present on this node**, and
that this is a pre-existing property of a shared host running four unrelated workloads. No delta in
evictions is claimed in either direction.

**So "co-tenants unchanged" is not claimed from these numbers, because these numbers do not
establish it.** What is established, and by what:

| Claim | Evidence |
|---|---|
| Repository-driven reconciliation cannot write in any co-tenant namespace | `SubjectAccessReview` against `system:serviceaccount:flux-system:renvor-reconciler` — **re-run live on 2026-08-17 while writing this record**, see §4.1 |
| Renvor created objects only in `renvor-site`, `renvor-site-staging`, and the hand-applied bootstrap in `flux-system` | the applied manifests, and `targetNamespace` on both Kustomizations |
| Renvor installs no *ingress* distribution, adds no second ingress controller, and upgrades nothing | it uses the existing Traefik 3.6.13 and cert-manager v1.20.2 through their public APIs, and adds no CRD, admission webhook, or controller to either |
| **Repository-driven** reconciliation creates only namespaced objects | the two Roles in §3.1 grant no cluster-scoped resource, and §4.1 shows `create` denied on `namespaces`, `nodes`, `customresourcedefinitions`, `clusterroles`, and `clusterrolebindings` |
| The **hand-applied bootstrap** created 15 cluster-scoped objects, and did install a distribution — Flux | enumerated in §4.2 |
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

### 4.2 What the bootstrap created outside any namespace

The claim corrected here is the one most likely to mislead somebody else who runs a workload on this
host, so it is enumerated rather than characterised.

`kubectl apply --server-side -k clusters/hostinger/flux-system/` is applied **by hand, once**, and
is not reconciled from Git. It applies upstream Flux v2.9.4 unmodified plus the tenancy boundary.
Between them they create **15 cluster-scoped objects**:

| Kind | Count | Names |
|---|---|---|
| `Namespace` | 3 | `flux-system`, `renvor-site`, `renvor-site-staging` |
| `CustomResourceDefinition` | 7 | `buckets`, `externalartifacts`, `gitrepositories`, `helmcharts`, `helmrepositories`, `ocirepositories` — all `.source.toolkit.fluxcd.io` — and `kustomizations.kustomize.toolkit.fluxcd.io` |
| `ClusterRole` | 3 | `crd-controller-flux-system`, `flux-edit-flux-system`, `flux-view-flux-system` |
| `ClusterRoleBinding` | 2 | `cluster-reconciler-flux-system` (→ `cluster-admin`, subjects `kustomize-controller` **and** `helm-controller`), `crd-controller-flux-system` |

Counted from `rendered/clusters-hostinger-flux-system.yaml` at
`07bda7ad59c0e82bc441e4cb400d290cd60a882d`, which is the rendered form of exactly what is applied.

**Three consequences, stated rather than left for a reader to infer:**

1. **Flux did not pre-exist on this cluster.** The `flux-system` Namespace is created by this
   bootstrap. Saying the `cluster-admin` binding is "retained" was accurate about the *action*
   — the bootstrap does not remove it — and misleading about the *provenance*, because it reads as
   though the binding was already there. It was not. Renvor introduced it.
2. **Seven CRDs are a cluster-wide API surface change.** Any workload on this host can now create a
   `Kustomization` or a `GitRepository`, subject to its own RBAC. That is a property of the cluster
   Renvor changed.
3. **The `cluster-admin` binding covers `helm-controller` as well**, which the first version of §3
   did not say. `helm-controller` is not deployed here — §1 records that only `kustomize-controller`
   and `source-controller` run — but the binding names it, so if it were ever deployed into
   `flux-system` it would be `cluster-admin` on arrival with no further change.

None of this is created or modifiable by repository-driven reconciliation; §4.1's denials are what
establish that, and they remain correct. The distinction the corrected table now draws is between
**the bootstrap**, which is hand-applied and cluster-scoped, and **reconciliation**, which is
Git-driven and namespaced.

### 4.3 What could not be re-verified while writing this correction

`kubectl` on the maintainer's workstation is configured for a local context and the SSH profile for
`153.92.208.119` failed authentication during this change, so the cluster was **not** re-read. Every
figure in §3.1 and §4.2 therefore comes from the applied manifests in `renvor-rs/renvor-infra` at
`07bda7ad…`, which is stated above at each point rather than presented as a live capture. The live
`SubjectAccessReview` results in §4.1 are from the original 2026-08-17 capture and are unchanged by
this correction. **No credential was requested, created, or printed to restore that access.**

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
- **The cluster's API surface was changed and is not un-done by anything here.** The 7 Flux CRDs and
  the `cluster-admin` binding enumerated in §4.2 remain, and removing them would break the
  reconciliation this deployment depends on. No claim is made that Renvor's footprint on this shared
  host is reversible without downtime for Renvor.
- **The independent-review gap is untouched.** Every review supporting this deployment was an
  automated advisory review, explicitly **NON-INDEPENDENT**. W-001 through W-006 remain active.

## 8 — Documentation-site deployment, 2026-09-03

This section is a later observation. It does not rewrite the 2026-08-17 deployment record above;
in particular, the statements that `docs.renvor.dev` was not deployed and T108 remained open were
true on that date.

### 8.1 Source, image, and rollout

| Evidence | Observed value |
|---|---|
| Canonical source | `renvor-rs/renvor-docs` at `26eb0e414113e01c38827ded969f928a4c0b9fb5` |
| Published image | `ghcr.io/renvor-rs/renvor-docs@sha256:9240f8621a7bbfe735cb895298cc9fe6a75572e2e011a68d4405e11ee69ebfcd` |
| Publication | workflow run `33731148884`, including image scan, SBOM, provenance, and anonymous digest pull |
| Deployment source | `renvor-rs/renvor-infra`; reviewed staging and production overlays, reconciled by Flux |
| Runtime | one staging pod and two production pods Ready, zero restarts, all running the exact published digest |
| Public route | `https://docs.renvor.dev` returns the Docusaurus site; nonexistent routes return the site's 404 |

The documentation repository owns its npm lockfile, content controls, build, rendered-link check,
container verification, and publication. The framework repository is not in that runtime path and
does not clone or build the site.

### 8.2 TLS

The production endpoint serves a publicly trusted Let's Encrypt certificate issued by **YE1** with
`docs.renvor.dev` in its subject alternative names. The certificate expires **2026-12-02** and its
recorded renewal window begins **2026-11-02**. Verification through the public endpoint returned
`ssl_verify_result=0`; plain HTTP redirects to HTTPS.

### 8.3 T108 disposition

**001-T108 is resolved late, not recorded as having run on time.** ADR-0009 removed the vulnerable
`image-size` package from the resolved documentation dependency graph and installed the fail-closed
image-input guard. The two observations that could not be made before a documentation image
existed are now available:

- the production runtime image contains no `image-size` parser package; and
- the runtime SBOM contains no `image-size` package.

The image was also scanned before publication, pulled anonymously by immutable digest, and promoted
through staging before production. Those facts discharge T108's remaining substance. The process
timing did not meet PLAN §26.12: the replacement was deployed before the framework copy was removed,
so this is a late resolution and the mismatch is preserved explicitly.

### 8.4 What remains open

This deployment does not publish a crate, create a framework release or tag, complete the Phase 012
documentation programme, or close the companion repository's protection gap. `renvor-docs` has CI,
but as observed on 2026-09-03 its `main` branch has no protection rule and no required checks.


**Erratum (2026-09-06).** The CI context `verify (stable)` named in this record — and every `platform (…, stable)` context — compiled with the pinned **1.94.0**, not with current stable, from `98a4e2c` (2026-08-11) until the fix in pull request #64; only three runs (pull request #63's) were inspected directly, the window is inferred from configuration history, and every locally recorded `cargo +stable xtask verify` leg was genuine. See `phase-011-evidence.md` §14. This note is appended; nothing above it is edited.
