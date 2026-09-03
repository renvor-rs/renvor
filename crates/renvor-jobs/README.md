# renvor-jobs

Durable background jobs for the [Renvor](https://github.com/renvor-rs/renvor) framework: the job-store port, bounded value types, an in-memory substitute, and a bounded worker with observable retries. The durable stores live in `renvor-sqlx` and `renvor-seaorm` behind their `jobs` features, so a MySQL application never resolves a PostgreSQL crate by choosing jobs.

**Prerelease. Nothing here is published and no API is stable.**

## The `rv_job` table

The durable stores live in `renvor-sqlx` and `renvor-seaorm` behind their `jobs` features; this
crate ships the schema they share, as one SQLx-format migration set per engine under
`migrations/postgres` and `migrations/mysql` (four `up`/`down` pairs, one statement per file,
versions `20260904000001`–`20260904000004`).

An application has **one** migration set and one ledger. Copy the files for your engine into your
own migration directory beside your other migrations; do not point a second `Migrations::load` at
this directory, because SQLx refuses a set that lacks versions the ledger already holds. The
versions are chosen not to collide with `renvor-auth`'s (`20260901…`).

## Licence

`MIT OR Apache-2.0`.
