# Quickstart: Validating the Governance Foundation

**Feature**: `specs/001-governance-foundation` | **Date**: 2026-08-11

How to prove Phase 001 actually works. Each scenario maps to success criteria and produces an artifact for `governance/phase-001-evidence.md`. Run them in order — the gates are ordered for a reason.

## Prerequisites

| Requirement | Notes |
|---|---|
| Rust toolchain | Both the declared MSRV and current stable — see [support policy](./contracts/support-policy.md) |
| Node.js | Active LTS, version pinned in `.nvmrc` |
| `cargo-deny`, `gitleaks`, `lychee` | `cargo install --locked <tool>` |
| Hosting account | With rights to create an organization and configure protections |
| Package registry account | For name verification only — no publishing |

---

## Gate 0 — Repository is clean before a remote exists

**Validates**: SC-004, SC-005 (pre-creation scan) · **Blocks**: organization and repository creation, jointly with Gate 1

Run entirely locally. See the plan's Pre-Push Repository Cleanup section for the ordered stages and why the order matters.

```bash
# What would actually become public
git ls-tree -r HEAD --name-only          # review file by file
git diff --cached --name-status HEAD     # anything staged but uncommitted?

# After correcting ignore rules and unstaging
git status --porcelain                   # must print nothing

# After pruning
git count-objects -vH                    # size should drop to the reachable set

# Final scan of the state that will ship
gitleaks detect --no-banner --redact

# Branch name, before any remote exists
git branch --show-current                # must print: main
```

**Expected**: the tracked set matches the Stage 0 include/exclude decisions exactly; no `.idea/`, `.DS_Store`, or agent tool state anywhere in the index; working tree clean; object count reduced; zero secret findings recorded with date and tool version; branch is `main`.

**If `gitleaks` reports a finding**: stop. Rotate, revoke, purge, record, re-scan — in that order — before anything is created or pushed. A finding here is cheap; the same finding after the first push is permanent.

---

## Gate 1 — Names verified before anything is claimed

**Validates**: SC-001 · **Blocks**: organization and repository creation, jointly with Gate 0

```bash
# Check each package name; a 404 means available
curl -s -o /dev/null -w "%{http_code}\n" https://crates.io/api/v1/crates/renvor
curl -s -o /dev/null -w "%{http_code}\n" https://crates.io/api/v1/crates/renvor-cli
```

Record every result in `governance/name-availability.md` with location, date, status, and checker.

**Expected**: all ten rows present; every row `available` or `owned-by-project`; none older than 30 days.

**If any row is `held-by-other` or `ambiguous`**: **stop.** Do not proceed, do not substitute a name, do not commit a partial rename. Record an explicit naming decision first (FR-003).

---

## Gate 2 — Clean checkout verifies itself

**Validates**: SC-002, SC-003, SC-004, SC-016

```bash
git clone <repository> renvor-verify && cd renvor-verify
cargo xtask verify
git status --porcelain     # must print nothing
```

Then repeat on both toolchains — the fixed floor and the moving channel:

```bash
rustup run 1.94.0 cargo xtask verify     # the declared floor, pinned exactly
rustup run stable cargo xtask verify     # whatever stable currently is

# The floor is fixed, not derived — confirm it is stated, not computed
grep -r 'rust-version' crates/*/Cargo.toml Cargo.toml
grep 'resolver' Cargo.toml                # must show resolver = "3" explicitly
```

Then assert the MSRV has exactly one authoritative declaration:

```bash
# Exactly one literal declaration, at the workspace root
grep -c 'rust-version *= *"' Cargo.toml                  # must print 1
grep -rc 'rust-version *= *"' crates/*/Cargo.toml xtask/Cargo.toml   # must all print 0
grep -rc 'rust-version.workspace *= *true' crates/*/Cargo.toml       # members inherit
```

