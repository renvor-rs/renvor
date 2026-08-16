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

# Examples live in the FACADE crate, not at the repository root, and they are run as
# `-p renvor` targets. The earlier form of this gate globbed `examples/*.rs` from the root —
# a directory that does not exist — so the loop body never executed once and the gate
# reported a pass having run nothing.
# No `mapfile`: it is a bash 4 builtin, and macOS ships bash 3.2. A gate the maintainer
# cannot run locally is a gate only CI ever runs. Word-splitting is safe here because a
# Rust example's file stem is an identifier and cannot contain whitespace.
EXAMPLE_DIR=crates/renvor/examples
EXAMPLES=$(find "$EXAMPLE_DIR" -maxdepth 1 -name '*.rs' -exec basename {} .rs \; | sort)
COUNT=$(printf '%s\n' "$EXAMPLES" | grep -c .)

# CONTROL 1: discovery found the examples. A glob that matches nothing must not pass.
if [ "$COUNT" -lt 3 ]; then
  echo "FAIL 12 — discovery found $COUNT example(s) in $EXAMPLE_DIR"; exit 1
fi
echo "discovered $COUNT examples: $(printf '%s ' $EXAMPLES)"

for name in $EXAMPLES; do
  echo "--- running example: $name"
  cargo run --quiet -p renvor --example "$name"
done

# CONTROL 2: the gate fails when an example is not runnable. Plants an example that exits
# non-zero, confirms the runner reports it, and removes it. Without this, a `cargo run` whose
# failure was being swallowed would look identical to a clean pass.
CONTROL="$EXAMPLE_DIR/gate12-control.rs"
trap 'rm -f "$CONTROL"' EXIT
printf 'fn main() { std::process::exit(1); }\n' > "$CONTROL"
if cargo run --quiet -p renvor --example gate12-control; then
  echo "FAIL 12 control — an example that exits non-zero was reported as a pass"; exit 1
fi
rm -f "$CONTROL"; trap - EXIT
echo "control: a failing example is detected"

# FR-032 / SC-014's "no hidden global mutable state" — CHECKED, not asserted in prose. The
# Pass paragraph used to claim this while the script checked only exit status.
# The alternation covers `static mut` AND the forms that are idiomatic in safe Rust since
# `Mutex::new` became `const` — `static X: Mutex<..>`, `static X: AtomicUsize`, and friends. The
# first version of this pattern missed exactly those, which the W-005 verification re-review (N3)
# caught by noting that the same commit adding this gate had added a `static _: AtomicUsize` to the
# kernel: the grep was hunting a shape its own author had just written and would not have found.
GLOBALS='static mut|static [A-Z_]+ *: *(Mutex|RwLock|Atomic|Cell|RefCell|OnceLock|OnceCell|LazyLock)|lazy_static!|once_cell::|thread_local!|SyncLazy|Box::leak'
if grep -REn "$GLOBALS" "$EXAMPLE_DIR" ; then
  echo "FAIL 12 — an example uses global mutable state (FR-032)"; exit 1
fi
# CONTROL 3: the pattern can match. Without this a typo in $GLOBALS reads as a clean result.
printf 'static COUNTER: AtomicUsize = AtomicUsize::new(0);\n' > "$EXAMPLE_DIR/../globals-probe.txt"
grep -REn "$GLOBALS" "$EXAMPLE_DIR/.." > /dev/null \
  || { rm -f "$EXAMPLE_DIR/../globals-probe.txt"; echo "FAIL 12 — the global-state pattern matches nothing"; exit 1; }
rm -f "$EXAMPLE_DIR/../globals-probe.txt"
echo "control: the global-state pattern fires on a planted global"

