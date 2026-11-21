#!/usr/bin/env bash
# The one place that lists the checks of this repository (REAS-DOC-006).
# CI and the Stop hook call the groups of this script; CLAUDE.md, the README and the runbook
# point here. A check that is added to a group runs in each of those places.
#
#   scripts/check.sh rust       toolchain, format, compile, lint, the planted lint violations, unit tests,
#                               doc tests, documentation build
#   scripts/check.sh planted    each lint of Cargo.toml fires on examples/lint_canary.rs at its level
#   scripts/check.sh toolchain  rust-toolchain.toml, rustc and (in CI) the installed toolchain agree
#   scripts/check.sh compile    compile only; what the Stop hook runs after a change to the code
#   scripts/check.sh database   every test target, with the integration tests; needs DATABASE_URL
#   scripts/check.sh sqlx       migrations on the database of DATABASE_URL, and the checked-query
#                               metadata; needs sqlx-cli. CI runs it on an empty database
#   scripts/check.sh deny       cargo deny: advisories, bans, licenses, sources
#   scripts/check.sh audit      cargo audit against the RustSec database
#   scripts/check.sh docs       documentation checker and its tests
#   scripts/check.sh privacy    privacy canary: shapes, and your terms when the terms file exists
#   scripts/check.sh hooks      the test of the guard hook and the test of this script
#   scripts/check.sh lists      the workflows and the Stop hook call this script, and the commands
#                               that this script runs are in no line of theirs
#   scripts/check.sh all        every group above except sqlx; database only with DATABASE_URL
#
# Each command is printed before it runs, so a report can name it. The last line of `all` names
# what it did not run. Two jobs of CI are actions and have no command here: `secrets`
# (gitleaks) and `dependency-review`. The `deny` job runs `cargo deny check` through a pinned
# action, which brings its own version of the tool.
#
# `lists` is a gate against drift: it finds the commands of this script when somebody writes
# them into a workflow or the hook. It does not find a tool that this script does not know, and
# it is no control against a change that is meant to switch a check off. That control is the
# review of the pull request.
set -euo pipefail
cd "$(dirname "$0")/.."

GROUPS_IN_ORDER=(rust planted toolchain compile database sqlx deny audit docs privacy hooks lists all)
# What the workflow of pull requests must call, the daily workflow, and the Stop hook.
CI_GROUPS=(toolchain rust sqlx database audit docs privacy hooks lists)
SCAN_GROUPS=(audit)
HOOK_GROUPS=(compile docs privacy hooks lists)
# A line of a workflow or of the Stop hook that holds one of these runs a check of this script
# without this script.
OWN_CHECK='cargo (fmt|check|clippy|test|doc|deny|audit|sqlx)|rustc --version|python3? +scripts/check_docs|python3? +-m +unittest|scripts/pii_canary\.sh|bash +\.claude/hooks/guard-bash|bash +scripts/check\.test'
# In a workflow, each of these keys can make a job pass without its check. A job that needs a
# job that failed or was skipped is skipped itself.
RISKY_KEY='^[0-9]+:[[:space:]]*(-[[:space:]]+)?(if|needs|continue-on-error|shell):'
# One `if` is known: the dependency review runs on a pull request only.
KNOWN_IF="^[0-9]+:    if: github\.event_name == 'pull_request'\$"
NOT_RUN=()
# The lint levels of Cargo.toml, as Clippy reports them on the planted violations: `deny`
# gives an error and `warn` a warning. `forbid` keeps its own word here, because a forbidden
# lint lowered to `deny` could be expected away in the code. A lint that is added to the
# manifest is added here and gets its violation in examples/lint_canary.rs, in the same
# change. The list does not get shorter: LINT_FLOOR is its length on 2026-11-21.
LINT_FLOOR=14
FLAG_DECLARATION='unexpected_cfgs = { level = "warn", check-cfg = ["cfg(lint_canary)"] }'
LINT_LEVELS=(
  unsafe_code=forbid
  clippy::wildcard_enum_match_arm=error
  clippy::same_name_method=error
  clippy::needless_pass_by_value=warning
  clippy::redundant_clone=warning
  clippy::unwrap_used=error
  clippy::expect_used=warning
  clippy::allow_attributes=error
  clippy::allow_attributes_without_reason=error
  clippy::undocumented_unsafe_blocks=error
  clippy::multiple_unsafe_ops_per_block=error
  clippy::arithmetic_side_effects=error
  clippy::indexing_slicing=error
  clippy::string_slice=error
)

