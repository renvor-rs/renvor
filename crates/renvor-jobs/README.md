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

## Boot and Stop, bounded

`JobsWorkerProvider` proves the configured store answers before its loop exists: one bounded read
of the queue's depth at Boot, and a store that refuses or does not answer fails Boot with a closed
category rather than polling for ever. At Stop, a job still running at the grace has its
handler's own task aborted and joined before its lease is released — never the wrapper task
around it, whose abort would leave the handler running with no owner — and a handler that
cannot be dropped keeps its lease to expire rather than having it released under it. The
releases run concurrently under one bound; a release the store refuses or does not answer, and
a lease withheld this way, are counted in the `WorkerReport` and reported by Stop as
`LeasesNotReleased`, never swallowed. The `[jobs]` section
in `config` carries every bound with its default and hard cap and is refused at Validate by key,
constraint, and layer.
