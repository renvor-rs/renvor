# ADR-0026: The session cookie boundary, and the two prefix rules that are not one rule

| Field | Value |
|---|---|
| **ID** | 0026 |
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

Phase 009 needs a browser session cookie. `draft-ietf-httpbis-rfc6265bis-22` defines the `__Host-`
prefix — **an Internet-Draft in the RFC Editor queue, not a published RFC**; RFC 6265 defines no
prefix and no `SameSite` at all, which is why every citation below names the draft by revision.

The draft states the prefix rule **twice, to two different audiences**, and they do not agree:

| Section | Audience | Rule |
|---|---|---|
| §4.1.3.2 | **servers** | *"begins with a **case-sensitive match** for the string `__Host-`"* |
| §5.4 | **user agents** | *"UAs MUST match the prefix string **case-insensitively**"* |

§5.4 supplies its own rationale: *"some servers will process cookies case-insensitively, resulting
in them unintentionally miscapitalizing and accepting miscapitalized prefixes."*

Reading §5.4 as licence for a server to accept `__host-rv_session` would implement the exact defect
the sentence was written to describe.

## Decision

**Each comparison does one job, and only one.**

| Question | Comparison | Outcome it can produce |
|---|---|---|
| is this cookie *my session*? | **case-sensitive** equality (§4.1.3.2) | a session, or nothing |
| is this cookie *impersonating* it? | **case-insensitive** equality (§5.4) | **an error, only ever** |

There is no branch on which a case-folded name yields a session. The case-insensitive comparison
appears solely on a path that returns `CookieRejection::PrefixImpersonation`.

The rejection is narrow: it fires only for a name that case-insensitively equals
`__Host-rv_session` and does not equal it exactly. Other `__Host-` cookies are untouched, and a
malformed *unrelated* pair does not lose the session — closing on everything would let any script on
the origin end a session by writing one junk cookie.

Four further decisions come with it:

1. **Every attribute is emitted, never defaulted.** bis §5.7 step 21.3 requires `Path` be present in
   the attribute list; a cookie that merely defaults to `/` is not `__Host-`.
2. **`SameSite` has no `None`.** `SameSiteChoice` offers `Lax` and `Strict`, so a deployment cannot
   be configured into `SameSite=None` on a session cookie. It is defence in depth, not the defence.
3. **The expiry cookie is built here, not by the crate.** `cookie`'s own removal cookie drops
   `HttpOnly` and `SameSite` and derives `Expires` from `OffsetDateTime::now_utc()` — a wall-clock
   read inside a crate whose entire expiry story is an injected clock.
4. **`SetCookie` keeps the value and the attributes in separate fields**, so the audit-safe
   `attributes()` reads a field that has never held a session identifier.

## Consequences

**Positive.** The server behaviour is safe under *either* user-agent rule, since exact matching is
the stricter of the two. The impersonation rejection cannot be turned into a denial of service:
bis §5.7 step 21 has the user agent apply the `__Host-` criteria case-insensitively, so a conforming
browser refuses to store a `__host-`-prefixed cookie carrying a `Domain` attribute — which is what a
sibling subdomain would have to send. **The UA's case-insensitive rule is precisely what makes the
server's case-insensitive rejection safe.**

**Negative.** A non-conforming user agent that both stores a miscapitalised prefixed cookie *and*
sends it alongside the real one will see its requests refused. Fail-closed, and stated.

**Cost.** One new package (`cookie 0.18.2`, default features off); `time` and `percent-encoding`
were already in the lock.

## Alternatives rejected

- **Case-insensitive acceptance**, as a literal reading of §5.4 suggests. This is the vulnerability
  §5.4 describes.
- **Case-sensitive matching only, with no impersonation check.** Safe against the primary attack,
  but silently discards an unambiguous attack signal arriving on the same request.
- **Rejecting the whole header on any malformed pair.** Fail-closed to the point of being a
  self-inflicted denial of service.
- **`tower-cookies` / `axum-extra`.** Both pull `axum`; `renvor-auth` resolves no transport, and
  `xtask`'s isolation gate fails the build if it did.
