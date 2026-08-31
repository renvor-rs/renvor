# ADR-0030: Put the authentication HTTP wiring in its own adapter crate

| Field | Value |
|---|---|
| **ID** | 0030 |
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

`plan.md` §1 said: *"Transport wiring lives in `renvor-http`."* It also said, in the next paragraph,
that the placement was **a hypothesis** and that *"the crate DAG is gate-enforced, so getting this
wrong fails verification step 7 rather than failing silently — which is the desired property."*

The gate refuted it. `renvor-auth` depends on `renvor-config` for `Secret<T>`, and step 7's CLAIM 3
forbids `renvor-http` from resolving `renvor-config`:

```rust
// CLAIM 3 — the transport depends INWARD. It must not reach back up to the facade, the
// configuration crate, or the CLI.
for outward in ["renvor v", "renvor-config ", "renvor-cli "] {
```

So a normal dependency `renvor-http → renvor-auth` fails verification. FR-081 nevertheless requires
HTTP routes for every flow to exist.

## Decision

A new workspace member, **`renvor-auth-http`**, holding the routes, the Problem Details mapping, and
the OpenAPI security schemes. It depends on `renvor-auth`, `renvor-http`, `renvor-error`,
`renvor-openapi` and `renvor-core`; **nothing depends on it**.

It stands to the transport exactly as `renvor-sqlx` and `renvor-seaorm` stand to the persistence
ports: `renvor-auth` defines application operations, and an adapter joins them to one protocol.
Reaching the configuration layer is allowed here because this is not the transport — it is an
adapter *above* the transport.

A step 7 row is added asserting that `renvor-auth --all-features` resolves **no** HTTP crate, turning
the claim its manifest header has made since batch A into a gate.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **An optional `auth` feature on `renvor-http`** | Step 7 reads the *default-feature* tree, so this would pass — **by being invisible to the gate rather than by satisfying it**. That is evading a check. |
| **Logic in the `renvor` facade** | The facade is legally allowed to depend on both; it *is* the composition root. But its own header states *"Everything below the metadata constants is a `pub use`"*, and it contains no logic at all. Breaking a documented design rule to avoid breaking a gate is the same act one level over. |
| **Move `Secret<T>` to `renvor-core` so `renvor-auth` drops the config edge** | Directly contradicts a stated invariant. `crate_dag_holds`: *"`renvor-core` carries no parser, no derive macro, and **no secret type**. That absence is the whole reason `renvor-config` exists as a separate crate."* Making the architecture's own reason-for-existing untrue is not a smaller change than adding a crate. |
| **Ship the routes as test-only code** | FR-081 requires routes to exist for an author to use. Test code is not a deliverable. |

## Consequences

**A thirteenth publishable crate.** It joins `RELEASING.md`'s table and `release-dry-run.yml`'s
`CRATES` list in **topological** order — after `renvor-http` and `renvor-openapi`, before `renvor`.
`cargo publish --dry-run` computes its own order and would go green regardless, which is why the list
is pinned and gate-checked; that has bitten this branch once already.

**Two additions to `renvor-http`'s public surface**, both needed and both argued for in their own
documentation:

- `PresentedCredentials` — exactly two header values, `Cookie:` and `Authorization:`, and no
  `header(name)`. A handler could not otherwise read the credential it must validate. `Host`,
  `Origin`, `Forwarded` and `X-Forwarded-For` remain unreachable, so Phase 004's property is intact.
- `Route::dispatch` — the registry was transport-neutral in its *types* and unusable without the
  axum bridge. It runs the middleware chain; there is deliberately no accessor handing out a bare
  handler.

**The facade still does not expose authentication.** An author names `renvor-auth-http` directly, as
they already name `renvor-sqlx`. Consistent, and a recorded limitation.

**`plan.md` §1 is now wrong and is corrected there**, with a pointer to this record. A plan that
survives its own refutation misleads the next reader.

## Compliance

- **Constitution principle II** (dependencies point inward): `renvor-auth-http → {renvor-auth,
  renvor-http}`, and neither depends on it.
- **FR-001** (the transport depends inward only): unchanged — `renvor-http` gained no dependency.
- **FR-081, FR-082, FR-083**: the routes, the security schemes, and the test application all live
  here.
- **PLAN.md §1**: refuted and corrected, by the mechanism §1 nominated for refuting it.
