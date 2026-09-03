# renvor-jobs

Durable background jobs for the [Renvor](https://github.com/renvor-rs/renvor) framework: the job-store port, bounded value types, an in-memory substitute, and a bounded worker with observable retries. The durable stores live in `renvor-sqlx` and `renvor-seaorm` behind their `jobs` features, so a MySQL application never resolves a PostgreSQL crate by choosing jobs.

**Prerelease. Nothing here is published and no API is stable.**

## Licence

`MIT OR Apache-2.0`.
