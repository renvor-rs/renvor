#!/usr/bin/env bash
# Toolchain identity guard for the verification jobs (WI-012-4, 2026-09-06).
#
# Why this exists: the repository's `rust-toolchain.toml` pins 1.94.0, and rustup lets a
# directory's toolchain file beat `rustup default`. The `stable` legs of `verify` and `platform`
# therefore compiled with 1.94.0 from the day the pin landed (98a4e2c, 2026-08-11) until this
# guard and the job-level `RUSTUP_TOOLCHAIN` selection were added — rustup even installed
# 1.94.0 on the stable runners in the middle of the gate. Job labels said `stable`; the
# compiler was the MSRV. This script makes the selection explicit and proves it, and fails
# closed on anything it cannot prove.
#
#   identity  <toolchain>            run BEFORE the suite, inside the checkout. Records what the
#                                    proxies resolve here (rustc and cargo: release, commit, host;
#                                    the active toolchain and why it is active) and fails unless
#                                    it is the toolchain the job was given, selected by the
#                                    environment, with implicit installation disabled and shown
#                                    to be disabled on this runner's rustup.
#   control   <toolchain> <control>  run BEFORE the suite, after the control toolchain — the OTHER
#                                    leg's — has been installed explicitly (U-10, approved
#                                    2026-09-07; brief §5.4). The selection controls of §5.4 need
#                                    two installed toolchains and prove a resolution by comparing
#                                    compiler identities, so this records `rustc +<toolchain> -vV`
#                                    and `rustc +<control> -vV` (release, commit, host) with the
#                                    control's cargo, rustfmt, and clippy, and fails unless the two
#                                    compilers differ in release AND commit — "a control run
#                                    against an identical identity proves nothing and fails the
#                                    leg" — unless the control has both components, and unless the
#                                    checkout still resolves <toolchain> after the second install
#                                    (whose `rustup default` names the control; the job-level
#                                    selection wins regardless, and this proves it). The control's
#                                    presence is read with `rustc +<control>` itself: under
#                                    RUSTUP_AUTO_INSTALL=0 an absent toolchain is refused by name,
#                                    so nothing is listed to find out.
#   artifacts <toolchain> <dir>...   run AFTER the suite. Reads cargo's own record of the compiler
#                                    it used, `<dir>/.rustc_info.json`, for every listed target
#                                    directory and fails unless each one names the same compiler.
#                                    With the census's target directory in the list this is the
#                                    runtime proof that generated projects — built by `cargo`
#                                    inside `renvor new`'s sealed environment — inherited the
#                                    selection too.
#
# Nothing here installs, downloads, or changes a default. It only reads.
set -euo pipefail

fail() { echo "::error::$*" >&2; exit 1; }
note() { echo "$*"; if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then echo "$*" >> "$GITHUB_STEP_SUMMARY"; fi; }

# `rustc -vV` lines for one toolchain, selected explicitly with `+<toolchain>`; that syntax beats
# every other selection, so this is the reference the checkout's own resolution is compared to.
reference() { rustc "+$1" -vV; }
field() { sed -n "s/^$2: //p" <<<"$1"; }

mode="${1:-}"; toolchain="${2:-}"
[ -n "$mode" ] && [ -n "$toolchain" ] || fail "usage: toolchain-identity.sh identity <toolchain> | control <toolchain> <control> | artifacts <toolchain> <target-dir>..."

