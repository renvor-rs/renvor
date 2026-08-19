# Phase 003 — Quickstart and acceptance validation

**Feature**: [`spec.md`](spec.md) · **Plan**: [`plan.md`](plan.md) · **Created**: 2026-08-17

This is the runnable proof that Phase 003 does what its specification says. Every gate below maps to
a numbered success criterion, and **a gate that cannot run is a failure, not a skip** — the same rule
Phase 001 established at FR-023 and Phase 002 inherited.

## Prerequisites

```bash
rustup toolchain install 1.94.0 --component rustfmt clippy   # the declared MSRV
rustup toolchain install stable  --component rustfmt clippy
cargo --version && rustc --version
```

Everything below runs from the repository root and needs **no network access**. Gate 11 proves that
rather than asserting it.

## Build

```bash
cargo build -p renvor-cli --locked
RENVOR="$(pwd)/target/debug/renvor"
"$RENVOR" --version
```

**The binary is named `renvor`, not `renvor-cli`.** If `target/debug/renvor` does not exist, FR-001
has failed and nothing below is meaningful.

---

## Gate 0 — every gate below must actually run something

```bash
# A `cargo test` filter that matches no test runs zero tests and exits 0. Six of the gates below
# did exactly that until 2026-08-18, so six success criteria were "verified" by commands that
# verified nothing. Run this first.
grep -oE 'cargo test -p renvor-cli --(test|bins) [a-z_:]+( -- [a-z_]+)?' quickstart.md | sort -u |
while read -r cmd; do
  out=$(eval "$cmd" 2>&1 | grep -E '^test result' | head -1)
  case "$out" in
    # `. 0 passed;` and not `0 passed` — the loose pattern also matches "10 passed", which made
    # this gate cry wolf on its own suite. A gate that reports a failure that is not one gets
    # ignored, which is the same outcome as not having it.
    *". 0 passed;"*) echo "GATE RUNS NOTHING: $cmd -> $out"; exit 1 ;;
    *)            echo "ok: $cmd -> $out" ;;
  esac
done
```

**Expected**: no line beginning `GATE RUNS NOTHING`. A gate that reports `0 passed; 0 failed` has
told you nothing and exited `0` while doing so, which is worse than a gate that fails.

---

## Gate 1 — Cancellation leaves nothing (SC-001)

```bash
cargo test -p renvor-cli --test transaction -- cancelling_at_each_prompt
```

Drives the wizard to **each** prompt in turn and cancels there. After every one: the destination path
does not exist, and exit is `4`.

**Expected**: every prompt covered, not a sample. A test that cancels only at the first prompt proves
the first prompt.

---

## Gate 2 — Injected failure leaves nothing (SC-002)

```bash
cargo test -p renvor-cli --test transaction -- a_failure_at_any_mutating_step
cargo test -p renvor-cli --test transaction -- a_pre_existing_empty_destination_is_refused
```

Fails the render at each mutating step against a destination that does not exist, and separately
checks that a destination which **does** exist is refused before any of those steps can run — no
staging created, `details.injected` absent from the failure document.

**Expected**: 0 modifications. **Plus the control**: an un-injected run into the same fixture
succeeds, so the harness is not merely refusing everything.

---

## Gate 3 — Prompt and flag parity (SC-003)

```bash
cargo test -p renvor-cli --test parity
```

Generates once through a scripted terminal and once through flags with equivalent answers, into two
destinations, then compares `renvor.toml` byte-for-byte and the two file manifests entry-for-entry.

**Expected**: identical. This is a test of [`data-model.md`](data-model.md) invariant I-2 — one
configuration type — not of two interfaces agreeing by luck.

---

## Gate 4 — Unsupported input is refused before any write (SC-004)

```bash
cargo test -p renvor-cli --test generated -- a_reserved_flag_exits_three
"$RENVOR" new demo --transport rest --output json 2>/dev/null | jq -r '.error.code, .error.details.phase'
```

**Expected**: `reserved_for_later_phase`, and the phase named. Exit `3`, destination absent.

A reserved flag must **not** report "unknown flag" — see [`contracts/command-surface.md`](contracts/command-surface.md).

---

## Gate 5 — The generated project is real (SC-005)

```bash
cargo test -p renvor-cli --test generated -- every_generated_variant
```

Generates into a temporary location and then, inside it, runs `cargo fmt --check`, `cargo clippy`,
`cargo build`, `cargo test`, and starts the binary.

**Expected**: all pass. This runs as part of generation too (FR-030), so a skeleton that does not
build is a **generation failure**, not a discovery.

---

## Gate 6 — Dry run predicts reality exactly (SC-006)

```bash
D=$(mktemp -d)/demo
"$RENVOR" new "$D" --yes --dry-run --output json > /tmp/dry.json
test ! -e "$D" && echo "destination absent after dry run: ok"
"$RENVOR" new "$D" --yes --output json > /tmp/real.json
diff <(jq -S '.result.manifest' /tmp/dry.json) <(jq -S '.result.manifest' /tmp/real.json) && echo "manifests identical: ok"
```

**Expected**: destination absent after the dry run, and **0 differences** between the predicted and
actual manifests.

---

## Gate 7 — One JSON document, always (SC-007)

