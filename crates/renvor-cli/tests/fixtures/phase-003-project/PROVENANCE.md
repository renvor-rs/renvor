# Provenance of this fixture

**This directory was not written by hand.** It is the verbatim output of the **Phase 003
generator**, so that the Phase 004 compatibility test is a test against a real artifact rather
than against a Phase 004 author's recollection of what Phase 003 produced.

| Field | Value |
|---|---|
| Produced by | `renvor new legacy-api --target api --yes` |
| Generator built from | `10da854736598d99218d1627c3ad79866a2f7f89` — the live `main` this branch forked from |
| Date captured | 2026-08-22 |
| `template_version` | `1` |
| `[project].transport` | **absent** — Phase 003 did not record a transport |

## Why it matters

Phase 004 added `transport` to the generated manifest. A first attempt made the field **required**,
and `renvor check` then rejected **every project Phase 003 had generated** — a framework
invalidating its own prior output. That defect was found by review and fixed, but its regression
test used a synthetic manifest, which cannot catch a divergence between what the test author
believed Phase 003 emitted and what it actually emitted.

This fixture closes that gap. `Cargo.lock` and `.gitignore` are deliberately **not** copied: a
lockfile would drift with the workspace and a `.gitignore` inside a fixture directory would hide
the fixture's own contents from Git.
