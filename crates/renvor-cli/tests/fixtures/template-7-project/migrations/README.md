# Migrations for legacy-api

Applied **on Boot** by the database provider, in version order, each checksummed and recorded in
`_sqlx_migrations`; an edited migration fails closed rather than diverging silently. Every file
here is `<version>_<name>.up.sql` with its `.down.sql` beside it.

What this directory holds is exactly what the selection needs:
- `0001_item` — the example domain's table.

Add a migration as a new pair with a higher version. `renvor generate migration <name>` writes
the pair for you.
