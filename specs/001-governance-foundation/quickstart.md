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

# Final scan of the state that will ship.
# `gitleaks detect` was removed in gitleaks 8.x: `git` scans history, `dir` scans the
# working tree INCLUDING untracked files. Both are required — a secret that is untracked
# today is one `git add` away from being permanent, and `git` alone would never see it.
gitleaks git --redact --no-banner .      # history
gitleaks dir --redact --no-banner .      # working tree, untracked included
gitleaks version                         # record the version with the result

# Branch name, before any remote exists
git branch --show-current                # must print: main
```

**Expected**: the tracked set matches the Stage 0 include/exclude decisions exactly; no `.idea/`, `.DS_Store`, or agent tool state anywhere in the index; working tree clean; object count reduced; **both** secret scans exit 0 with zero findings, each recorded with its own date and the tool version; branch is `main`.

**Record the exit code, not just the message.** `gitleaks` exits 0 for a clean scan and non-zero when it finds something; a scan whose exit code was never captured is not evidence. Capture it immediately — a shell's `$?` and `PIPESTATUS` are reset by the very next command, including a bare assignment:

```bash
gitleaks git --redact --no-banner . ; rc_git=$?
gitleaks dir --redact --no-banner . ; rc_dir=$?
[ "$rc_git" = 0 ] && [ "$rc_dir" = 0 ] || { echo "GATE 0 FAILED"; exit 1; }
```

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
set -euo pipefail
for f in LICENSE-MIT LICENSE-APACHE SECURITY.md CONTRIBUTING.md \
         CODE_OF_CONDUCT.md GOVERNANCE.md SUPPORT.md; do
  test -f "$f" || { echo "FAIL: missing $f"; exit 1; }
done

# Assert the declared terms, not merely that the word "license" appears somewhere.
grep -qE '^license *= *"MIT OR Apache-2.0"' crates/renvor/Cargo.toml \
  || { echo "FAIL: renvor does not declare MIT OR Apache-2.0"; exit 1; }
echo "GATE 3 file and licence checks PASS"
```

Then manually: from the rendered README, confirm all six governance documents are reachable in one link. Send a test report through the private path in `SECURITY.md` and confirm it arrives at a monitored contact.

**Expected**: all files present; `renvor` declares `MIT OR Apache-2.0`; six documents each one link away; the security report arrives.

---

## Gate 4 — The push is authorised by a scan of what is actually being pushed

**Validates**: SC-005 (pre-push scan) · **Blocks**: the first content push

Gate 0's scan ran before the workspace, policy files, governance documents, and workflows existed. It describes a repository that no longer exists. This gate re-scans the state actually about to become public.

```bash
# Fresh scan of the current tree AND history. Both subcommands, both exit codes.
gitleaks git --redact --no-banner . ; rc_git=$?
gitleaks dir --redact --no-banner . ; rc_dir=$?
[ "$rc_git" = 0 ] && [ "$rc_dir" = 0 ] || { echo "GATE 4 FAILED — do not push"; exit 1; }

# Confirm the four push preconditions are met. Fail closed: `test` and `grep -c` are
# asserted, never eyeballed, because a passing-looking command that printed nothing is
# indistinguishable from a check that never ran.
test -f LICENSE-MIT && test -f LICENSE-APACHE || { echo "FAIL: licence text missing"; exit 1; }
test -f SECURITY.md || { echo "FAIL: security contact missing"; exit 1; }
rows=$(grep -c 'available\|owned-by-project' governance/name-availability.md)
[ "$rows" -gt 0 ] || { echo "FAIL: zero name rows matched — wrong file or wrong pattern"; exit 1; }
echo "name rows confirmed: $rows"
```

**Expected**: **both** scans exit 0 with zero findings, recorded with a **fresh** date distinct from the Gate 0 scan; both licence texts present; security contact live; every name row confirmed with a **non-zero** count.

> **A zero-result check is not a passing check until you have proved it can fail.** Before trusting any `grep` used as a gate, run it against a string you know is present and confirm it matches. An empty result from a mistyped pattern, a wrong path, or an unquoted shell variable looks exactly like a clean result.

**If this scan finds something Gate 0 did not**, that is the gate working — a credential was introduced by one of the tasks in between. Rotate, revoke, purge, record, re-scan before pushing.

---

## Gate 5 — Repository protections are real, not configured-and-bypassable

**Validates**: SC-007, SC-008

**Every command in this gate is read-only.** *(Revised 2026-08-15.)* An earlier version ended with `git push origin main` "expected: rejected". That is a **write attempt** against a protected production branch: it depends on protection being correctly configured to be safe, which is the very thing under test, and a misconfiguration turns the check into the incident. Protection is verified by **reading the settings**, not by trying to break them.

