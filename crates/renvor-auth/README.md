# renvor-auth

Authentication, sessions, tokens, and authorization policies for the
[Renvor](https://github.com/renvor-rs/renvor) framework.

**Prerelease. Nothing here is published and no API is stable.**

This crate names no transport and no database driver. Repository *traits* live here; their
implementations live in `renvor-sqlx` and `renvor-seaorm`, and the HTTP surface lives in
`renvor-http`.

## Licence

`MIT OR Apache-2.0`.
