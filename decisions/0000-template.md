# ADR-NNNN: [Imperative, specific title]

<!--
  Template for Renvor architecture decision records.
  Field set is defined by [data-model.md §Decision Record](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/data-model.md).

  Copy to decisions/NNNN-kebab-case-title.md. Numbers are four digits, monotonic,
  and never reused — a rejected or superseded record keeps its number forever.
-->

| Field | Value |
|---|---|
| **ID** | NNNN |
| **State** | `proposed` \| `accepted` \| `rejected` \| `superseded` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | *(required to enter `superseded`)* |

> **A record MUST NOT be marked `accepted` without a recorded independent review**
> (spec FR-013). Who qualifies as an independent reviewer is established in
> `GOVERNANCE.md`. Where no independent reviewer exists, acceptance requires a
> waiver recorded in `governance/waivers.md` with an absolute expiry date —
> the gap is never left unrecorded.

## Context

<!-- The forces that make a decision necessary. What is true today, what pressure
     exists, what constraints apply. Not the decision itself. -->

## Decision

<!-- What was chosen, stated unambiguously and in the present tense.
     A reader should be able to act on this section alone. -->

## Alternatives considered

<!-- REQUIRED. Each alternative with the reason it was rejected.
     A decision with no rejected alternatives was not a decision. -->

| Alternative | Rejected because |
|---|---|
|  |  |

## Consequences

<!-- Including the costs accepted, not only the benefits. State what becomes
     harder, what is now locked in, and what would have to change to reverse this. -->

## Compliance

<!-- Which constitution principles and PLAN.md sections this decision touches,
     and how it satisfies them. -->
