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

Every interface this crate exposes — the flag surface, the exit codes, the JSON envelope, and the
error-code registry — is a **contract** from the first release that ships it. Until then it may
change. The contracts are in
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

## Exit codes

`0` success · `1` **internal defect — report it** · `2` usage · `3` validation · `4` cancelled ·
`5` environment.

## What it will not do

- It will not modify a trust store or issue a certificate. `--local-https` records intent and
  nothing else.
- It will not leave a partially written destination. Generation stages, verifies, and then performs
  a single rename; any failure before that removes the staging directory.
- It will not reach the network for any local flow.