case "$mode" in
identity)
  # 1. Record everything first, so a failure below is reported with the identities beside it.
  rustup_version="$(rustup --version 2>/dev/null | head -1 || echo 'rustup: not on PATH')"
  active="$(rustup show active-toolchain 2>&1 || true)"
  actual_rustc="$(rustc -vV)"
  actual_cargo="$(cargo -vV)"
  expected="$(reference "$toolchain")"
  note "### Toolchain identity (${RUNNER_OS:-local}, job toolchain \`$toolchain\`)"
  note "- rustup: \`$rustup_version\`"
  note "- active toolchain in the checkout: \`$active\`"
  note "- rustc resolved in the checkout: \`rustc $(field "$actual_rustc" release)\` host \`$(field "$actual_rustc" host)\` commit \`$(field "$actual_rustc" commit-hash)\`"
  note "- cargo resolved in the checkout: \`cargo $(field "$actual_cargo" release)\` commit \`$(field "$actual_cargo" commit-hash)\`"
  note "- reference \`rustc +$toolchain\`: \`rustc $(field "$expected" release)\` commit \`$(field "$expected" commit-hash)\`"
  note "- RUSTUP_TOOLCHAIN=\`${RUSTUP_TOOLCHAIN:-<unset>}\` RUSTUP_AUTO_INSTALL=\`${RUSTUP_AUTO_INSTALL:-<unset>}\`"

  # 2. The compiler the checkout resolves must be the compiler the job was given — release AND
  #    commit, not a label.
  [ "$(field "$actual_rustc" release)" = "$(field "$expected" release)" ] &&
  [ "$(field "$actual_rustc" commit-hash)" = "$(field "$expected" commit-hash)" ] ||
    fail "the checkout resolves rustc $(field "$actual_rustc" release) ($(field "$actual_rustc" commit-hash)) but the job's toolchain is '$toolchain' = rustc $(field "$expected" release) ($(field "$expected" commit-hash)); the toolchain file won over the job"
  [ "$(field "$actual_cargo" release)" = "$(field "$expected" release)" ] ||
    fail "cargo $(field "$actual_cargo" release) does not belong to rustc $(field "$expected" release)"

  # 3. It must be selected by the job, not by whichever file happens to be in the tree, and the
  #    job must forbid implicit installation.
  [ "${RUSTUP_TOOLCHAIN:-}" = "$toolchain" ] ||
    fail "RUSTUP_TOOLCHAIN is '${RUSTUP_TOOLCHAIN:-<unset>}'; the job must select '$toolchain' at job level so every step and every child cargo inherits it"
  case "$active" in *RUSTUP_TOOLCHAIN*) ;; *) fail "the active toolchain is not attributed to RUSTUP_TOOLCHAIN by rustup: '$active'";; esac
  [ "${RUSTUP_AUTO_INSTALL:-}" = "0" ] || fail "RUSTUP_AUTO_INSTALL is '${RUSTUP_AUTO_INSTALL:-<unset>}'; the job must set it to 0 so a missing toolchain is a named failure, never a download"

  # 3b. The components the verification sequence and the sealed project verification need must
  #     resolve under the same selection. With the selection explicit, the checkout's toolchain
  #     file no longer adds them implicitly (that was a download in the middle of the job), so the
  #     installing action must have been told, and this fails closed if it was not.
  for tool in "rustfmt --version" "cargo clippy --version"; do
    out="$($tool 2>&1)" || fail "\`$tool\` does not resolve under toolchain '$toolchain' (${out}); the installing action must add the component explicitly"
    note "- \`$tool\`: \`$out\`"
  done

  # 4. Prove on THIS runner's rustup — not from a version table — that an absent selected
  #    toolchain is refused by name without a download or a fall-back. The probe selects a
  #    channel that is not installed and points every download at a closed loopback port, so a
  #    rustup that ignored RUSTUP_AUTO_INSTALL would fail differently (a connection error), not
  #    silently succeed.
  absent=""
  for candidate in 1.85.0 1.86.0 1.87.0 1.88.0; do
    if ! rustup toolchain list | grep -q "^$candidate-"; then absent="$candidate"; break; fi
  done
  [ -n "$absent" ] || fail "every probe candidate toolchain is installed on this runner; the absent-toolchain probe cannot run"
  set +e
  probe="$(RUSTUP_TOOLCHAIN="$absent" RUSTUP_DIST_SERVER=http://127.0.0.1:9 RUSTUP_UPDATE_ROOT=http://127.0.0.1:9 rustc --version 2>&1)"
  probe_status=$?
  set -e
  note "- absent-toolchain probe (\`RUSTUP_TOOLCHAIN=$absent\`): exit $probe_status, \`$(echo "$probe" | head -1)\`"
  [ "$probe_status" -ne 0 ] || fail "selecting the absent toolchain '$absent' succeeded: something installed it or fell back (output: $probe)"
  case "$probe" in *"is not installed"*) ;; *) fail "the absent toolchain '$absent' was not refused by name; this runner's rustup ($rustup_version) may not honour RUSTUP_AUTO_INSTALL=0 (output: $probe)";; esac
  case "$probe" in *syncing*|*downloading*|*"could not download"*) fail "rustup attempted a download for the absent toolchain '$absent' (output: $probe)";; esac
  if rustup toolchain list | grep -q "^$absent-"; then fail "the probe installed '$absent'"; fi
  note "- result: the checkout compiles with \`rustc $(field "$expected" release) ($(field "$expected" commit-hash))\`, selected by the job; implicit installation is refused by name on this runner"
  ;;

