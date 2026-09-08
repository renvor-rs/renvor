# Provenance

A **template-version-7 starter** — the shape `renvor generate` meets on every project generated
before Phase 012, and the input `tests/legacy_compatibility.rs`'s FR-012-10 tests read.

## How it was made, and why not by running the version-7 generator

The three older fixtures beside this one were produced by building the generator at the commit
that wrote their version and running it. That was not done here, and the reason is stated rather
than glossed: template version 7 is `main` at `7281e4f`, whose generator is the one this branch is
editing — a second checkout and a second full build of the workspace, for a tree whose only
difference from what the current binary renders is this branch's own template diff.

So it was made the other way round, from that diff, and the recipe was **proved against a recorded
version-7 artifact** before it was applied here.

    generated   renvor new legacy-api --path <tmp>/legacy-api --framework-path <this worktree> \
                  --database postgres --orm sqlx --example-domain --yes
    with        the current binary (template version 8), 2026-09-07

Then, and only then, the inverse of this branch's template diff — four edits, each the deletion of
something version 8 added:

1. `rust-toolchain.toml`, added by `templates/rust_toolchain.toml.j2`, deleted.
2. the `rust-version` line under `[package]`, added by `templates/starter/Cargo.toml.j2`'s
   `{% if toolchain_declared %}` guard, deleted.
3. the whole `## Toolchain` section of `README.md`, added by `templates/starter/README.md.j2`'s
   guard, deleted up to the next heading.
4. `template_version = "8"` → `"7"`.

## The recipe is not a recollection: it was checked against a recorded artifact

`tests/snapshots/snapshots__manifest-v7-bare.snap` is a **tracked, unmodified** record of what the
version-7 generator produced for the `bare` skeleton — five paths and their SHA-256 digests.
Applying exactly the four edits above to a version-8 `bare` skeleton reproduced **all five digests
byte-for-byte** (`.gitignore` `a565fbc8…`, `Cargo.toml` `533fa285…`, `README.md` `9e3575a1…`,
`renvor.toml` `08595197…`, `src/main.rs` `8804e366…`). The recipe therefore reconstructs version 7
exactly, on the one tree shape where the repository holds version 7's own answer.

No such snapshot exists for a starter — a starter's manifest depends on the framework checkout it
is pointed at, which is why `starter_matrix.rs` proves starters instead — so the starter here rests
on the recipe rather than on a second independent digest.

## Two deliberate departures from the raw generator output

**`Cargo.lock` is excluded**, for the reason the version-3 fixture gives: cargo regenerates it, it
is resolved rather than rendered, and it is not part of the compatibility surface. It is excluded
from `.renvor/generated.toml` with it, so the record describes the tree as committed.

**The framework path is `/opt/renvor`**, the placeholder `crates/renvor-cli/src/commands/check.rs`
already uses for a fabricated starter manifest. The generator writes the absolute path of the
checkout it was pointed at into `[framework].path` and into every path dependency of `Cargo.toml`;
committing this machine's path would make the fixture unreadable everywhere else.
`renvor generate` resolves `[framework].path` and refuses a directory that does not exist, so the
tests substitute the real workspace root after copying — and the recorded digests of those two
files are stale from that moment, which changes nothing: neither file is in a
`generate resource` write set.

## The record

`.renvor/generated.toml` is a **legacy** record: no `record_version`, no `[toolchain]`, no
`[verified_with]` — the shape `record::read` classifies as "written by a generator older than
version 2". It was re-rendered over the tree as committed, in the version-7 layout, which
`git show 7281e4f:crates/renvor-cli/src/generate/record.rs` defines and which this branch's
`record::render` still emits unchanged for a record without `record_version`.
