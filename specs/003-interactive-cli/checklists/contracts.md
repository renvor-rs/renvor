# Public Contract Requirements Checklist: Interactive CLI, templates, and local runtime

**Purpose**: Validate the **quality of the requirements** governing the command surface, the dual
prompt/flag interface, the machine-readable output contract, secret redaction, and the three recorded
scope narrowings. Unit tests for the English, not for the code.
**Created**: 2026-08-17
**Feature**: [spec.md](../spec.md) · **Depth**: formal release gate · **Audience**: reviewer

## Prompt and Flag Parity — One Model, Not Two That Agree

- [x] CHK031 Do the requirements force a **single** configuration value, or would two independent code paths that happen to agree satisfy them as written? [Clarity, Spec §FR-006]
- [x] CHK032 Is the parity criterion stated as **byte-identical** output rather than "equivalent" or "the same"? [Measurability, Spec §SC-003]
- [x] CHK033 Is validity required to be a property of **construction** — so that an unvalidated configuration cannot exist — rather than the result of remembering to call a validator? [Clarity, Data-model §I-1]
- [x] CHK034 Do the requirements state that validation completes before **any** filesystem write, and is the ordering guaranteed structurally rather than by discipline? [Ambiguity, Spec §FR-007]
- [x] CHK035 Is the impossibility of holding a secret stated as a property of the configuration **type**, rather than as a filter applied at write time? [Clarity, Data-model §I-3]

## Non-Terminal Behaviour

- [x] CHK036 Are **both** prohibited failure modes named separately — must not hang, and must not silently default? [Completeness, Spec §FR-010]
- [x] CHK037 Do the requirements explain why **two** independent mechanisms are required, or would a single check appear sufficient to a reader? [Clarity, Plan §Complexity Tracking]
- [x] CHK038 Is the required message content specified (naming the missing flags), rather than only the exit behaviour? [Completeness, Spec §FR-010]

## Reserved Later-Phase Flags — Three Behaviours, Clearly Distinguished

- [x] CHK039 Do the requirements distinguish unambiguously between **unknown flag**, **silently ignored**, and **reserved and rejected by name**? [Clarity, Spec §FR-005b]
- [x] CHK040 Is it stated that a reserved flag must **parse successfully** and then fail validation, rather than being rejected by the parser? [Clarity, Contract §C-1]
- [x] CHK041 Is the rejection required to name the **phase** that will support the choice, and is that carried into the error-detail contract? [Traceability, Contract §C-2]
- [x] CHK042 Do the requirements state the consequence being prevented — that a command line written now would otherwise change meaning later? [Clarity, Contract §C-1]
- [x] CHK043 Is it required that a reserved choice **never reaches the generated manifest**? [Consistency, Data-model §I-4 / §I-12]

## Exit Codes and Error Registry as Compatibility Contracts

- [x] CHK044 Is each exit code given a **distinct meaning** with an example, rather than a category label? [Clarity, Contract §C-1]
- [x] CHK045 Is the reservation of code `1` for unclassified failure stated **with its reason** — that an unclassified failure is a defect, not an outcome? [Clarity, Contract §C-1]
- [x] CHK046 Are stability obligations stated for the error-code registry — that a code must not be renamed or reused for a different meaning? [Completeness, Contract §C-2]
- [x] CHK047 Is it stated which parts of the JSON envelope are stable (`code`) and which are explicitly **not** (`message`)? [Clarity, Contract §C-2]
- [x] CHK048 Is `schemaVersion` required to be an **integer** rather than a string, and is the reason recorded? [Clarity, Contract §C-2]
- [x] CHK049 Is every error code mapped to exactly one exit code, with no code left unmapped? [Consistency, Contract §C-2]
- [x] CHK050 Is the requirement stated that failure must **also** produce one valid document, rather than only success? [Completeness, Spec §FR-022]

## Stream Discipline