control)
  control="${3:-}"
  [ -n "$control" ] || fail "control mode needs the control toolchain: control <toolchain> <control>"
  [ "$control" != "$toolchain" ] ||
    fail "the control toolchain is the leg's own ('$toolchain'); a control run against an identical identity proves nothing and fails the leg"

  # 1. Record everything first, so a failure below is reported with the identities beside it.
  #    The leg is read as in `identity`. The control is read with `rustc +<control>` ITSELF: under
  #    RUSTUP_AUTO_INSTALL=0 an absent toolchain is refused by name, so this is the presence check
  #    as well as the identity, and nothing is listed, installed, or defaulted to find out. Every
  #    query is captured with its failure named, since rustup's own text would otherwise be the
  #    only diagnostic.
  leg="$(reference "$toolchain")"
  control_rustc="$(rustc "+$control" -vV 2>&1)" ||
    fail "the control toolchain '$control' does not resolve ($(echo "$control_rustc" | head -1)); the installing action must provision it explicitly (U-10) — nothing here installs"
  control_cargo="$(cargo "+$control" -vV 2>&1)" ||
    fail "cargo does not resolve under the control toolchain '$control' ($(echo "$control_cargo" | head -1))"
  control_rustfmt="$(rustfmt "+$control" --version 2>&1)" ||
    fail "\`rustfmt +$control --version\` does not resolve ($(echo "$control_rustfmt" | head -1)); the installing action must add the component explicitly"
  control_clippy="$(cargo "+$control" clippy --version 2>&1)" ||
    fail "\`cargo +$control clippy --version\` does not resolve ($(echo "$control_clippy" | head -1)); the installing action must add the component explicitly"
  actual_rustc="$(rustc -vV)"
  note "### Control toolchain identity (${RUNNER_OS:-local}, leg \`$toolchain\`, control \`$control\`)"
  note "- leg \`rustc +$toolchain\`: \`rustc $(field "$leg" release)\` host \`$(field "$leg" host)\` commit \`$(field "$leg" commit-hash)\`"
  note "- control \`rustc +$control\`: \`rustc $(field "$control_rustc" release)\` host \`$(field "$control_rustc" host)\` commit \`$(field "$control_rustc" commit-hash)\`"
  note "- control \`cargo +$control\`: \`cargo $(field "$control_cargo" release)\` commit \`$(field "$control_cargo" commit-hash)\`"
  note "- control \`rustfmt +$control --version\`: \`$control_rustfmt\`"
  note "- control \`cargo +$control clippy --version\`: \`$control_clippy\`"
  note "- rustc resolved in the checkout after the second install: \`rustc $(field "$actual_rustc" release)\` commit \`$(field "$actual_rustc" commit-hash)\`"
  note "- RUSTUP_TOOLCHAIN=\`${RUSTUP_TOOLCHAIN:-<unset>}\` RENVOR_TEST_CONTROL_TOOLCHAIN=\`${RENVOR_TEST_CONTROL_TOOLCHAIN:-<unset>}\` RENVOR_TEST_REQUIRE_TOOLCHAINS=\`${RENVOR_TEST_REQUIRE_TOOLCHAINS:-<unset>}\`"

  # 2. Both identities must be readable before they can be compared: an empty field on one side
  #    would "differ" from anything, and that is not a proof.
  for side in leg control_rustc; do
    for key in release commit-hash; do
      [ -n "$(field "${!side}" "$key")" ] || fail "\`$key\` is unreadable from \`rustc +$([ "$side" = leg ] && echo "$toolchain" || echo "$control") -vV\`; the identities cannot be compared"
    done
  done

  # 3. The two compilers must differ in release AND commit. The controls compare identities to
  #    prove which toolchain a resolution picked; against the same compiler twice they would pass
  #    whatever the resolution did. In the brief's words (§5.4): a control run against an
  #    identical identity proves nothing and fails the leg.
  [ "$(field "$leg" release)" != "$(field "$control_rustc" release)" ] ||
    fail "the leg '$toolchain' and the control '$control' are both rustc $(field "$leg" release); a control run against an identical identity proves nothing and fails the leg"
  [ "$(field "$leg" commit-hash)" != "$(field "$control_rustc" commit-hash)" ] ||
    fail "the leg '$toolchain' and the control '$control' are the same compiler build ($(field "$leg" commit-hash)); a control run against an identical identity proves nothing and fails the leg"
  [ "$(field "$control_cargo" release)" = "$(field "$control_rustc" release)" ] ||
    fail "cargo $(field "$control_cargo" release) does not belong to the control's rustc $(field "$control_rustc" release)"

  # 4. The second install ran `rustup default <control>`. The job-level selection must still win
  #    in the checkout — release AND commit — and must still be the job's, not the default's.
  [ "$(field "$actual_rustc" release)" = "$(field "$leg" release)" ] &&
  [ "$(field "$actual_rustc" commit-hash)" = "$(field "$leg" commit-hash)" ] ||
    fail "after the control was installed the checkout resolves rustc $(field "$actual_rustc" release) ($(field "$actual_rustc" commit-hash)) but the job's toolchain is '$toolchain' = rustc $(field "$leg" release) ($(field "$leg" commit-hash)); the second install's default won over the job"
  [ "${RUSTUP_TOOLCHAIN:-}" = "$toolchain" ] ||
    fail "RUSTUP_TOOLCHAIN is '${RUSTUP_TOOLCHAIN:-<unset>}'; the job must still select '$toolchain' at job level after the control is installed"

  # 5. The suite reads the control's name from the environment. It must be the toolchain this
  #    step proved distinct, and the controls must be required rather than skippable, or the
  #    proof above is about a compiler the suite never uses.
  [ "${RENVOR_TEST_CONTROL_TOOLCHAIN:-}" = "$control" ] ||
    fail "RENVOR_TEST_CONTROL_TOOLCHAIN is '${RENVOR_TEST_CONTROL_TOOLCHAIN:-<unset>}' but the control proved distinct here is '$control'; the suite would run its controls against a toolchain this step did not prove"
  [ "${RENVOR_TEST_REQUIRE_TOOLCHAINS:-}" = "1" ] ||
    fail "RENVOR_TEST_REQUIRE_TOOLCHAINS is '${RENVOR_TEST_REQUIRE_TOOLCHAINS:-<unset>}'; in CI the selection controls must be required, never skipped"
  note "- result: the leg \`rustc $(field "$leg" release) ($(field "$leg" commit-hash))\` and the control \`rustc $(field "$control_rustc" release) ($(field "$control_rustc" commit-hash))\` are distinct compilers, both with rustfmt and clippy; the checkout still resolves the leg"
  ;;