# Prints the command as a shell reads it, so that a quoted argument stays one argument.
run() {
  printf '\n$'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

needs_database() {
  if [ -z "${DATABASE_URL:-}" ]; then
    echo "scripts/check.sh $1 needs DATABASE_URL; see docs/runbooks/develop.md" >&2
    exit 2
  fi
}

# The toolchain file is what rustup follows in the checkout. In CI, INSTALLED is the name that
# the install step reports: a workflow that installs another version fails here (REAS-DATA-004).
toolchain() {
  local pinned active
  pinned=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
  active=$(rustc --version | cut -d' ' -f2)
  printf '\nrust-toolchain.toml: %s; rustc in the checkout: %s; installed by the workflow: %s\n' \
    "$pinned" "$active" "${INSTALLED-not given to this step}"
  if [ -z "$pinned" ] || [ "$active" != "$pinned" ]; then
    echo "FAIL: rust-toolchain.toml and rustc do not name the same toolchain" >&2
    return 1
  fi
  if [ -n "${INSTALLED+set}" ]; then
    case "$INSTALLED" in
      "$pinned" | "$pinned"-*) ;;
      *)
        echo "FAIL: the workflow installed another toolchain than rust-toolchain.toml names" >&2
        return 1
        ;;
    esac
  fi
}

compile() { run cargo check --workspace --all-targets --locked; }

rust() {
  toolchain
  run cargo fmt --all -- --check
  compile
  run cargo clippy --workspace --all-targets --locked -- -D warnings
  planted
  run cargo test --workspace --locked --lib --bins
  run cargo test --workspace --locked --doc
  run env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --locked --no-deps --document-private-items
}

# The lint table as the manifest declares it, one `name=level` in a line.
manifest_lints() {
  sed -n '/^\[lints\.rust\]/,/^\[/{s/^\([a-z0-9_]*\) = "\(forbid\|deny\|warn\)".*/\1=\2/p}' Cargo.toml
  sed -n '/^\[lints\.clippy\]/,/^\[/{s/^\([a-z0-9_]*\) = "\(forbid\|deny\|warn\)".*/clippy::\1=\2/p}' Cargo.toml
}

# What the manifest and clippy.toml hold beside the plain lines that manifest_lints reads.
# A lint in a form that the function above does not read, another lint table, a change to the
# declaration of the flag, or a setting of Clippy that narrows a lint: each is printed here.
lint_table_surprises() {
  grep -nE '^\[(workspace\.)?lints' Cargo.toml | grep -vE '^[0-9]+:\[lints\.(rust|clippy)\]$' || true
  awk '/^\[/ { inside = ($0 ~ /^\[(workspace\.)?lints/) ; next } inside && !/^[[:space:]]*(#|$)/ { print FILENAME ":" FNR ":" $0 }' Cargo.toml \
    | grep -vE '^Cargo\.toml:[0-9]+:[a-z0-9_]+ = "(forbid|deny|warn)"[[:space:]]*(#.*)?$' \
    | grep -vF "$FLAG_DECLARATION" || true
  grep -nvE '^[[:space:]]*(#|$)' clippy.toml \
    | grep -vxE '[0-9]+:(allow-unwrap-in-tests|allow-expect-in-tests|allow-indexing-slicing-in-tests) = true' \
    | sed 's/^/clippy.toml:/' || true
  # The flag of the canary is for the example only: code of the program behind it would be
  # in no build and in no check.
  grep -rn 'lint_canary' src tests 2>/dev/null || true
}