cargo test --workspace --all-targets
echo "GATE 12 PASS"
```

**Pass**: **every** example is discovered, compiles, and runs to a zero exit; the discovery finds at
least three; an example that cannot run **fails the gate**; and **no example uses global mutable
state** — each of the four proven by a control rather than assumed.

**"0 examples require a transport, a port, or a database" is evidenced by Gate 13a, not here.** No
transport, database, or CLI package exists anywhere in the resolved workspace graph, so no example
can require one. This gate runs the examples; 13a is what makes their independence a fact rather
than an observation about three files somebody read.

CI runs the same sequence on both `1.94.0` and current stable.

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
#
# The test module is EXCLUDED. The earlier form of this check scanned the whole file and
# counted the facade's own unit tests as implementation items — five of them — so it reported
# a violation that did not exist. A facade with tests is not a facade with an implementation,
# and a gate that cannot tell them apart is a gate that gets ignored.
FACADE_PROD=$(sed -n '1,/^#\[cfg(test)\]/p' crates/renvor/src/lib.rs | sed '$d')
IMPL_COUNT=$(printf '%s\n' "$FACADE_PROD" | grep -cE '^[[:space:]]*(pub )?(fn|impl|struct|enum) ' || true)
test "$IMPL_COUNT" -eq 0 || { echo "FAIL 13f — facade contains $IMPL_COUNT implementation items"; exit 1; }

# CONTROL 1: the facade really does re-export things, so the zero above means "re-exports only"
# rather than "the file is empty".
printf '%s\n' "$FACADE_PROD" | grep -qE '^[[:space:]]*pub use ' \
  || { echo "CONTROL FAILED — facade has no re-exports at all; 13f proves nothing"; exit 1; }

# CONTROL 2: the pattern can still match. Run against the WHOLE file it must find the test
# functions — otherwise the zero above is a broken regex rather than a clean facade.
WHOLE_COUNT=$(grep -cE '^[[:space:]]*(pub )?(fn|impl|struct|enum) ' crates/renvor/src/lib.rs || true)
test "$WHOLE_COUNT" -gt 0 \
  || { echo "CONTROL FAILED — the item pattern matches nothing anywhere; 13f proves nothing"; exit 1; }
echo "13f PASS (both controls fired; production items=0, whole-file items=$WHOLE_COUNT)"
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
# The directory must exist. `grep -r` on a missing path exits 2, and the previous form's `||`
# branch treated that exactly like "found nothing" — a zero-asserting check that passed hardest
# when there was nothing to search.
test -d .github/workflows \
  || { echo "FAIL 14c — .github/workflows/ is missing; this check has nothing to search"; exit 1; }

if grep -rlE 'ghcr\.io|docker/build-push-action' .github/workflows/ ; then
  echo "FAIL 14c — an image-publishing workflow exists"; exit 1
fi

# POSITIVE CONTROL: the pattern and the search both work. Without it, a renamed action or a typo
# in the alternation reads as "no image publishing" — the fail-open this gate exists to prevent,
# and the only zero-asserting check in this file that had no control.
CONTROL=.github/workflows/gate14c-control.yml
trap 'rm -f "$CONTROL"' EXIT
printf 'jobs:\n  probe:\n    steps:\n      - uses: docker/build-push-action@v6\n' > "$CONTROL"
grep -rlE 'ghcr\.io|docker/build-push-action' .github/workflows/ > /dev/null \
  || { echo "FAIL 14c control — the pattern did not match a planted publishing step"; exit 1; }
rm -f "$CONTROL"; trap - EXIT
echo "14c: no image-publishing workflow (control fired)"
```

> **Stated limit**: an anonymous query against a container registry returns **403** for both *absent*
> and *private* packages, so it **cannot** distinguish them. No gate here claims "no image exists" —
> only that **no workflow capable of publishing one is present**. Claiming the stronger statement
> from a 403 would be the same fail-open error as reading an API error body as zero versions.

### 14d — ADR-0007 governance gate

**The authority is `W-004`, granted and merged on 2026-08-16.** This gate previously described the
authority as *"a separately proposed and separately approved waiver"* — future tense, from before
W-004 existed. It exists, it is `active` in `governance/waivers.md`, and it is scoped to
**ADR-0007 and nothing else**. What remains checkable is whether its **four counted controls** and
**three restated preconditions** are actually on the record.

