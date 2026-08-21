## What this changes

<!-- Explain why, not what. The diff already shows what changed. -->

## Why

## Verification

- [ ] `cargo xtask verify` passes locally (exit 0, every contract-defined step)
- [ ] The change is focused on one thing
- [ ] Documentation is updated where the change makes it inaccurate

## If this touches a published contract

- [ ] `SUPPORT.md`, the MSRV, or a public API is affected — a decision record is included
- [ ] No claim is made that is not backed by a passing check

## If this adds a dependency

- [ ] Its licence is on the `deny.toml` allow-list
- [ ] The pull request says what it does and why a smaller option will not serve

## Notes for the reviewer

<!--
The project has a single maintainer, so pull requests merge without a second approving
review under waiver W-001 (governance/waivers.md). Every required check still applies,
and no account can bypass them.
-->
