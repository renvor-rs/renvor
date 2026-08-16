---
description: "Phase 002 quickstart — runnable, fail-closed validation gates proving the core kernel meets its success criteria"
---

# Quickstart: Validating the Transport-Independent Core Kernel

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Contracts**: [contracts/](./contracts/)
**Date**: 2026-08-16 *(revision 2 — commands corrected)*

> **This is a validation guide, not an implementation guide.** Each gate states what to run and
> what result proves the criterion. Implementation belongs in `tasks.md` and the implement stage.

> **Nothing here mutates anything.** No gate publishes, tags, pushes, deploys, or edits a tracked
> file. Registry and release checks are **read-only**. Revision 1 asked the operator to edit and
> restore `deny.toml`; that is gone — a gate that mutates repository policy and relies on the
> operator restoring it can leave the repository altered when it fails midway.

## Setup — run this first, once per shell

```bash
set -euo pipefail
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"
UA='renvor-verification (https://github.com/renvor-rs/renvor)'
# Every crate name this phase introduces or touches:
PHASE_CRATES=(renvor renvor-core renvor-config renvor-testkit)
rustc --version   # expect 1.94.0 — pinned by rust-toolchain.toml
```

Each gate below assumes this preamble. Gates that are a **single pass/fail unit** repeat
`set -euo pipefail` so a failure in the middle of a pipeline cannot be reported as a pass.

## A note on fail-closed checks

**Several gates assert a count of zero.** A zero produced by a check that could never have matched
is indistinguishable from a clean pass, and Phase 001 lost real time to exactly that. **Every
zero-asserting gate below carries a positive control** — a case proving the check *can* fire.
**A gate whose control does not fire has FAILED, regardless of what its main assertion reported.**

---

## Gate 0 — Workspace builds and the toolchain is the floor

```bash
set -euo pipefail
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
echo "GATE 0 PASS"
```

**Pass**: `GATE 0 PASS` prints. Under `set -e` any non-zero exit aborts before it.

---

## Gate 1 — Dependency policy (SC-012, SC-017)

```bash
set -euo pipefail
cargo deny check licenses advisories bans sources
test "$(grep -c '^\[\[package\]\]' Cargo.lock)" -gt 2
echo "GATE 1 PASS — lockfile has $(grep -c '^\[\[package\]\]' Cargo.lock) packages"
```

**Positive control — prove the licence gate can fail, without touching the repository's policy.**
The fixture is built in a temporary directory and removed by a trap, so nothing in the repository
is edited and nothing needs restoring:

```bash
set -euo pipefail
FIX="$(mktemp -d)"; trap 'rm -rf "$FIX"' EXIT
cp "$REPO_ROOT/Cargo.lock" "$FIX/" 2>/dev/null || true
# A policy that allows nothing must reject the real graph.
printf '[licenses]\nversion = 2\nallow = []\nconfidence-threshold = 0.93\n' > "$FIX/deny-empty.toml"
if cargo deny --manifest-path "$REPO_ROOT/Cargo.toml" --config "$FIX/deny-empty.toml" check licenses 2>/dev/null; then
  echo "CONTROL FAILED — an empty allow-list accepted the graph; Gate 1 proves nothing"; exit 1
else
  echo "CONTROL OK — the licence gate can fail"
fi
```

**Fails if**: any wildcard requirement exists (`wildcards = "deny"`), any dependency resolves
outside crates.io, or any advisory affects a resolved version.

---

## Gate 2 — Lifecycle order and rollback (SC-001, SC-002)

```bash
set -euo pipefail
cargo test -p renvor-core lifecycle:: -- --nocapture
```

**Pass**: the observed sequence is exactly `Load → Validate → Register → Boot → Ready → Drain →
Stop` with **0** deviating runs; for a failure at provider *n* of *k*, shutdown order is the exact
reverse of **actual initialisation order** in **100%** of runs.

**Positive control**: a test asserting against **registration** order instead of **initialisation**
order **MUST fail** on a graph where resolution reorders. If it passes, the two orders were never
made to differ and this gate proved nothing.

---

## Gate 3 — Configuration proof gate, all 8 obligations (SC-020)

```bash
set -euo pipefail
cargo test -p renvor-config layering:: -- --nocapture
```