artifacts)
  shift 2
  [ $# -ge 1 ] || fail "artifacts mode needs at least one target directory"
  expected="$(reference "$toolchain")"
  note "### Compiler recorded by cargo per target directory (job toolchain \`$toolchain\`)"
  for dir in "$@"; do
    info="$dir/.rustc_info.json"
    [ -f "$info" ] || fail "$info does not exist — nothing was built there, so nothing is proven for it"
    stdouts="$(jq -r '.outputs[] | .stdout' "$info")"
    releases="$(sed -n 's/^release: //p' <<<"$stdouts" | sort -u)"
    commits="$(sed -n 's/^commit-hash: //p' <<<"$stdouts" | sort -u)"
    hosts="$(sed -n 's/^host: //p' <<<"$stdouts" | sort -u)"
    note "- \`$dir\`: rustc \`$(echo "$releases" | tr '\n' ' ')\` commit \`$(echo "$commits" | tr '\n' ' ')\` host \`$(echo "$hosts" | tr '\n' ' ')\`"
    [ "$(echo "$releases" | wc -l | tr -d ' ')" = "1" ] && [ "$releases" = "$(field "$expected" release)" ] ||
      fail "$info records rustc [$(echo "$releases" | tr '\n' ' ')] but the job's toolchain is '$toolchain' = $(field "$expected" release)"
    [ "$commits" = "$(field "$expected" commit-hash)" ] ||
      fail "$info records commit [$(echo "$commits" | tr '\n' ' ')] but '$toolchain' is $(field "$expected" commit-hash)"
  done
  ;;
*) fail "unknown mode '$mode'";;
esac
