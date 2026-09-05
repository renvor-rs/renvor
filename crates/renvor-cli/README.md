# `renvor-cli`

The crate that builds the **`renvor`** executable.

**The package is `renvor-cli`; the executable is `renvor`.** Those are separate facts and both are
normative — ADR-0010 makes the installed executable's name a compatibility promise, and `cargo
xtask verify` asserts it with a control, because Cargo's default is to name the binary after the
package and that default is wrong here.

## Status

**Pre-release and unstable.** `publish = false`, deliberately and unlike every other member of this
workspace: Phase 003 ships no release, and a package marked publishable that nobody has rehearsed
publishing is a claim without evidence.

Every interface this crate exposes — the flag surface, the exit codes, the JSON envelope, the
error-code registry, and the terminal presentation — is a **contract** from the first release that
ships it. Until then it may change. The contracts are in
[`contracts/`](../../contracts/).

## Commands

| Command | Purpose |
|---|---|
| `renvor new [NAME]` | Create a project, transactionally |
| `renvor doctor` | Report environment readiness. Reports; never installs |
| `renvor check` | Validate a project without building it |
| `renvor dev` | Run the local development loop |
| `renvor docker up\|down\|status\|logs` | Container development controls |

Commands listed in `PLAN.md` §9.3 but not above are **absent, not stubbed**. A command that exits
zero without doing the work reports success for something that did not happen.

## Starters (Phase 011)

`renvor new` generates one of two shapes. Without `--framework-path` it generates the dependency-free
**skeleton** every earlier phase shipped. With `--framework-path <checkout>` it generates a
**framework-backed starter**: a real Renvor application whose `Cargo.toml` declares `path`
dependencies on exactly the crates the selection needs, whose `Cargo.lock` is seeded from the
checkout's pins so it resolves offline, and whose every provider, route, migration, and
configuration key is a file the user owns.

| Flag | Values | What it changes |
|---|---|---|
| `--auth` | `none`, `session` | the Phase 009 session starter: registration, login, the current user, verification and password reset over the `mail` capability, CSRF-protected logout, ownership on the example domain; its migration set, copied byte for byte |
| `--capabilities` | `none`, or a list of `cache`, `jobs`, `mail`, `storage`, `observability` | one module under `src/capabilities/`, one `config/<section>.toml`, one provider the kernel boots, and the routes that exercise it; an unselected capability appears nowhere |

Every cross-choice rule is refused before anything is written, naming the flag: `session` needs
a database and `mail`; `jobs` needs a database; any capability or `--auth session` needs
`--framework-path`; `--auth api` and `--auth full` do not exist, because no token-issuing route
does.

Generation verifies a starter as it verifies a skeleton — `cargo fmt --check`, `clippy -D
warnings`, `build`, `test` — in a **sealed environment** (what the toolchain needs; no `RENVOR_*`,
no credential from the shell), and then sends it `--renvor-dump-routes`, the request `renvor
routes` sends, which the starter answers from its route registry before Boot and without a
database. The placed project's `tests/starter.rs` is its live proof: set `RENVOR_DATABASE_URL`
and the capability secrets named in `.env.example`, and it migrates, seeds, starts, drives every
selected flow over loopback, exports to a loopback OTLP receiver, and stops cleanly on the
interrupt a terminal sends. The framework's gate runs that proof for every covering row
(`crates/renvor-cli/tests/starter_matrix.rs`).

## Exit codes

`0` success · `1` **internal defect — report it** · `2` usage · `3` validation · `4` cancelled ·
`5` environment.

## Human output

Styled when it is talking to a terminal and never otherwise. Five separate conditions each switch
styling off on their own — `--output json`, a stream that is not a terminal, `TERM=dumb`,
`--no-color`, and a non-empty `NO_COLOR` — and an explicit refusal beats any force-colour
environment variable. The decision is made **per stream**, so `renvor doctor > report.txt` writes a
plain file while your terminal is still attached to `stderr`.

Colour is never the only signal: every state also carries a word (`INFO`, `WARN`, `ERROR`, `DONE`,
`OK`, `TOO OLD`, `MISSING`, `ABSENT`). There are no emoji. Rows measure width in columns, so CJK
text aligns, and a row that cannot fit **stacks** rather than truncating a value.

The rules are [`contracts/terminal-presentation.md`](../../contracts/terminal-presentation.md).
**They govern presentation only** and change nothing about the flags, the exit codes, or the JSON.

## What it will not do

- It will not modify a trust store or issue a certificate. `--local-https` records intent and
  nothing else.
- It will not leave a partially written destination. Generation stages, verifies, and then performs
  a single rename; any failure before that removes the staging directory.
- It will not reach the network for any local flow.
