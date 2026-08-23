# Vendored OpenAPI specification schemas

These are the **official**, unmodified machine-readable schemas published by the OpenAPI
Initiative. They are vendored rather than fetched so that verification runs offline and
deterministically — a gate that needs the network is a gate that fails when the network does, and
`contracts/verification-sequence.md` requires a check that cannot run to be a failure rather than a
skip.

| File | Source URL | Retrieved |
|---|---|---|
| `oas-3.2-schema-2025-09-17.json` | <https://spec.openapis.org/oas/3.2/schema/2025-09-17> | 2026-08-23 |
| `oas-3.1-schema-2022-10-07.json` | <https://spec.openapis.org/oas/3.1/schema/2022-10-07> | 2026-08-23 |

## Why the 3.1 schema is here too

The 3.2 schema alone cannot prove a document is *genuinely* 3.2 rather than 3.1 output with the
version string changed — a relabelled 3.1 document validates against the 3.2 schema perfectly well,
because 3.2 is largely backwards compatible.

The 3.1 schema is what discriminates. Both schemas use `unevaluatedProperties: false` throughout,
so a document carrying a 3.2-only member is **structurally** rejected by 3.1 — and the test
neutralises 3.1's `openapi` version pattern first, so the rejection cannot be attributed to the
version string.

## These files are not edited

They are upstream artifacts. A local edit would make the gate judge Renvor's opinion of the
standard rather than the standard.
