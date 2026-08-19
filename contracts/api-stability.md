# Contract C-S1 — public API stability

**Status**: normative · **Applies to**: the `renvor` facade crate and the kernel surface it re-exports

This is the **single authoritative statement** of when Renvor's public surface stops being
unstable. It was previously stated three times inside one phase specification, and the verification
sequence checked that all three copies stayed byte-identical. Consolidating it here removes the
possibility of the drift that check existed to catch — there is now one copy, so two copies cannot
disagree.

## The surface is explicitly unstable

The kernel's public surface is **explicitly unstable**, and that instability is stated in the
surface's own published documentation rather than only in a specification. Breaking changes are
permitted without a compatibility procedure while the window is open.

**No semantic-versioning compatibility promise is made or implied for this surface while this
contract stands.**

## The window is closed by an event, not by a phase number

Closing it requires **both** of the following, with **no phase number forming part of either
condition**:

- **(a)** **the first real transport adapter has exercised the surface and its feedback has been applied**; and
- **(b)** an **accepted decision record that supersedes ADR-0002** closes the window explicitly.

<!-- The condition (a) line above is deliberately NOT wrapped. `cargo xtask verify` step 7 matches
     that sentence byte-for-byte, and a line break inside it makes the match fail — which it did,
     once, and the gate caught it. Do not reflow this file. -->

**Supersession, not amendment.** ADR-0002 is the accepted record governing facade stability.
Amending it would leave an accepted record still partly governing a guarantee it no longer fully
states, reproducing the two-records-disagreeing defect this contract exists to prevent. A
superseding record replaces the governing statement outright and leaves exactly one authority.

*(Roadmap rationale, not part of the condition: the current roadmap places the first real transport
after the interactive CLI, which is why the window is expected to outlive that work. An earlier
draft made the closure condition itself phase-numbered and ended it one phase before the transport
that was supposed to close it — self-contradictory, because it justified instability as lasting
until a real transport had exercised the surface while freezing that surface before any transport
existed. An event-named gate also survives roadmap renumbering; a phase-numbered one does not.)*

## Narrowness is part of the contract

The facade re-exports a **deliberately narrow** surface. It does **not** re-export an item merely
because that item is public in an implementation crate, so that later narrowing is possible without
a breaking change.

## How this contract is enforced

`cargo xtask verify` step 7 reads this file and fails the build if the closure clause names a phase
number, or if either condition has gone missing. The check is in
[`contracts/verification-sequence.md`](verification-sequence.md).
