# renvor-storage

Object storage capability for the [Renvor](https://github.com/renvor-rs/renvor) framework: a narrow put/get/head/delete port whose keys cannot traverse, an in-memory substitute, and a filesystem adapter rooted in a `cap-std` capability behind the off-by-default `filesystem` feature. **No S3 adapter ships in this phase**: every candidate failed the repository's licence or advisory gate when measured; see ADR-0035.

**Prerelease. Nothing here is published and no API is stable.**

## Licence

`MIT OR Apache-2.0`.