- [x] CHK051 Is the `stdout`/`stderr` split stated as a requirement with **testable consequences** (a pipe to a JSON parser must work unfiltered)? [Measurability, Contract §C-1]
- [x] CHK052 Is behaviour on a prematurely closed `stdout` specified, rather than left to the edge-case list? [Coverage, Spec §Edge Cases]
- [x] CHK053 Are requirements for a non-terminal `stderr` (no progress rendering, no escape codes) stated? [Completeness, Contract §C-1]

## Secret Redaction — Written So It Cannot Pass Vacuously

- [x] CHK054 Are **all four** output paths enumerated — human, JSON, dry-run manifest, error messages — rather than covered by a general statement? [Completeness, Spec §FR-041]
- [x] CHK055 Is a **control** required, proving the search would have detected a leak had one been present? [Measurability, Spec §SC-008]
- [x] CHK056 Is it stated that the JSON path is **not exempt** because it is machine-readable? [Clarity, Contract §C-2]
- [x] CHK057 Do the requirements make clear that a self-redacting type does not by itself make a manifest safe? [Clarity, Research §D9]

## The Three Scope Narrowings — Traceable, Not Quietly Dropped

- [x] CHK058 Is "no certificate issuance" traceable from spec through plan to quickstart, and stated as a **narrowing of `PLAN.md`'s deliverable** rather than as a design choice? [Traceability, Spec §FR-036]
- [x] CHK059 Is "no archive support" stated with its consequence — that archive hardening leaves scope and is replaced by a **structural absence assertion**? [Clarity, Contract §C-4]
- [ ] CHK060 Is "a wizard shorter than `PLAN.md` §9.1's fifteen prompts" recorded where a reader of `PLAN.md` §20 would encounter it? [Traceability, Spec §FR-005a] **— PARTIAL.** Recorded in `spec.md` §Clarifications and in `governance/phase-003-evidence.md` §1–§2. **It is not recorded in `PLAN.md` §20 itself**, so a reader who opens `PLAN.md` and stops there still would not encounter it. Closing this means editing `PLAN.md`, which is out of this phase's scope.
- [x] CHK061 Do the requirements state that the trust-store assertion is **"0 modifications"**, which is stronger than "none without consent", and explain why the stronger form is the correct one here? [Clarity, Spec §SC-010]
- [x] CHK062 Is the consent boundary required to exist **now** even though the operation it gates does not, with the reason recorded? [Clarity, Spec §FR-037]
- [x] CHK063 Are the narrowings collected somewhere they will appear in the phase completion record, rather than only inside individual requirements? [Traceability, Checklists §requirements.md]

## Dependencies, Assumptions, and Governance

- [x] CHK064 Is the dependency inventory required to be produced from the **resolved lockfile** rather than from the research document? [Clarity, Spec §SC-015]
- [x] CHK065 Are maintenance concerns recorded with **exit conditions** rather than as observations? [Completeness, Research §D2]
- [x] CHK066 Is "no maintained package does this" recorded as a **finding** where it applies, rather than left as an unexplained custom implementation? [Traceability, Research §D5]
- [x] CHK067 Is "no package needed — it is in the standard library" recorded as a package-first **outcome**, so it is not later mistaken for an oversight? [Traceability, Research §D12]
- [x] CHK068 Do the requirements state that the independent-review gate remains **open**, and that advisory reviews are not independent? [Clarity, Spec §FR-046]
- [x] CHK069 Is it stated that the phase must not assume a waiver is available for that gate? [Clarity, Spec §FR-046]

## Notes

- Check items off as completed: `[x]`
- Every item asks whether something is **written correctly**, not whether the code works.
- **CHK031 is the single most important item in this file.** FR-006 is the requirement that makes
  constitution VII true; if it can be satisfied by two agreeing code paths, the phase's central
  guarantee is decorative.
- **CHK055 and CHK021** (in the sibling checklist) exist for the same reason: this project has
  repeatedly found its own gates fail-open, and a redaction test with no control passes on an empty
  corpus.
- **CHK058–CHK063** guard the failure mode where a phase narrows scope honestly at specification time
  and is then read, six months later, as having delivered `PLAN.md`'s original wording.