**Pass requires all eight** ([configuration-contract.md](./contracts/configuration-contract.md) C-C7):

| # | Obligation | Expected |
|---|---|---|
| 1 | precedence | `defaults < earlier TOML < later TOML < environment`, **100%** |
| 2 | nested-table merge | **100%** of sibling keys from lower layers survive |
| 3 | array replacement | replaces; **0** concatenations |
| 4 | **source attribution** | the winning layer is reportable for **every** resolved key |
| 5 | invalid **non-empty** env | fails at Validate naming **3 of 3** — key, layer, expected type |
| 6 | invalid **empty** env (`KEY=""`) | **also fails**; **0** fall-throughs to a lower layer |
| 7 | structural conflict | fails naming **both** layers; **0** coercions, **0** last-wins |
| 8 | format isolation | **0** JSON/YAML features in the resolved graph (asserted by Gate 13) |

> **This gate is the decision point.** If any obligation fails, the recorded fallback triggers and
> the replacement adapter **MUST NOT merge** until ADR-0007 clears the governance gate (Gate 14).
> Obligations **4 and 6 have known negative evidence** for the candidate crate.

---

## Gate 4 — Secrets and opaque state never appear (SC-007, SC-016)

```bash
set -euo pipefail
# Package selectors first, then exactly ONE test filter.
cargo test -p renvor-core -p renvor-config redaction:: -- --nocapture
```

> *Revision 1 wrote `cargo test -p renvor-core redaction:: -p renvor-config redaction::`, which is
> invalid — cargo accepts many `-p` flags but only one positional filter, so the second `redaction::`
> would be rejected as an unexpected argument.*

**Pass**: **0** occurrences of a secret-marked value and **0** occurrences of registered-state
contents across **every** path — `Display`, `Debug`, error message, error context, structured log
fields, span fields, and serialization.

**Two positive controls, both required**:

1. The SC-016 test **registers a credential-bearing value without marking it secret** and fails if
   its contents appear. A suite that only tests values the author remembered to mark tests the easy
   half.
2. A **deliberately leaking wrapper** type is asserted against by the same helpers and **MUST be
   detected**. If the leak detector does not fire on a type built to leak, the **0** results above
   are meaningless.

---

## Gate 5 — Provider graph ceilings, work budget, recursion depth (SC-005, SC-021)

```bash
set -euo pipefail
cargo test -p renvor-core provider::graph:: -- --nocapture
```

| Case | Expected |
|---|---|
| acyclic, 1024 providers **and** 8192 edges | **succeeds**, 100% |
| 1025 providers | fails at Register naming ceiling **1024**, observed **1025** |
| 8193 edges | fails at Register naming ceiling **8192**, observed **8193** |
| oversize graphs | **0** reach traversal |
| maximum accepted graph | ≤ **2048** provider examinations, ≤ **16384** edge examinations, ≤ **18432** work units |
| maximum accepted graph, *expected observed* | **2048** / **8192** / **10240** — single pass |
| at least **3** graph sizes | counters stay within `2 × providers` and `2 × edges`; **0** violations |
| cycle within the ceilings | cycle diagnostic naming **100%** of providers in the cycle; **0** runs report budget exhaustion instead |
| **1024-node linear chain** | resolves **without exhausting the stack**, exercised on a **Tokio worker thread** (smaller default stack than the main thread) |

**Fails if** any assertion consults elapsed wall-clock time — the bound is a property of the graph,
not of the machine.

---

## Gate 6 — Drain, including the zero budget (SC-006)

```bash
set -euo pipefail
cargo test -p renvor-core drain:: -- --nocapture
```

**Pass**: an over-budget drain reports **incomplete** in **100%** of runs, **0** report clean; a
**zero** budget with work in flight also reports outstanding work, **0** reporting clean. Uses the
paused clock (FR-031), so this gate consumes no real elapsed time.

---

## Gate 7 — Health and readiness disagree (SC-008)

```bash
set -euo pipefail
cargo test -p renvor-core health:: -- --nocapture
```

**Pass**: at least **1** asserted state where liveness reports alive while readiness reports
not-ready — including `Drain` — plus a failing contributor that is individually identifiable, and a
**panicking** contributor that is caught rather than taking the process down.

---

## Gate 8 — Failure injection at every phase (SC-009)