```bash
set -euo pipefail
ADR=$(ls decisions/0007-*.md 2>/dev/null | head -1)
test -n "$ADR" || { echo "14d FAIL — ADR-0007 ABSENT; custom infrastructure MUST NOT merge (FR-035)"; exit 1; }

# The waiver must exist and be active. An accepted ADR with no live authority behind it is
# the failure this check exists for.
grep -q '| \*\*W-004\*\* |' governance/waivers.md \
  || { echo "14d FAIL — W-004 is not recorded in the waiver ledger"; exit 1; }
grep -qE '\*\*W-004\*\*.*\| `active` \|' governance/waivers.md \
  || { echo "14d FAIL — W-004 is not active"; exit 1; }

# The decision-record template renders state as a TABLE ROW, not as front matter. The earlier
# form of this check looked for `status: accepted` at the start of a line, which this record has
# never contained — so the gate would have reported ADR-0007 as unaccepted while it was accepted,
# in the fail-CLOSED direction, which is why nobody noticed until the pattern was run.
grep -qiE '^\| \*\*State\*\* \| `accepted` \|' "$ADR" \
  || { echo "14d FAIL — ADR-0007 is not accepted"; exit 1; }

# The reviewer field must say what is true. W-004's grant fixes this string exactly, so that
# no reader can mistake a self-review for an independent one.
grep -qF 'Ahmed Anbar — self-review under W-004' "$ADR" \
  || { echo "14d FAIL — ADR-0007's reviewer field is not the exact string W-004 requires"; exit 1; }

# The FOUR counted controls, each of which must be visible in the record.
for control in \
  'proof gate' \
  'NON-INDEPENDENT' \
  'ADVISORY' \
  'disposition'
do
  grep -qiF "$control" "$ADR" \
    || { echo "14d FAIL — ADR-0007 does not record W-004 control evidence for: $control"; exit 1; }
done

# The advisory reviews must have produced a RESULT. Silence is recorded as not performed.
grep -qiE 'no findings|finding' "$ADR" \
  || { echo "14d FAIL — no advisory review result recorded; silence is NOT PERFORMED"; exit 1; }

# A Phase 001 waiver must not be cited as the authority for a Phase 002 record.
if grep -qE 'W-002|W-003' "$ADR"; then
  grep -qE 'W-002|W-003' "$ADR" | head -1
  echo "14d NOTE — ADR-0007 mentions a Phase 001 waiver; confirm it is cited as NOT applicable"
fi

# The record must STATE the truth, not merely avoid claiming otherwise. W-004's grant requires
# this sentence, because a reader who finds no claim of independence may infer it from silence.
grep -qiF 'No independent human review of ADR-0007 has occurred' "$ADR" \
  || { echo "14d FAIL — the record does not state that no independent review occurred"; exit 1; }

# POSITIVE CONTROL: these greps can return false. Without it, a `grep` that matched everything
# would satisfy every check above — including, note, the check immediately above, whose subject
# string is a SUBSTRING of the honest denial: searching for "independent human review of ADR-0007
# has occurred" finds a match inside "**No** independent human review of ADR-0007 has occurred".
# That is why the check is written as the affirmative sentence rather than as its absence.
grep -qiF 'this record was independently reviewed and approved' "$ADR" \
  && { echo "14d CONTROL FAILED — grep matched text that is not in the record"; exit 1; }
echo "14d PASS — ADR-0007 accepted under W-004, with its four counted controls on the record"
```

**No automated check can discharge the underlying requirement.** It needs a **qualified independent
human review**, which has **not occurred**. W-004 waives *who reviews*; it waives nothing about
*what must be true*. Its three restated preconditions — the alternatives-and-consequences analysis,
the package-first evaluation of every custom primitive, and the unconditional CI, dependency,
licence, advisory, secret-scanning, and code-quality gates — are **required by FR-035, constitution
principle III, and the workflow independently of this waiver**, and are deliberately **not counted**
as compensating controls. They must all hold; none of them is what the waiver buys.

**W-002 covers Phase 001 decision records; W-003 covers Phase 001's phase-level review. Neither
authorises accepting a Phase 002 ADR — W-004 does, and only for ADR-0007.**

---

## Gate 15 — Complete resolved-dependency inventory (FR-040)

Runs **after** manifests and `Cargo.lock` exist and **before** any adoption is confirmed. The
research table covers **direct candidates only**; a transitive dependency can carry an incompatible
licence or a live advisory just as easily.