# Clippy on the planted violations of examples/lint_canary.rs, with the flag that compiles
# them. Fails unless the manifest declares exactly the lints of LINT_LEVELS, in the plain form,
# and each of them fires on the file at its level (REAS-DOC-006).
planted() {
  local declared expected entry code level found surprises failed=0
  if [ "${#LINT_LEVELS[@]}" -lt "$LINT_FLOOR" ]; then
    echo "FAIL: LINT_LEVELS holds ${#LINT_LEVELS[@]} lints, fewer than the floor of $LINT_FLOOR" >&2
    failed=1
  fi
  declared=$(manifest_lints | sed 's/=deny$/=error/; s/=warn$/=warning/' | sort)
  expected=$(printf '%s\n' "${LINT_LEVELS[@]}" | sort)
  if [ "$declared" != "$expected" ]; then
    echo "FAIL: the lint table of Cargo.toml and LINT_LEVELS of scripts/check.sh differ:" >&2
    diff <(printf '%s\n' "$declared") <(printf '%s\n' "$expected") >&2 || true
    failed=1
  fi
  surprises=$(lint_table_surprises)
  if [ -n "$surprises" ]; then
    echo "FAIL: a line that scripts/check.sh planted does not know changes what the lints do:" >&2
    echo "$surprises" >&2
    failed=1
  fi
  printf '\n$ cargo clippy --locked --example lint_canary --message-format=json -- --cfg lint_canary\n'
  # Clippy exits with an error here: the violations are the point. The messages are the result.
  found=$(cargo clippy --locked --example lint_canary --message-format=json -- --cfg lint_canary 2>/dev/null \
    | grep -F '"reason":"compiler-message"' | grep -F '"file_name":"examples/lint_canary.rs"' || true)
  for entry in "${LINT_LEVELS[@]}"; do
    code=${entry%=*}
    level=${entry#*=}
    # A forbidden lint is reported as an error, like a denied one; the table check above is
    # what tells the two apart.
    if [ "$level" = forbid ]; then level=error; fi
    if ! printf '%s\n' "$found" | grep -F "\"code\":{\"code\":\"$code\"" | grep -qF "\"level\":\"$level\""; then
      echo "FAIL: $code does not fire as $level on examples/lint_canary.rs" >&2
      failed=1
    fi
  done
  if [ "$failed" != 0 ]; then
    echo "To see what Clippy says, run the command above without --message-format=json." >&2
    return 1
  fi
  echo "PASS: ${#LINT_LEVELS[@]} lints fire on their planted violations at the level of Cargo.toml"
}

database() {
  needs_database database
  # Every test target of every kind, so that a new one is not left out. The unit tests run
  # here a second time; the integration tests run here only.
  run cargo test --workspace --locked
}

sqlx() {
  needs_database sqlx
  run cargo sqlx migrate run
  run cargo sqlx prepare --check -- --all-targets
}

deny() { run cargo deny check; }

audit() { run cargo audit --file Cargo.lock; }

docs() {
  run python3 scripts/check_docs.py
  run python3 -m unittest discover -s tests/docs -t .
}

privacy() {
  local status=0 terms
  run scripts/pii_canary.sh || status=1
  if [ -n "${QM_CANARY_TERMS:-}" ]; then
    terms=$QM_CANARY_TERMS
    if [ ! -r "$terms" ]; then
      echo "FAIL: QM_CANARY_TERMS names a file that cannot be read" >&2
      return 1
    fi
  else
    terms="$HOME/.config/qrtrmastr/canary-terms.txt"
  fi
  if [ -r "$terms" ]; then
    # The path of the terms file is not printed: it holds the name of an account.
    printf '\n$ scripts/pii_canary.sh --terms <your terms file>\n'
    scripts/pii_canary.sh --terms "$terms" || status=1
  else
    printf '\nNOT RUN: scripts/pii_canary.sh --terms (no terms file on this machine)\n'
    NOT_RUN+=("the canary with your terms (no terms file)")
  fi
  return $status
}

hooks() {
  run bash .claude/hooks/guard-bash.test.sh
  run bash scripts/check.test.sh
}

# The lines of a file that are not comments, with their numbers.
code_lines() { grep -nvE '^[[:space:]]*#' "$1" 2>/dev/null || true; }

# How many steps of a workflow run one group of this script.
steps_that_run() { code_lines "$1" | grep -cE "^[0-9]+:[[:space:]]*(-[[:space:]]+)?run: scripts/check\.sh $2\$" || true; }

# lists [ci-workflow] [stop-hook] [daily-workflow]: the files can be given, so that the test of
# this script can plant a violation in a copy.
lists() {
  local workflow="${1:-.github/workflows/ci.yml}" hook="${2:-.claude/hooks/stop-check.sh}"
  local scan="${3:-.github/workflows/scan.yml}"
  local failed=0 group file found trigger
  for file in "$workflow" "$scan" "$hook"; do
    if [ ! -r "$file" ]; then
      echo "FAIL: cannot read $file" >&2
      failed=1
      continue
    fi
    found=$(code_lines "$file" | grep -E "$OWN_CHECK" || true)
    if [ -n "$found" ]; then
      echo "FAIL: $file names a check of its own; put it in scripts/check.sh:" >&2
      echo "$found" >&2
      failed=1
    fi
  done
  found=$(code_lines "$workflow" | grep -E "$RISKY_KEY" | grep -vE "$KNOWN_IF" || true)
  if [ "$(code_lines "$workflow" | grep -cE "$KNOWN_IF" || true)" -gt 1 ]; then
    found="$found"$'\n'"more than one job has the condition of the dependency review"
  fi
  found="$found$(code_lines "$scan" | grep -E "$RISKY_KEY" || true)"
  if [ -n "$found" ]; then
    echo "FAIL: a workflow has a line that can make a job pass without its check:" >&2
    echo "$found" >&2
    failed=1
  fi
  for group in "${CI_GROUPS[@]}"; do
    if [ "$(steps_that_run "$workflow" "$group")" != 1 ]; then
      echo "FAIL: $workflow does not run scripts/check.sh $group in one step" >&2
      failed=1
    fi
  done
  for group in "${SCAN_GROUPS[@]}"; do
    if [ "$(steps_that_run "$scan" "$group")" != 1 ]; then
      echo "FAIL: $scan does not run scripts/check.sh $group in one step" >&2
      failed=1
    fi
  done
  # The deny job is the pinned action with the same check, in both workflows.
  for file in "$workflow" "$scan"; do
    if ! code_lines "$file" | grep -q 'uses: EmbarkStudios/cargo-deny-action@' \
      || ! code_lines "$file" | grep -qE '^[0-9]+:[[:space:]]*command: check$'; then
      echo "FAIL: $file does not run cargo deny check through its action" >&2
      failed=1
    fi
  done
  for trigger in 'pull_request:' 'push:'; do
    if ! code_lines "$workflow" | grep -qE "^[0-9]+:  $trigger\$"; then
      echo "FAIL: $workflow does not run on $trigger" >&2
      failed=1
    fi
  done
  if ! code_lines "$scan" | grep -qE '^[0-9]+:  schedule:$'; then
    echo "FAIL: $scan does not run on a schedule" >&2
    failed=1
  fi
  # A job of the daily workflow has its own name, so that its result is never taken for a
  # required check of a pull request.
  for group in daily-audit daily-deny; do
    if ! code_lines "$scan" | grep -qE "^[0-9]+:  $group:\$"; then
      echo "FAIL: $scan has no job named $group" >&2
      failed=1
    fi
  done
  if code_lines "$scan" | grep -qE '^[0-9]+:  (rust|audit|deny|secrets|privacy|docs|dependency-review):$|^[0-9]+:    name:'; then
    echo "FAIL: $scan has a job with the name of a required check" >&2
    failed=1
  fi
  for group in "${HOOK_GROUPS[@]}"; do
    if ! grep -qE "^[[:space:]]*scripts/check\.sh $group 1>&2 \|\| failed=1\$" "$hook" 2>/dev/null; then
      echo "FAIL: $hook does not run scripts/check.sh $group" >&2
      failed=1
    fi
  done
  # A workflow that this script does not know is read by no line above.
  while IFS= read -r file; do
    case "$file" in
      "$workflow" | "$scan") ;;
      *)
        echo "FAIL: $file is a workflow that scripts/check.sh does not know" >&2
        failed=1
        ;;
    esac
  done < <(find "$(dirname "$workflow")" -mindepth 1 -maxdepth 1)
  if [ $# -eq 0 ]; then
    for file in CLAUDE.md README.md docs/runbooks/develop.md; do
      if ! grep -q 'scripts/check\.sh' "$file"; then
        echo "FAIL: $file does not point at scripts/check.sh" >&2
        failed=1
      fi
    done
  fi
  if [ "$failed" != 0 ]; then return 1; fi
  printf '\nPASS: %s groups in the workflow of pull requests, %s in the daily workflow, %s in the Stop hook\n' \
    "${#CI_GROUPS[@]}" "${#SCAN_GROUPS[@]}" "${#HOOK_GROUPS[@]}"
}

all() {
  rust
  if [ -n "${DATABASE_URL:-}" ]; then
    database
  else
    printf '\nNOT RUN: scripts/check.sh database (DATABASE_URL is not set)\n'
    NOT_RUN+=("database (DATABASE_URL is not set)")
  fi
  NOT_RUN+=("sqlx (CI runs it on an empty database)")
  deny
  audit
  docs
  privacy
  hooks
  lists
}

group="${1:-all}"
case " ${GROUPS_IN_ORDER[*]} " in
  *" $group "*) ;;
  *)
    echo "usage: scripts/check.sh {$(IFS='|'; echo "${GROUPS_IN_ORDER[*]}")}" >&2
    exit 2
    ;;
esac
shift || true
"$group" "$@"
if [ ${#NOT_RUN[@]} -eq 0 ]; then
  printf '\nPASS: %s\n' "$group"
else
  printf '\nPASS: %s. NOT RUN: %s\n' "$group" "$(IFS=';'; echo "${NOT_RUN[*]}")"
fi
