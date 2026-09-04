# renvor-storage

Object storage capability for the [Renvor](https://github.com/renvor-rs/renvor) framework: a narrow put/get/head/delete port whose keys cannot traverse, an in-memory substitute, and a filesystem adapter rooted in a `cap-std` capability behind the off-by-default `filesystem` feature. **No S3 adapter ships in this phase**: every candidate failed the repository's licence or advisory gate when measured; see ADR-0035.

**Prerelease. Nothing here is published and no API is stable.**

## The filesystem adapter's on-disk layout

One file per object under `objects/`: the magic `RVO1`, a big-endian `u16` content-type length (0 when there is none), the content type (at most 255 bytes), then the bytes. One temporary-file-and-rename carries the bytes and the content type together, so `put` is last-writer-wins, whole, never interleaved (contract C-C5). A file that does not decode is reported as `Unavailable` with a closed reason and never its contents. **Pre-release: no compatibility with the earlier two-tree layout (`objects/` beside a `meta/` sidecar tree) is promised or provided.**

## Licence

`MIT OR Apache-2.0`.