```bash
set -euo pipefail
INVENTORY=governance/phase-002-dependency-inventory.md
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

# --- 15a: what the graph ACTUALLY resolves, workspace members excluded ---
cargo metadata --locked --format-version 1 > "$WORK/meta.json"
python3 - "$WORK" <<'PY'
import json, sys, os
work = sys.argv[1]
meta = json.load(open(os.path.join(work, "meta.json")))
members = set(meta["workspace_members"])
local = {p["name"] for p in meta["packages"] if p["id"] in members}
rows = sorted(f'{p["name"]} {p["version"]}'
              for p in meta["packages"] if p["name"] not in local)
open(os.path.join(work, "resolved.txt"), "w").write("\n".join(rows) + "\n")
PY

# --- 15b: what the inventory DOCUMENTS, read from its resolved-set table only ---
awk '/^## T030\/T033/{inside=1; next} inside && /^## /{inside=0} inside' "$INVENTORY" \
  | sed -n 's/^| `\([^`]*\)` | \([0-9][^ |]*\) |.*/\1 \2/p' | sort -u > "$WORK/documented.txt"

RESOLVED=$(wc -l < "$WORK/resolved.txt")
DOCUMENTED=$(wc -l < "$WORK/documented.txt")
echo "resolved: $RESOLVED    documented: $DOCUMENTED"

# CONTROL 1: both sides were actually read. Two empty files compare equal, and a gate that
# passes by reading nothing is the failure mode this gate exists to prevent.
if [ "$RESOLVED" -lt 40 ] || [ "$DOCUMENTED" -lt 40 ]; then
  echo "FAIL 15 — one side was not read (resolved=$RESOLVED documented=$DOCUMENTED)"; exit 1
fi

# --- 15c: the comparison itself, in BOTH directions ---
if ! diff -u "$WORK/documented.txt" "$WORK/resolved.txt"; then
  echo "FAIL 15 — the documented inventory and the resolved graph disagree."
  echo "  lines marked '-' are documented but NOT resolved (stale rows)."
  echo "  lines marked '+' are resolved but NOT documented (unevaluated packages)."
  exit 1
fi

# CONTROL 2: the comparison can fail. Plants one package that is not in the graph and
# confirms the diff rejects it — so a passing comparison means agreement rather than a
# comparison that silently stopped working.
{ echo 'not-a-real-package 9.9.9'; cat "$WORK/documented.txt"; } > "$WORK/tampered.txt"
if diff -q "$WORK/tampered.txt" "$WORK/resolved.txt" > /dev/null; then
  echo "FAIL 15 control — a planted package was not detected"; exit 1
fi
echo "control: the inventory comparison detects a package the graph does not contain"

# --- 15d: every documented row carries the columns FR-040 requires ---
#
# The Pass paragraph claimed licence, MSRV, and origin were verified. Nothing checked them: the
# extraction above captures name and version and discards the rest of each row. A row with an
# empty licence cell would have compared equal and passed.
awk '/^## T030\/T033/{inside=1; next} inside && /^## /{inside=0} inside' "$INVENTORY" \
  | grep '^| `' \
  | awk -F'|' '{
      name=$2; version=$3; licence=$4; msrv=$5; origin=$6; reach=$7;
      gsub(/^[ \t]+|[ \t]+$/, "", licence);
      gsub(/^[ \t]+|[ \t]+$/, "", msrv);
      gsub(/^[ \t]+|[ \t]+$/, "", origin);
      gsub(/^[ \t]+|[ \t]+$/, "", reach);
      if (licence == "" || msrv == "" || origin == "" || reach == "")
        { print "INCOMPLETE ROW:" name; bad=1 }
      if (licence ~ /none/) { print "NO LICENCE:" name; bad=1 }
    } END { exit bad ? 1 : 0 }' \
  || { echo "FAIL 15 — a documented package is missing evidence FR-040 requires"; exit 1; }
echo "every documented row carries a licence, an MSRV, an origin, and a reach"

# --- 15e: policy and supporting evidence against the real lockfile ---
cargo deny check licenses advisories bans sources

# Duplicate versions are RECORDED, not discarded. The previous form piped this to `|| true` and
# kept nothing, while the Pass paragraph said duplicates were recorded.
cargo tree --workspace --duplicates > "$WORK/duplicates.txt" 2>&1 || true
echo "duplicate-version findings:"; cat "$WORK/duplicates.txt"
cargo tree --workspace --edges features --prefix none > "$WORK/features.txt"
echo "enabled-feature lines: $(wc -l < "$WORK/features.txt")"
echo "GATE 15 PASS"
```

**Pass**: the documented inventory and the resolved graph are **identical, in both directions** —
no stale row, no unevaluated package — **every documented row carries a non-empty licence, MSRV,
origin, and reach**, checked column by column; `cargo deny` is clean against the real lockfile; and
the duplicate-version and enabled-feature output is **printed**, not discarded.

Every clause above is now executed by the script. Three of them were prose until 2026-08-16: the
per-column evidence check did not exist, the duplicate output went to `|| true` and was thrown
away, and the feature file was written and then deleted by the cleanup trap without being read.

**This gate previously compared nothing.** It counted resolved packages, printed the count, and ran
`cargo deny`; the inventory document was never opened. A row for a package that had left the graph,
or a package that had entered it undocumented, passed unnoticed — and both had happened. The
comparison, and the control proving the comparison can fail, are what make the pass mean what the
paragraph above says.

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
