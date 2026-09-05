# Provenance

Produced by **running the template-version-6 generator**, not written by hand.

    commit    6b9b70a  ("fix: warning-free feature-off port crates and the jobs migration count")
    command   renvor new v6-project --path <tmp>/v6-project --database postgres --orm seaorm \
                --container --container-cache valkey --example-domain --seed-data --yes
    built     from the Phase 011 worktree at that commit, before any template changed

Template version 6 is the last shape before Phase 011: `[project]` has no `auth` key, there is
no `[framework]`, `[auth]`, or `[capabilities]` table, and `[container]` records
`cache_wired_into_application = false` for a cache container the generated project does not use.
Every one of those absences is what `renvor check` must keep accepting after version 7 introduced
the keys — and the `false` is the sentence Phase 010's clarifications said "remains true and
remains asserted", which version 7 makes conditional rather than constant.

`Cargo.lock` is excluded, for the reason the version-3 fixture gives: cargo regenerates it and it
is not part of the manifest compatibility surface.

A hand-written imitation would prove only that the code agrees with the author's recollection of
version 6. See `tests/legacy_compatibility.rs`.
