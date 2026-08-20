# Deferred verification work

Verification checks that were attempted, found unfit to gate merges, and withdrawn — with the
source commit that holds the withdrawn implementation and the design intended to replace it.

A withdrawn check leaves a real gap. Recording the gap here is the point of this file: a
verification sequence that lists only what runs tells a reader what is checked but not what is
unchecked.

---

## DV-001 — repository documentation cross-references

| Field | Value |
|---|---|
| **State** | withdrawn 2026-08-20, replacement not yet designed in detail |
| **Withdrawn from** | `contracts/verification-sequence.md` (proposed step 11) and `cargo xtask verify` |
| **Reached `main`?** | **No.** It existed only on `chore/public-repository-surface` and was removed before that branch merged |
| **Source reference** | commit `a8b70009e0ed619746377ea9f57676f419108907`, path `scripts/check-doc-references.py` |
| **Decided by** | Ahmed Anbar, project maintainer. Not an independent review |

### What it was

A 443-line Python script validating the repository's own references against the **git index and
local git objects**, never the filesystem — because `git rm --cached` leaves a file on disk, so a
filesystem check is structurally incapable of detecting the breakage that untracking creates. It
reported three failure classes: broken relative Markdown links, unresolved `specs/`-shaped path
references in tracked text, and same-repository `blob/` URLs that were neither pinned to a full
commit SHA this clone holds nor pointing at a currently tracked path on the live branch. It
carried its own control suite — 13 negative, 9 positive, 1 invariant — run on every invocation.

It found real defects. An earlier filesystem-based version of the same check had reported
"0 broken" while eight links were in fact broken, four of them in the independent-review packet a
reviewer is meant to open first. Those eight are fixed, and the fixes are part of the merged
public-surface work; only the *gate* is withdrawn.

### Why it was withdrawn

**It parses Markdown with hand-written regular expressions.** Markdown is not a regular language.
The following cases were run against the implementation at `a8b7000` on 2026-08-20 and every one
is mishandled — each in the false-positive direction, meaning the gate would reject valid
documents:

| Input | Correct behaviour | Actual behaviour |
|---|---|---|
| A link inside a fenced code block | not a link | followed as a link |
| A link inside an indented code block | not a link | followed as a link |
| A link inside an inline code span | not a link | followed as a link |
| A link inside an HTML comment | not rendered | followed as a link |
| `\[escaped\](target)` | not a link | followed as a link |
| `[x](some(file).md)` — balanced parens are legal | target `some(file).md` | truncated to `some(file` |
| `[x](<README.md>)` — angle-bracket destination | resolves to a tracked file | reported **broken** |

None of these constructs happens to appear in the current tree, so the check passes today. It
passes by accident of corpus content, not by correctness — which is the same property the check
was written to detect in others.

**The defect rate did not fall across review rounds.** Four consecutive rounds of internal
advisory review of this branch returned 6, 5, 4, and 4 findings. Every finding was in this
checker or the contract text describing it; none was in the archival work the checker exists to
verify. Four rounds of fixes to one 443-line file, with the count flat, is evidence of a design
that is wrong rather than of a fixed number of remaining bugs.

**A merge-blocking gate must be more trustworthy than what it gates.** This one was not.

### Intended replacement

Not designed in detail, and deliberately not part of the branch that withdrew the original:

1. Parse Markdown with **`mdast-util-from-markdown`**, declared as a direct dependency in
   `docs/package.json`. Node and `npm ci` are already required by step 9, so this adds a
   dependency but no new toolchain.
2. Keep only a **small custom policy layer** over the resulting syntax tree: resolve targets
   against `git ls-files -z`, and validate `blob/<sha>/<path>` citations with `git cat-file -e`.
   That layer is the part with genuine project-specific value and no off-the-shelf equivalent.
3. **No hand-written Markdown grammar**, in any language.

Whether the result becomes a step of `cargo xtask verify` is a separate decision, to be made
against the same standard this check failed.

### Retention requirement

The withdrawn implementation is reachable only through commit
`a8b70009e0ed619746377ea9f57676f419108907`. That commit is on branch
`chore/public-repository-surface`, which was squash-merged; the individual commit is therefore
**not** in `main`'s history.

**Do not delete branch `chore/public-repository-surface`.** Deleting it makes the commit
unreachable and this reference dead. If the branch is ever deleted, the implementation must first
be preserved elsewhere and this record updated to point at the new location.

Permalink (valid while the branch exists):
<https://github.com/renvor-rs/renvor/blob/a8b70009e0ed619746377ea9f57676f419108907/scripts/check-doc-references.py>

### What this record does not claim

- It does **not** claim the withdrawn check was reviewed independently. It was reviewed by an
  automated reviewer and by the maintainer who wrote it. Neither is independent review under
  `GOVERNANCE.md`.
- It does **not** claim the reference gap is closed. It is open, named, and unguarded.
- It does **not** claim the seven mishandled constructs are the complete set. They are the seven
  that were tested.