```bash
cargo test -p renvor-cli --test cli -- every_json_document
"$RENVOR" new / --output json 2>/dev/null | jq -e '.status == "failure" and .schemaVersion == 1' >/dev/null && echo "failure is still one valid document: ok"
```

**Expected**: exactly one document on `stdout` for success *and* failure, `schemaVersion` an integer,
and **0 human-readable text on `stdout`**.

The failure case is the one that matters: a command that fails by printing prose has broken the
contract precisely when a consumer most needs it.

---

## Gate 8 — No secret reaches any output (SC-008)

```bash
cargo test -p renvor-cli --test redaction
```

Drives a corpus of secret-shaped inputs through **all four** output paths — human output, JSON
output, the dry-run manifest, and error messages — and asserts none appears.

**Expected**: 0 occurrences. **Plus the control**: a non-secret marker with the same shape *does*
appear, proving the search would have found a leak.

---

## Gate 9 — Hostile paths and templates fail closed (SC-009)

```bash
cargo test -p renvor-cli --test hostile
```

The corpus: path traversal, absolute-path injection, a destination that is a symlink elsewhere,
platform-reserved device names, and a template whose output path escapes the staging root.

Since 2026-08-18 it also covers the tightened destination policy. Every existing destination is
refused — empty directory, non-empty directory, regular file, symbolic link including a dangling
one — and a destination whose state cannot be established fails closed rather than being treated as
absent:

```bash
cargo test -p renvor-cli --bins paths::tests
cargo test -p renvor-cli --bins generate::place::tests
```

**Expected**: all pass, including `no_production_path_removes_the_destination`, which reads
`place.rs` and fails if any removal names anything but this process's own staging directory.

**Expected**: 100% refused before any write, **and the positive control succeeds** — an ordinary
destination still generates. Without that control the whole gate is satisfied by refusing everything.

**No archive cases**, deliberately: FR-040 removes the archive path, and the absence is asserted
structurally instead. See [`contracts/template-contract.md`](contracts/template-contract.md).

---

## Gate 10 — The trust store is never touched (SC-010)

```bash
cargo test -p renvor-cli --test tls_consent
```

Snapshots the trust store, runs **every** command in the phase with local HTTPS both requested and
not, and re-snapshots.

**Expected**: byte-identical. This phase issues no certificate and modifies nothing, so the assertion
is **"0 modifications"** rather than the weaker "none without consent".

---

## Gate 11 — No network, proven (SC-011)

```bash
cargo test -p renvor-cli --test offline
```

Runs the local flows with networking unavailable.

**Expected**: every flow completes. Asserting "we do not use the network" in a comment is not this
gate; running without one is.

---

## Gate 12 — Not a terminal: no hang, no default (SC-012)

```bash
"$RENVOR" new demo < /dev/null; echo "exit=$?"
```

**Expected**: exits non-zero **promptly**, naming the missing flags. It must not block, and it must
not invent answers. Two independent mechanisms enforce this — see [`research.md`](research.md) D2 and
D12 — because the requirement has two distinct failure modes.

---

## Gate 13 — Every bound holds (SC-013)

```bash
cargo test -p renvor-cli --test bounds
```

`tests/bounds.rs` covers the one bound an operator can aim at — `manifest_bytes`, reachable because
`renvor check` takes a directory from the command line — with an over-bound **and** a boundary case.
**The four template bounds are unit tests in `src/generate/render.rs`, which this command does not
run**: every template is embedded, so no external input reaches the renderer. That file's header
tabulates where each bound's over-bound and boundary tests live. A fifth, `RECURSION_DEPTH`, has no
reachable trigger at all.

**Expected**: each bound is enforced, each violation reports `bound_exceeded` with the bound named,
and the destination is untouched in every case.

---

## Gate 14 — Both toolchains, and only claimed platforms (SC-014)

```bash
cargo +1.94.0 xtask verify
cargo +stable  xtask verify
```

**Expected**: exit 0 on both. A platform-specific behaviour that CI does not exercise is recorded as
unverified rather than claimed — the rule Phase 002 established for macOS and Windows.

---

## Gate 15 — The dependency inventory matches the lockfile (SC-015)

```bash
cargo tree -p renvor-cli --edges normal --prefix none | sort -u
```

Cross-check against the recorded inventory. **The lockfile is the authority, not
[`research.md`](research.md)** — research evaluates candidates, the lockfile records what was
actually resolved.

**Expected**: 0 omissions.

---

## Gate 16 — Generation is reproducible (SC-016)

```bash
cargo test -p renvor-cli --test generated -- generating_the_same_configuration_twice
```

Generates twice from the same generator version, template version, and configuration.

**Expected**: identical manifests.

---

## Gates that are NOT satisfied by this phase

Stated here so the quickstart is not mistaken for a completion certificate:

| Gate | Status |
|---|---|
| **D6 decision record** for hand-composed path containment over `cap-std` | **BLOCKING.** The path component must not merge before it is accepted |
| Certificate issuance | **Not delivered.** Narrowed by clarification |
| Archive support | **Not delivered.** Narrowed by clarification |
| The full fifteen-prompt wizard of `PLAN.md` §9.1 | **Not delivered.** Narrowed by clarification |
| Independent human requirements and security review | **Open.** Advisory reviews are not independent |
