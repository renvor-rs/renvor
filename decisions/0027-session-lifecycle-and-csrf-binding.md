# ADR-0027: Session lifecycle and CSRF binding

| Field | Value |
|---|---|
| **ID** | 0027 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-019. **Not independent** |
| **Review date** | 2026-08-31 |
| **Superseded by** | *(not superseded)* |

> **`accepted` under [W-019](../governance/waivers.md), and the review behind it was NOT
> independent.** No independent human review of this record has occurred, and none is claimed.
> The maintainer authored it and took every measurement it rests on; automated and maintainer
> reviews are **advisory**, never independent.
>
> W-019 covers **ADR-0024 through ADR-0030 as one coupled cluster** — each depends on a boundary
> another draws, so reviewing one alone would review a fragment — and it authorises nothing else.
> It does **not** close Phase 009; [W-020](../governance/waivers.md) is a separate exception on a
> separate axis.
>
> Accepted **2026-08-31** against head `0090c6784acdbfac863fc966e449245201a2b1fd`,
> tree `dd1b27e32f79a41efaaaa6abc2e4d477262f326d`. W-019 expires **2027-02-11**, or
> immediately when a qualified independent human reviewer becomes available — whichever
> is first.

## Context

A session must expire, be revocable, resist fixation and replay, and be bounded per subject; and
every state-changing browser request must be CSRF-protected without that protection being demanded
of API-token callers. All of it must behave identically on the four persistence rows.

NIST SP 800-63B-4 §2.2.3 (AAL2), quoted: *"A definite reauthentication overall timeout **SHALL** be
established, which **SHOULD** be no more than 24 hours at AAL2"*, and *"The inactivity timeout
**SHOULD** be no more than 1 hour."* §2.3.3 gives AAL3 as *"no more than 12 hours"* with a 15-minute
inactivity SHOULD.

> **Revision matters.** 12 hours / 30 minutes are **revision 3** numbers and are wrong for AAL2 in
> revision 4, which also renumbered the sections. 12 hours is AAL3.

## Decision

### Timeouts: the SHALL structurally, the SHOULD as a refusal

- *A timeout **SHALL** be established* — `SessionPolicy` has **no "unlimited" representation**, so a
  deployment cannot fail to establish one.
- *It **SHOULD** be no more than 24 h / 1 h* — `SessionPolicy::new` **refuses** longer values.
  **This enforces a SHOULD as a refusal, which is stricter than the document requires. It is a
  decision, not a citation.** A framework that permits a 30-day session by configuration will be
  deployed with one, and AAL1's 30-day allowance describes a lower assurance level than a
  password-plus-session design targets.

Refused rather than clamped: clamping lets an operator believe they configured something they did
not.

### Fixation is impossible, not prevented

No function turns a caller-supplied identifier into a live session. `begin` **generates** one; the
`Cookie` header it also accepts is used to *revoke* a pre-login session, never to adopt one.

### Liveness lives in `WHERE` clauses

`touch` is one conditional `UPDATE` carrying unrevoked, not-idle and not-too-old; `revoke` is one
conditional `UPDATE` on `revoked_at IS NULL`. `contracts/database-portability.md` §3 forbids
depending on the isolation level, and the engines differ, so a `SELECT`-then-`UPDATE` would give
different answers on different rows.

### Rotation revokes before it creates

If the create then fails, the subject is logged out — annoying and safe. The other order fails
**open**: a successful create followed by a failed revoke leaves two live sessions, one of them the
identifier the rotation existed to retire.

### Logout's guarantee is in the return type

`log_out` returns `Result<(SetCookie, LogoutOutcome), ServiceError>`. The expiry cookie exists only
on the `Ok` arm, which is only reached after the repository returns successfully from revoking. A
storage failure yields `Err` and **no `SetCookie` is constructed**, so a caller cannot tell a browser
it is signed out while a usable row remains.

### CSRF: OWASP's signed double-submit, bound to the session digest

Reproduced literally from the *Cross-Site Request Forgery Prevention Cheat Sheet*:

> `message = sessionID.length + "!" + sessionID + "!" + randomValue.length + "!" + randomValue.toHex()`
> `csrfToken = hmac.toHex() + "." + randomValue.toHex()`

Bound to the session's **digest**, not the raw identifier: a token is a value that gets copied into
forms, headers and logs, and the digest gives the same uniqueness with nothing to lose.

**FR-030 then costs nothing.** Rotating a session changes the digest, so every token bound to the
old one stops verifying — no second mechanism, and no window in which a rotated session still
accepts its predecessor's tokens.

**CSRF keys on what authenticated the request**, not on whether a cookie was present. A bearer token
is not attached by the browser automatically. **This is engineering judgement, not a citable
requirement**: OWASP's sheet does not discuss non-cookie authentication, and NIST §5.1 states a
blanket rule with no carve-out.

## Consequences

**Positive.** Expiry, revocation and the concurrency bound are all properties of statements the
database evaluates, proven on all four rows. CSRF rotation is structural.

**Negative, and stated rather than claimed away:**

- **The concurrency bound is enforced per login and is not atomic across concurrent logins.** Two
  simultaneous logins for one subject can momentarily leave `bound + 1` live sessions; the next
  login corrects it. Making it atomic would mean serialising every login for a subject.
- **`touch` writes on every authenticated request.** The inactivity window requires it. That is
  write amplification on a read path, and it is the price of an idle timeout.
- **`Origin`/`Referer` is not validated here.** OWASP does **not** claim signing removes the need
  for it — it recommends origin verification as a *separate* defence-in-depth layer. What is claimed
  is narrower: the property here does not *depend* on an `Origin` check. The check belongs in the
  transport adapter, the only layer that sees the header.
- **Only the MAC comparison is constant-time.** The hex decode, length checks and the split on `.`
  are not, and nothing here describes a whole request as constant-time.

## Alternatives rejected

- **Refusing a new session at the bound instead of evicting.** Locks a subject out of the device in
  front of them because of devices they no longer have; a control users route around is not a
  control.
- **Evicting by creation order.** Would evict the session the subject is using right now. The port's
  `live_for` contract is therefore *least recently seen first*, asserted on all four rows.
- **Per-request single-use CSRF tokens.** Breaks parallel requests and the back button. What must
  never be replayable is a token across *sessions*, which the binding already prevents.
- **The `csrf` / `axum_csrf` crates.** `axum_csrf` pulls `axum`, disqualifying it. Neither declares
  a `rust-version`, so 1.94.0 compatibility is unconfirmed. Neither can bind to our session type,
  which is the security property.
- **Binary length prefixes instead of OWASP's `!` delimiters.** Equivalent in strength and *not what
  a reviewer can check against the source*.
