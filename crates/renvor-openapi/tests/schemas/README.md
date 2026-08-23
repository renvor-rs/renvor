# Vendored OpenAPI specification schemas

These are the **official**, unmodified machine-readable schemas published by the OpenAPI
Initiative. They are vendored rather than fetched so that verification runs offline and
deterministically — a gate that needs the network is a gate that fails when the network does, and
`contracts/verification-sequence.md` requires a check that cannot run to be a failure rather than a
skip.

| File | Source URL | Retrieved |
|---|---|---|
| `oas-3.2-schema-2025-11-23.json` | <https://spec.openapis.org/oas/3.2/schema/2025-11-23> | 2026-08-23 |
| `oas-3.1-schema-2025-11-23.json` | <https://spec.openapis.org/oas/3.1/schema/2025-11-23> | 2026-08-23 |

## Why these dates, and why they are pinned

**There is no `/latest` alias.** `https://spec.openapis.org/oas/3.2/schema/latest` returns **404**.
Every reference must name a date, which is inconvenient once and reproducible forever.

**These were re-pinned from `2025-09-17` on 2026-08-23.** That earlier artifact resolves and is
valid, but it is superseded: `2025-11-23` replaces `$defs.styles-for-form` with
`$defs.explode-for-form`, dropping `required: ["style"]` from its `if`, so the `explode: true`
default now also applies when `style` is omitted. That is an **annotation-default fix, not a
pass/fail change** — every verdict in this suite is identical under both — but pinning the earlier
one would freeze this gate on a schema with a known-fixed defaulting bug.

**The schema and the dialect carry different dates.** The schema is `2025-11-23`; the dialect and
meta artifacts are still `2025-09-17`
(`https://spec.openapis.org/oas/3.2/dialect/2025-09-17`). Do not assume one date across all four.

**Never vendor from the `v3.2-dev` branch.** Its files contain `WORK-IN-PROGRESS` placeholders
rather than real URIs.

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