**Expected**: exit code 0 on both toolchains; every step ran; no step skipped; working tree clean afterwards; the MSRV is declared **once** at the workspace root with members inheriting it — a second literal declaration would pass a naive grep but violates FR-017; the workspace declares resolver 3 explicitly rather than relying on edition inheritance.

**Fixed-floor property**: when a newer Rust stable ships, only the second command's toolchain changes. The first stays pinned at 1.94.0 and no policy violation is recorded — that is the whole point of a fixed floor over a rolling window.

**Also test the failure path** — this is the requirement most likely to be quietly broken:

```bash
PATH=/usr/bin:/bin cargo xtask verify   # hide the optional tooling
```

**Expected**: exit code **2**, a message naming each missing tool and how to install it, and the line stating no checks were run. An exit code of 0 here means FR-023 is not actually implemented.

---

## Gate 3 — Governance is discoverable and the security path works

**Validates**: SC-006, SC-015

```bash
ls LICENSE-MIT LICENSE-APACHE SECURITY.md CONTRIBUTING.md \
   CODE_OF_CONDUCT.md GOVERNANCE.md SUPPORT.md
grep -c "license" crates/renvor/Cargo.toml
```

Then manually: from the rendered README, confirm all six governance documents are reachable in one link. Send a test report through the private path in `SECURITY.md` and confirm it arrives at a monitored contact.

**Expected**: all files present; `renvor` declares `MIT OR Apache-2.0`; six documents each one link away; the security report arrives.

---

## Gate 4 — The push is authorised by a scan of what is actually being pushed

**Validates**: SC-005 (pre-push scan) · **Blocks**: the first content push

Gate 0's scan ran before the workspace, policy files, governance documents, and workflows existed. It describes a repository that no longer exists. This gate re-scans the state actually about to become public.

```bash
# Fresh scan of the current tree and history
gitleaks detect --no-banner --redact

# Confirm the four push preconditions are met
test -f LICENSE-MIT && test -f LICENSE-APACHE && echo "licences present"
test -f SECURITY.md && echo "security contact present"
grep -c 'available\|owned-by-project' governance/name-availability.md   # all rows
```

**Expected**: zero findings, recorded with a **fresh** date distinct from the Gate 0 scan; both licence texts present; security contact live; every name row confirmed.

**If this scan finds something Gate 0 did not**, that is the gate working — a credential was introduced by one of the tasks in between. Rotate, revoke, purge, record, re-scan before pushing.

---

## Gate 5 — Repository protections are real, not configured-and-bypassable

**Validates**: SC-007, SC-008

```bash
gh api repos/{owner}/{repo} --jq '.visibility'
gh api repos/{owner}/{repo}/branches/main/protection
gh api repos/{owner}/{repo}/code-scanning/alerts --jq 'length'
grep -rn "uses:" .github/workflows/ | grep -v "@[0-9a-f]\{40\}"   # must print nothing
grep -L "permissions:" .github/workflows/*.yml                     # must print nothing
```

Then attempt the thing protection is supposed to stop:

```bash
git push origin main       # expected: rejected
```

**Expected**: visibility `public`; protection requires a pull request and the four named checks; `enforce_admins` true; every scanning control enabled; every third-party action pinned to a 40-character SHA; every workflow declares permissions; the direct push is refused.

**The one permitted waiver**: `required_approving_review_count: 0` under W-001 in `governance/waivers.md`. Any *other* waiver — especially a cost or availability one — means something did not go to plan, since research Finding 3 confirmed every control is free on a public repository.

---

## Gate 6 — Documentation builds and links resolve

**Validates**: SC-012

```bash
cd docs && npm ci && npm run build
lychee --no-progress 'build/**/*.html'
```

**Expected**: build succeeds; zero broken links; search returns results against the built output; the prose site and API documentation cross-link (FR-056).

---

## Gate 7 — The release rehearsal publishes nothing, and its identity controls exist

**Validates**: SC-010, SC-014

