# ADR-0024: Argon2id parameters, NFC normalization, and where the blocklist stops

| Field | Value |
|---|---|
| **ID** | 0024 |
| **State** | `proposed` |
| **Reviewer** | *(none — not reviewed)* |
| **Review date** | *(not reviewed)* |
| **Superseded by** | *(not superseded)* |

> **`proposed`, and deliberately not `accepted`.** Phase 009 has no independent review and no
> authority has been given to accept this record. It is written now because the code in
> `renvor-auth` already depends on these choices, and a decision that lives only in a source comment
> is a decision nobody can disagree with.

## Context

PLAN.md §16.1 mandates Argon2id. PLAN.md §13.1 requires *"Argon2id password hashing with parameters
benchmarked and recorded for the deployment class"* and NIST-conformant password rules. Neither
document says which parameters, which normalization form, or what the blocklist contains.

Three choices had to be made before a password could be hashed. Each is recorded here with its
evidence, because each was reached by measurement or by quotation rather than by preference.

## Decision 1 — RFC 9106 §4's **second** recommended option is the default

`m = 2^16` (64 MiB), `t = 3`, `p = 4`, 128-bit salt, 256-bit tag.

**Not the first option**, which the RFC lists ahead of it. Measured on aarch64 macOS 26.3, release
build, by `renvor_auth::password::benchmark`:

| Option | Memory | Hash | Verify |
|---|---|---|---|
| **second — chosen** | 64 MiB | **71.9 ms** | 67.8 ms |
| first | 2 GiB | **1.50 s** | 861 ms |

Ten simultaneous logins under the first option want **20 GiB of RAM and 15 seconds**. That is a
decision a specific deployment may make about its own hardware; it is not one a framework may make
on behalf of every deployment. The RFC itself calls the second *"a uniformly safe option"* for the
memory-constrained case, and a general-purpose web framework is that case by default.

**The benchmark ships**, not only its result. The numbers above describe one machine, and a
deployment that has not run it on its own hardware has a citation rather than a measurement.

### Consequence

`Argon2idParameters::RFC_9106_FIRST` is provided and is **not** the default. An application that has
measured its memory can select it in one line.

## Decision 2 — NFC, applied before both length measurement and hashing

NIST SP 800-63B-4 §3.1.1.2:

> *"If Unicode characters are accepted in passwords, the verifier SHOULD apply the normalization
> process for stabilized strings using the Normalization Form Canonical Composition (NFC)…"*

**NFC and not NFKC.** NFKC additionally folds *compatibility* characters — `ﬁ` becomes `fi`, a
full-width digit becomes ASCII — which silently changes what a password-manager-generated secret
hashes to and shrinks an alphabet nobody agreed to shrink.

Normalization is applied at **both** ends, and that symmetry is the decision. Applying it only at
verification passes an obvious round-trip test while permanently stranding any user who *registers*
in decomposed form: the stored hash is of decomposed bytes, every later login normalises to composed,
and the account is unreachable behind a message that says "wrong password". **A mutation found this
gap in the first version of the test suite** and it is recorded in the Phase 009 evidence.

### Consequence: length is counted in code points, after NFC

NIST says *"a minimum of 15 characters"* and does not define "character". Renvor counts **Unicode
code points after NFC** and says so rather than implying the standard did. Counting UTF-8 bytes —
the default thing to write — admits `"éééééééé"`: eight characters, sixteen bytes, through a
fifteen-character floor.

## Decision 3 — the blocklist is a **port**, and this phase does not choose the corpus

`PasswordBlocklist` takes the **complete candidate** and returns a verdict. There is no method on it
that could accept a substring, so §3.1.1.2's *"The entire password SHALL be subject to comparison,
not substrings or words that might be contained therein"* is met by the shape rather than by
implementations remembering it.

**Which corpus ships — its size, its source, and its licence — is NOT decided here.** Phase 009
delivers the mechanism and an in-memory implementation. Shipping a multi-hundred-megabyte breach
corpus inside a framework crate is a packaging and licensing decision with consequences for
`cargo deny`, for crate size limits, and for every downstream build, and it deserves its own record.

**This is a stated gap, not an oversight.** A framework that shipped an empty blocklist while
claiming NIST conformance would be making a false claim; `StaticBlocklist::is_empty` exists so a
deployment can assert its list is populated, and the Phase 009 limitations record carries this as
open work with an owner.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| RFC 9106's first option as default | measured at 1.5 s and 2 GiB per hash — see above |
| bcrypt | truncates at 72 bytes, which §3.1.1.2 forbids outright |
| PBKDF2 | permitted by NIST, but not memory-hard; PLAN.md §16.1 already mandates Argon2id |
| NFKC | folds compatibility characters, changing what a generated secret hashes to |
| normalising only at verification | strands users who register in decomposed form — found by mutation |
| counting UTF-8 bytes | admits an 8-character password through a 15-character floor |
| a network blocklist lookup | puts somebody else's outage on the registration path; the port is offline by construction |
| shipping a breach corpus now | a packaging and licensing decision that deserves its own record |