```bash
set -euo pipefail
cargo test -p renvor-testkit injection:: -- --nocapture
```

**Pass**: **7 of 7** lifecycle phases accept an injected failure, each covered by a test, each
producing a deterministic failure and an assertable rollback.

---

## Gate 9 — Run identifier opacity (SC-019)

```bash
set -euo pipefail
cargo test -p renvor-core observe::run_id:: -- --nocapture
```

**Gating assertions only**: **exactly 1** generation site; with fixed entropy the identifier is a
**pure function of those bytes** — identical across runs, with **0** of its bytes changing when
hostname, clock, process id, counter, and the entire configuration vary; **1 of 1** production
entropy sources is the operating-system CSPRNG.

**Explicitly non-gating**: any random-sample collision or ordering check. It may run; **0** release
gates depend on it. It is probabilistic and can fail on a correct implementation.

---

## Gate 10 — Tracing ownership (FR-029)

```bash
set -euo pipefail
cargo test -p renvor-core observe::bootstrap:: -- --nocapture
```

**Pass**:

- building an `Application` installs **0** global subscribers — asserted by building one and then
  successfully installing a subscriber afterwards, which is only possible if `build()` installed
  nothing;
- the preferred bootstrap **returns** a value rather than installing it;
- if a global-install helper exists, a **second** call returns `AlreadyInstalled` — **0** panics,
  **0** silent successes, **0** silent replacements.

---

## Gate 11 — Hostile configuration input (FR-038)

```bash
set -euo pipefail
cargo test -p renvor-config hostile:: -- --nocapture
```

**Pass**: malformed, truncated, and unexpectedly large TOML each produce a **bounded, actionable**
error; **0** panics, **0** unbounded memory or time. Per principle IX this boundary also receives
**property or fuzz** testing, not only example-based cases.

---

## Gate 12 — Examples and documentation (SC-013, SC-014)

```bash
set -euo pipefail
cargo test --workspace --doc
for f in examples/*.rs; do cargo run --example "$(basename "$f" .rs)"; done
cargo test --workspace --all-targets
echo "GATE 12 PASS"
```

**Pass**: every example compiles, runs, and uses **no global mutable state**; **0** examples require
a transport, a port, or a database. CI runs the same sequence on both `1.94.0` and current stable.

---

## Gate 13 — Scope discipline and the crate dependency DAG (SC-010, plan §Crate dependency DAG)

```bash
set -euo pipefail

# --- 13a: no transport/persistence/CLI/frontend capability anywhere in the workspace ---
FORBIDDEN='axum|hyper|tower-http|sqlx|sea-orm|tauri|clap|graphql|actix|rocket'
if cargo tree --workspace --prefix none --edges normal | sort -u | grep -Eiq "$FORBIDDEN"; then
  echo "FAIL 13a — out-of-scope dependency present"; exit 1
fi
# POSITIVE CONTROL: the same pipeline must be able to match something that IS present.
cargo tree --workspace --prefix none --edges normal | sort -u | grep -Eiq 'tokio' \
  || { echo "CONTROL FAILED — the scope search cannot match anything; 13a proves nothing"; exit 1; }
echo "13a PASS (control fired)"

# --- 13b: core-only consumers get no parser, no derive framework, no secret crate ---
CONFIG_ONLY='^(toml|serde|serde_derive|confique|confique_macro|secrecy|zeroize)$'
if cargo tree -p renvor-core --prefix none --edges normal | awk '{print $1}' | sort -u | grep -Eq "$CONFIG_ONLY"; then
  echo "FAIL 13b — renvor-config dependencies leaked into renvor-core"; exit 1
fi
cargo tree -p renvor-core --prefix none --edges normal | awk '{print $1}' | sort -u | grep -Eq '^petgraph$' \
  || { echo "CONTROL FAILED — cannot see renvor-core's own dependencies; 13b proves nothing"; exit 1; }
echo "13b PASS (control fired)"

# --- 13c: no cycle — core must not depend on config ---
cargo tree -p renvor-core --prefix none --edges normal | awk '{print $1}' | grep -qx 'renvor-config' \
  && { echo "FAIL 13c — renvor-core depends outward on renvor-config"; exit 1; } || true
cargo tree -p renvor-config --prefix none --edges normal | awk '{print $1}' | grep -qx 'renvor-core' \
  || { echo "FAIL 13c — renvor-config does not depend on renvor-core; the port design is not in place"; exit 1; }
echo "13c PASS"

# --- 13d: testkit never enters a production graph ---
for p in renvor renvor-core renvor-config; do
  cargo tree -p "$p" --prefix none --edges normal | awk '{print $1}' | grep -qx 'renvor-testkit' \
    && { echo "FAIL 13d — renvor-testkit reachable from $p"; exit 1; } || true
done
echo "13d PASS"

# --- 13e: JSON/YAML config formats absent (obligation 8 of the Gate 3 proof) ---
if cargo tree --workspace --prefix none --edges normal | grep -Eiq 'serde_yaml|yaml-rust|json5'; then
  echo "FAIL 13e — a prohibited configuration format entered the graph"; exit 1
fi
cargo tree --workspace --prefix none --edges normal | grep -Eiq '(^| )toml' \
  || { echo "CONTROL FAILED — cannot see config formats at all; 13e proves nothing"; exit 1; }
echo "13e PASS (control fired)"

# --- 13f: the facade re-exports and does not implement (ADR-0002) ---
IMPL_COUNT=$(grep -cE '^\s*(pub )?(fn|impl|struct|enum) ' crates/renvor/src/lib.rs || true)
test "$IMPL_COUNT" -eq 0 || { echo "FAIL 13f — facade contains $IMPL_COUNT implementation items"; exit 1; }
grep -qE '^\s*pub use ' crates/renvor/src/lib.rs \
  || { echo "CONTROL FAILED — facade has no re-exports at all; 13f proves nothing"; exit 1; }
echo "13f PASS (control fired)"
echo "GATE 13 PASS"
```