```bash
cargo package -p renvor --list        # inspect this output carefully
cargo package -p renvor
cargo publish --dry-run -p renvor
shasum -a 256 target/package/renvor-*.crate

# Prove the negative
curl -s https://crates.io/api/v1/crates/renvor | jq '.versions | length'

# Prove no long-lived credential exists
grep -ri "CARGO_REGISTRY_TOKEN" --include="*.yml" .github/
gh secret list
```

Then verify the release-identity controls — the half of SC-014 that is not about credentials:

```bash
# Tag signing configured and verifiable
git config --get gpg.format; git config --get user.signingkey
git tag -v "$(git tag --list | tail -1)" 2>&1 | head -3   # once a tag exists

# Protected release environment with NAMED approvers
gh api repos/{owner}/{repo}/environments --jq '.environments[].name'
gh api repos/{owner}/{repo}/environments/{env}/deployment_protection_rules

# Provenance and bill-of-materials wired into the release path
grep -rn "actions/attest\|cyclonedx\|sha256" .github/workflows/
```

**Expected**: the file list contains only intended files; the artifact builds; the dry run passes; the registry reports **zero** versions; `CARGO_REGISTRY_TOKEN` appears only as an environment binding fed by the OIDC action, never as a stored secret; `gh secret list` shows no registry credential; a signing key is configured and tags verify; a protected environment exists with **named individual** approvers and a tag-only deployment-branch restriction; SBOM, checksum, and attestation steps are present in the release path.

**Note**: trusted publishing itself cannot be configured in this phase — it requires a package that already exists on the registry, and nothing is published. Its absence here is expected, not a gap.

---

## Gate 8 — Evidence is complete

**Validates**: SC-009, SC-011, SC-013

Open `governance/phase-001-evidence.md` and confirm:

- one row per PLAN.md Phase 001 acceptance criterion **and** per SC-001…SC-015;
- every row has an evidence link, command, platform, operator, date, and result;
- `open_blockers` is empty;
- known limitations include the FR-049 residual risk (verified-but-unreserved package names), with a named owner and target phase;
- all four ADRs are `accepted`, each with a reviewer and review date;
- no runtime framework capability was implemented (review against FR-047).

**Expected**: 100% criterion coverage, zero unevidenced rows, zero open blockers.

---

## Summary map

| Gate | Success criteria | Produces |
|---|---|---|
| 0 Cleanup | SC-004, SC-005 | Publish-set decisions, corrected ignore rules, pre-creation secret scan |
| 1 Names | SC-001 | `governance/name-availability.md` |
| 2 Clean checkout | SC-002, SC-003, SC-004, SC-016 | Verification run logs, both toolchains, single-source MSRV proof |
| 3 Governance | SC-006, SC-015 | Document inventory, security-path test |
| 4 Push authorisation | SC-005 | Pre-push secret scan with a fresh date, precondition checks |
| 5 Protections | SC-007, SC-008 | Protection baseline snapshot, waiver W-001 |
| 6 Documentation | SC-012 | Build log, link-check report |
| 7 Release rehearsal | SC-010, SC-014 | Artifact, checksum, file list, zero-version proof, signing and environment evidence |
| 8 Evidence | SC-009, SC-011, SC-013 | `governance/phase-001-evidence.md` |

**Four blocking gates**, in order:

| Gate | Blocks |
|---|---|
| 0 Cleanup | Organization and repository creation — jointly with Gate 1 |
| 1 Names | Organization and repository creation — jointly with Gate 0 |
| 4 Push authorisation | The first content push |
| 8 Evidence | Entry to Phase 002 |

Gates 0 and 1 are **both** preconditions of creating anything public: Gate 0 proves the local state is safe, Gate 1 proves the names are ours to take. Neither alone is sufficient. Everything else happens between the gates.

Gates 0 and 4 both run a secret scan, and both are required. They are not redundant — roughly twenty file-creating tasks separate them, so Gate 0's result says nothing about the state Gate 4 authorises.