```bash
set -euo pipefail
OWNER=renvor-rs REPO=renvor

# Visibility
[ "$(gh api "repos/$OWNER/$REPO" --jq '.visibility')" = public ] \
  || { echo "FAIL: not public"; exit 1; }

# Protection — assert each control rather than printing the blob
prot=$(gh api "repos/$OWNER/$REPO/branches/main/protection")
echo "$prot" | jq -e '.required_pull_request_reviews != null'      >/dev/null || { echo "FAIL: PR not required"; exit 1; }
echo "$prot" | jq -e '.enforce_admins.enabled == true'             >/dev/null || { echo "FAIL: admins can bypass"; exit 1; }
echo "$prot" | jq -e '.required_status_checks.strict == true'      >/dev/null || { echo "FAIL: checks not strict"; exit 1; }
echo "$prot" | jq -e '.allow_force_pushes.enabled == false'        >/dev/null || { echo "FAIL: force push allowed"; exit 1; }
echo "$prot" | jq -e '.allow_deletions.enabled == false'           >/dev/null || { echo "FAIL: deletion allowed"; exit 1; }
echo "$prot" | jq -e '(.required_status_checks.contexts | length) >= 4' >/dev/null || { echo "FAIL: fewer than 4 required checks"; exit 1; }

# Scanning controls
gh api "repos/$OWNER/$REPO/code-scanning/alerts" --jq 'length' >/dev/null \
  || { echo "FAIL: code scanning unreachable"; exit 1; }

# Workflow hygiene — FAIL CLOSED. Count first: an empty directory, a wrong path, or an
# unquoted variable makes both greps print nothing, which is indistinguishable from a pass.
wf_count=$(find .github/workflows -maxdepth 1 -name '*.yml' | wc -l | tr -d ' ')
[ "$wf_count" -gt 0 ] || { echo "FAIL: zero workflow files found — wrong path, not a clean result"; exit 1; }
echo "workflow files: $wf_count"

unpinned=$(grep -rn "uses:" .github/workflows/ | grep -v "@[0-9a-f]\{40\}" || true)
[ -z "$unpinned" ] || { echo "FAIL: unpinned actions:"; echo "$unpinned"; exit 1; }

nopermissions=$(grep -L "permissions:" .github/workflows/*.yml || true)
[ -z "$nopermissions" ] || { echo "FAIL: workflows without permissions:"; echo "$nopermissions"; exit 1; }

echo "GATE 5 PASS"
```

**Expected**: visibility `public`; protection requires a pull request and at least the four named checks; `enforce_admins` true; force pushes and deletions blocked; every scanning control enabled; every third-party action pinned to a 40-character SHA; every workflow declares permissions.

> **Do not test protection by attempting a push.** If a read shows protection is absent, that is the finding — configure it and re-read. A successful direct push to `main` is not a useful experiment; it is an unreviewed change to the production branch.

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

- one row per PLAN.md Phase 001 acceptance criterion **and** per **SC-001 through SC-016** — *(corrected 2026-08-15: this read "SC-001…SC-015" and silently omitted **SC-016**, the single-source MSRV and resolver-3 criterion)*;
- every row has an evidence link, command, platform, operator, date, and result;
- `open_blockers` is empty, **or** every remaining entry is explicitly categorised as transferred, waived, or cancelled with an owner and a destination — an open blocker may not be closed by rewording it;
- known limitations include the FR-049 residual risk (verified-but-unreserved package names), with a named owner and target phase;
- **all six decision records — ADR-0001 through ADR-0006 — are accounted for**, each either `accepted` with a reviewer and a review date, or explicitly `proposed` with the named blocking task that keeps it there. *(Corrected 2026-08-15: this read "all four ADRs", which predates ADR-0005 and ADR-0006. It must never be read as permission to ignore the two it omitted.)* Under **W-002** the reviewer field reads exactly `Ahmed Anbar — self-review under W-002` and **must not be described as independent**;
- **the task counts are recomputed by ID and explicit status**, not by counting checkboxes, and completed / open / transferred / waived / cancelled are reported as separate figures — see "How to count the tasks in this file" in `tasks.md`;
- no runtime framework capability was implemented (review against FR-047).

**Expected**: 100% criterion coverage, zero unevidenced rows, and every blocker either closed or explicitly categorised.

> **Every gate must be run fresh for the run being recorded.** *(Added 2026-08-15.)* Historical evidence from an earlier pass may be **cited** as history, but it may never be **reused** as the result of the current run: the tree, the toolchain, the dependency set, and the live GitHub state all move. Each gate's evidence carries its own date, and a date that predates the commit under evaluation is a failed gate, not a passing one. This is the same reasoning that makes Gates 0 and 4 both mandatory rather than redundant.

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
| 8 Evidence | SC-009, SC-011, SC-013; coverage of SC-001…**SC-016** | `governance/phase-001-evidence.md` |

**Four blocking gates**, in order:

| Gate | Blocks |
|---|---|
| 0 Cleanup | Organization and repository creation — jointly with Gate 1 |
| 1 Names | Organization and repository creation — jointly with Gate 0 |
| 4 Push authorisation | The first content push |
| 8 Evidence | Entry to Phase 002 |

Gates 0 and 1 are **both** preconditions of creating anything public: Gate 0 proves the local state is safe, Gate 1 proves the names are ours to take. Neither alone is sufficient. Everything else happens between the gates.

Gates 0 and 4 both run a secret scan, and both are required. They are not redundant — roughly twenty file-creating tasks separate them, so Gate 0's result says nothing about the state Gate 4 authorises.