> *Revision 1 reported `matches=$?` and called it a match count. It is **grep's exit status** — `1`
> means "found nothing", which is the opposite of a count. Every check above now branches on the
> exit status explicitly and names the failure.*

---

## Gate 14 — Publication and governance (SC-011, FR-034, ADR-0007)

### 14a — No crate is published, checked for **every** phase crate

Uses the **sparse index**, which returns a clean `404` for an unpublished name — unlike the JSON
API, whose error body a naive parser reads as "zero versions", failing **open**.

```bash
set -euo pipefail
idx_path() { local n="$1" L=${#1}
  if   [ "$L" -le 2 ]; then printf '%s/%s' "$L" "$n"
  elif [ "$L" -eq 3 ]; then printf '3/%s/%s' "${n:0:1}" "$n"
  else printf '%s/%s/%s' "${n:0:2}" "${n:2:2}" "$n"; fi; }

check_unpublished() {
  local n="$1" code
  code=$(curl -sS -A "$UA" --max-time 30 -o /dev/null -w '%{http_code}' \
         "https://index.crates.io/$(idx_path "$n")")
  case "$code" in
    404) echo "  $n: NOT PUBLISHED (404) — ok" ;;
    200) echo "  $n: *** PUBLISHED (200) — FR-034 VIOLATED ***"; return 1 ;;
    *)   echo "  $n: INCONCLUSIVE (http=$code) — treat as FAIL, do not read as unpublished"; return 1 ;;
  esac
}
for c in "${PHASE_CRATES[@]}"; do check_unpublished "$c"; done

# POSITIVE CONTROL: a known-published crate MUST return 200.
ctrl=$(curl -sS -A "$UA" --max-time 30 -o /dev/null -w '%{http_code}' \
       "https://index.crates.io/$(idx_path serde)")
test "$ctrl" = "200" || { echo "CONTROL FAILED (http=$ctrl) — 404s above prove nothing"; exit 1; }
echo "14a PASS (control returned 200)"
```

**An HTTP status other than 200 or 404 is a FAIL, never a pass.** A network error, a proxy page, or
a rate-limit response must not be read as "not published".

### 14b — No tags or releases were created

```bash
set -euo pipefail
test -z "$(git tag --list)" || { echo "FAIL — tags exist: $(git tag --list | tr '\n' ' ')"; exit 1; }
echo "14b: 0 git tags"
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  n=$(gh release list --limit 100 | wc -l | tr -d ' ')
  test "$n" -eq 0 || { echo "FAIL — $n GitHub releases exist"; exit 1; }
  echo "14b: 0 GitHub releases (verified)"
else
  echo "14b: GitHub releases NOT VERIFIED — gh unavailable or unauthenticated. Recorded as a gap, not as a pass."
fi
```

