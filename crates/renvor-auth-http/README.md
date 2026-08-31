# renvor-auth-http

The authentication transport adapter for the [Renvor](https://renvor.dev) framework.

Joins `renvor-auth`'s transport-independent operations to `renvor-http`'s routing seam: the routes
for every authentication flow, the Problem Details mapping that refuses to leak, and the OpenAPI
security schemes that describe what the transport actually implements.

`renvor-auth` names no transport and `renvor-http` names no domain. This crate is the only place
they meet.

**Pre-release, unpublished, and explicitly unstable.**

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