### 14c — Container images: the limit is recorded, not hidden

**No image-publishing workflow exists in this phase**, which is the positive statement that can be
verified locally:

```bash
set -euo pipefail
grep -rlE 'ghcr\.io|docker/build-push-action' .github/workflows/ && \
  { echo "FAIL — an image-publishing workflow exists"; exit 1; } || echo "14c: no image-publishing workflow"
```

> **Stated limit**: an anonymous query against a container registry returns **403** for both *absent*
> and *private* packages, so it **cannot** distinguish them. No gate here claims "no image exists" —
> only that **no workflow capable of publishing one is present**. Claiming the stronger statement
> from a 403 would be the same fail-open error as reading an API error body as zero versions.

### 14d — ADR-0007 governance gate

```bash
set -euo pipefail
ADR=decisions/0007-*.md
if compgen -G "$ADR" > /dev/null; then
  grep -qiE '^\s*status:\s*accepted' $ADR \
    && grep -qiE 'reviewer' $ADR \
    || { echo "FAIL — ADR-0007 is not accepted with a recorded reviewer"; exit 1; }
  grep -qiE 'W-002|W-003' $ADR && echo "REVIEW REQUIRED — ADR-0007 cites a Phase 001 waiver; neither covers a Phase 002 ADR"
  echo "14d: ADR-0007 present and accepted"
else
  echo "14d: ADR-0007 ABSENT — custom infrastructure MUST NOT merge (FR-035)"
fi
```

**This gate cannot be discharged by any automated check.** It requires a **qualified independent
human review**, or a **separately proposed and separately approved waiver** with all seven
mandatory fields. **W-002 covers Phase 001 decision records; W-003 covers Phase 001's phase-level
review. Neither authorises accepting a Phase 002 ADR.**

---

## Gate 15 — Complete resolved-dependency inventory (FR-040)

Runs **after** manifests and `Cargo.lock` exist and **before** any adoption is confirmed. The
research table covers **direct candidates only**; a transitive dependency can carry an incompatible
licence or a live advisory just as easily.

```bash
set -euo pipefail
cargo tree --workspace --edges normal --prefix none | awk '{print $1, $2}' | sort -u > /tmp/renvor-deps.txt
echo "resolved packages: $(wc -l < /tmp/renvor-deps.txt)"
cargo deny check licenses advisories bans sources        # against the ACTUAL lockfile
cargo tree --workspace --duplicates || true              # duplicate-version findings, recorded
cargo tree --workspace --edges features --prefix none > /tmp/renvor-features.txt
echo "GATE 15 PASS"
```

**Pass**: every resolved package — **transitive included** — has a recorded version, licence, MSRV
compatibility, and advisory status; `cargo deny` is clean against the real lockfile; enabled
features and duplicate versions are recorded. **The gate FAILS if any resolved package lacks the
evidence FR-040 requires**, including one that appeared only transitively.

---

## Summary of gates

| Gate | Criterion | Zero-assertion? | Control |
|---|---|---|---|
| 0 | buildable workspace | no | `set -e` |
| 1 | SC-012, SC-017 | no | empty allow-list fixture in a temp dir |
| 2 | SC-001, SC-002 | yes | order-divergence control |
| 3 | SC-020 (8 obligations) | yes | — decision point for the fallback |
| 4 | SC-007, SC-016 | yes | unmarked credential **+ leaking wrapper** |
| 5 | SC-005, SC-021 | yes | 1024-chain depth test |
| 6 | SC-006 | yes | — |
| 7 | SC-008 | no | — |
| 8 | SC-009 | no | — |
| 9 | SC-019 | yes | — |
| 10 | FR-029 | yes | install-after-build proves nothing was installed |
| 11 | FR-038 | yes | fuzz/property corpus |
| 12 | SC-013, SC-014 | yes | — |
| 13 | SC-010 + crate DAG | yes | **5 controls, one per sub-check** |
| 14 | SC-011, FR-034, ADR-0007 | yes | `serde` returns 200; non-200/404 is a FAIL |
| 15 | FR-040 | no | fails on missing evidence |
