#!/usr/bin/env bash
# The one place that lists the checks of this repository (REAS-DOC-006).
# CI and the Stop hook call the groups of this script; CLAUDE.md, the README and the runbook
# point here. A check that is added to a group runs in each of those places.
#
#   scripts/check.sh rust       toolchain, format, the layers, compile, lint, the planted lint violations,
#                               unit tests, doc tests, documentation build
#   scripts/check.sh layers     src/domain and src/application use only the dependencies that the
#                               script lists for them, and carry no serialization attribute
#   scripts/check.sh planted    each lint of Cargo.toml fires on examples/lint_canary.rs at its level
#   scripts/check.sh toolchain  rust-toolchain.toml, rustc and (in CI) the installed toolchain agree
#   scripts/check.sh compile    compile only; what the Stop hook runs after a change to the code
#   scripts/check.sh database   every test target, with the integration tests; needs DATABASE_URL
#   scripts/check.sh sqlx       migrations on the database of DATABASE_URL, and the checked-query
#                               metadata; needs sqlx-cli. CI runs it on an empty database
#   scripts/check.sh deny       cargo deny: advisories, bans, licenses, sources
#   scripts/check.sh audit      cargo audit against the RustSec database
#   scripts/check.sh docs       documentation checker and its tests
#   scripts/check.sh privacy    the rules group, then the privacy canary: shapes, and your terms
#                               when the terms file exists
#   scripts/check.sh rules      the Read deny rules of .claude/settings.json are the ones that
#                               docs/concepts/privacy.md lists, and none names a home directory
#   scripts/check.sh hooks      the tests of the guard hook, of the privacy canary and of this script
#   scripts/check.sh targets    the fuzz targets, their manifest, their seeds and this script name
#                               the same seven; the seeds are what scripts/fuzz_seeds.py writes;
#                               the fuzz lockfile holds the versions of the program
#   scripts/check.sh fuzz       the targets group, then each fuzz target for QM_FUZZ_SECONDS
#                               (30 when not set); needs cargo-fuzz and the nightly toolchain
#                               that FUZZ_TOOLCHAIN names. Not a part of `all`
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

GROUPS_IN_ORDER=(rust layers planted toolchain compile database sqlx deny audit docs privacy rules hooks targets fuzz lists all)
# What the workflow of pull requests must call, the daily workflow, and the Stop hook.
CI_GROUPS=(toolchain rust sqlx database audit docs privacy hooks lists)
SCAN_GROUPS=(audit)
FUZZ_GROUPS=(fuzz)
HOOK_GROUPS=(compile docs privacy hooks lists)
# A line of a workflow or of the Stop hook that holds one of these runs a check of this script
# without this script.
OWN_CHECK='cargo (\+[A-Za-z0-9.-]+ +)?(fmt|check|clippy|test|doc|deny|audit|sqlx|fuzz)|python3? +scripts/fuzz_seeds|rustc --version|python3? +scripts/check_docs|python3? +-m +unittest|scripts/pii_canary\.sh|bash +\.claude/hooks/guard-bash|bash +scripts/check\.test|bash +scripts/pii_canary\.test'
# In a workflow, each of these keys can make a job pass without its check. A job that needs a
# job that failed or was skipped is skipped itself.
RISKY_KEY='^[0-9]+:[[:space:]]*(-[[:space:]]+)?(if|needs|continue-on-error|shell):'
# One `if` is known: the dependency review runs on a pull request only.
KNOWN_IF="^[0-9]+:    if: github\.event_name == 'pull_request'\$"
NOT_RUN=()
# The fuzz targets (REAS-TEST-003): one for each reader of bytes that another party wrote. A
# target that is added is added here, to fuzz/Cargo.toml and to scripts/fuzz_seeds.py in the
# same change; the `targets` group fails when the four do not agree. The list does not get
# shorter: FUZZ_FLOOR is its length. `cargo fuzz` needs a nightly toolchain; this is the one
# that CI installs, and the date moves in a pull request of its own.
FUZZ_TOOLCHAIN=nightly-2026-11-15
FUZZ_FLOOR=7
# The budget of one target in each stage, in seconds (REAS-TEST-003). The fuzz workflow holds
# the same two numbers in one line, and `lists` fails when it does not.
FUZZ_PR_SECONDS=30
FUZZ_NIGHTLY_SECONDS=300
FUZZ_TARGETS=(takeout mime classify redact receipt pack gmail_json)
# What the two inner layers may use of the dependencies of Cargo.toml, by the name that Rust
# writes (REAS-ARCH-002, REAS-DATA-001, REAS-ARCH-005). Every other dependency is a finding
# there, also one that is added to the manifest later. The domain is pure: values in, values
# out. The application layer declares the ports and may use the runtime: its mail stream is a
# channel of the runtime. Both may use the two test libraries. The modules of the standard
# library that follow are the file system, the network, the environment, the process and the
# operating system, and for the domain also paths, input and output, threads and the clock.
# To let a layer use another library, add it here and to the same list in
# scripts/check.test.sh, and say why in the pull request.
DOMAIN_MAY_USE=(hex jiff regex rust_decimal thiserror unicode_normalization proptest rstest)
APPLICATION_MAY_USE=(jiff thiserror tokio proptest rstest)
DOMAIN_STD_OUTSIDE='fs|net|env|process|os|path|io|thread|time'
APPLICATION_STD_OUTSIDE='fs|net|env|process|os'
# The fewest Rust files with content that each layer must hold, and the fewest dependencies
# that the manifest must name, for the check to count.
DOMAIN_FLOOR=10
APPLICATION_FLOOR=8
DEPENDENCY_FLOOR=30
# The lint levels of Cargo.toml, as Clippy reports them on the planted violations: `deny`
# gives an error and `warn` a warning. `forbid` keeps its own word here, because a forbidden
# lint lowered to `deny` could be expected away in the code. A lint that is added to the
# manifest is added here and gets its violation in examples/lint_canary.rs, in the same
# change. The list does not get shorter: LINT_FLOOR is its length.
LINT_FLOOR=17
FLAG_DECLARATION='unexpected_cfgs = { level = "warn", check-cfg = ["cfg(lint_canary)"] }'
LINT_LEVELS=(
  unsafe_code=forbid
  missing_docs=error
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
  clippy::missing_errors_doc=error
  clippy::missing_panics_doc=error
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
  layers
  compile
  run cargo clippy --workspace --all-targets --locked -- -D warnings
  planted
  run cargo test --workspace --locked --lib --bins
  run cargo test --workspace --locked --doc
  run env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --locked --no-deps --document-private-items
}

# The dependencies and the development dependencies of a manifest, one in a line, as Rust
# writes the name.
dependency_names() {
  awk '
    /^\[/ { on = ($0 == "[dependencies]" || $0 == "[dev-dependencies]") }
    on && /^[A-Za-z0-9_-]+[[:space:]]*=/ { sub(/[[:space:]]*=.*/, ""); gsub(/-/, "_"); print }
  ' "$1" | sort -u
}

# The lines of the two dependency tables that the reader above does not take as a name: a
# dotted key, a quoted key, an indented line, a line that continues a value.
dependency_lines_not_read() {
  awk '
    /^\[/ { on = ($0 == "[dependencies]" || $0 == "[dev-dependencies]"); next }
    on && !/^[A-Za-z0-9_-]+[[:space:]]*=/ && !/^[[:space:]]*(#.*)?$/ { print }
  ' "$1" 2>/dev/null
}

# The dependencies that a layer may not use: all of them, less the given ones, as one pattern.
outside_names() {
  local manifest=$1 name allowed
  shift
  allowed=" $* "
  dependency_names "$manifest" | while read -r name; do
    case "$allowed" in *" $name "*) ;; *) printf '%s\n' "$name" ;; esac
  done | paste -sd'|' -
}

# The lines of code below a directory that a layer must not hold. The arguments are the
# directory, the pattern of the dependencies, the pattern of the modules of the standard
# library, and a word for the domain. A line is a finding when it:
#   - names one of the dependencies as a path (`name::`, also after `::` or another path), or
#     in a `use` or an `extern crate` of the crate itself;
#   - names one of the modules as `std::name`, or after the brace of a `use std::{...}` in the
#     same line, or takes all of `std` with `std::*`;
#   - names the file system, the network or the process of the runtime (`tokio::fs`);
#   - carries a serde or SQLx attribute, also inside `cfg_attr`, or names one of the two
#     serde traits;
#   - brings code in from another file by a `path` attribute or `include!`;
#   - for the domain: names `now` after `::` or `.`, asks for the time zone of the system, or
#     prints (`println!`, `eprintln!`, `print!`, `eprint!`, `dbg!`).
# A line that is a comment and does not close a block comment is not code and is left out. The
# search reads each file as text, so that a byte that is no text does not hide a file.
layer_findings() {
  local dir=$1 names=$2 std=$3 domain=${4:-} lines status=0
  local edge='(^|[^A-Za-z0-9_])' end='([^A-Za-z0-9_]|$)'
  local pattern="$edge($names)::"
  pattern="$pattern|$edge(use|extern[[:space:]]+crate)[[:space:]]+(::)?($names)([[:space:]]*;|[[:space:]]+as[[:space:]])"
  pattern="$pattern|${edge}std::($std)$end"
  pattern="$pattern|${edge}std::\\{(.*[^A-Za-z0-9_])?($std)$end"
  pattern="$pattern|${edge}std::\\*"
  pattern="$pattern|${edge}tokio::(fs|net|process)$end"
  pattern="$pattern|$edge(serde|sqlx)[[:space:]]*\\("
  pattern="$pattern|$edge(Serialize|Deserialize)$end"
  pattern="$pattern|#!?\\[(.*[^A-Za-z0-9_])?path[[:space:]]*=|${edge}include!"
  if [ -n "$domain" ]; then
    pattern="$pattern|(::|\\.)now$end|TimeZone::system|$edge(e?print(ln)?|dbg)!"
  fi
  lines=$(grep -rnaE --include='*.rs' -- "$pattern" "$dir") || status=$?
  if [ "$status" -gt 1 ]; then
    echo "$dir: the search failed with status $status"
    return 0
  fi
  printf '%s\n' "$lines" | grep -avE '^[^:]*:[0-9]+:[[:space:]]*//([^*]|\*+[^*/])*\**$' | grep -a . || true
}

# One layer: its floor, no symbolic link, and no finding. Prints what is wrong; prints nothing
# when the layer passes.
layer() {
  local what=$1 dir=$2 floor=$3 names=$4 std=$5 clock=${6:-} count links found
  count=$({ find "$dir" -type f -name '*.rs' -size +0 2>/dev/null || true; } | wc -l)
  if [ "$count" -lt "$floor" ]; then
    echo "FAIL: $dir holds $count Rust files with content, fewer than the floor of $floor"
  fi
  links=$({ find "$dir" -type l 2>/dev/null || true; })
  if [ -n "$links" ]; then
    echo "FAIL: $dir holds a symbolic link, which the search does not follow:"
    echo "$links"
  fi
  found=$(layer_findings "$dir" "$names" "$std" "$clock")
  if [ -n "$found" ]; then
    echo "FAIL: $what names a library or a module that the layer may not use (REAS-ARCH-002, REAS-DATA-001):"
    echo "$found"
  fi
}

# The inner layers stay free of what belongs to an adapter. The arguments are for the test of
# this script: the directory of the domain, the directory of the application layer, and the
# manifest.
#
# The check reads text, not the compiler's view, so it is a gate against drift and the review
# is the control. What it does not see: a name that a `use` with braces brings in when the
# braces are not after `std::` or go over several lines; a library that another crate
# re-exports; code that a macro of another module writes; a call that reaches an adapter
# through `crate::` or `super::`; a line of code after a block comment that opened on a line
# before it. What it refuses although it is harmless: a string, a trailing comment or a block
# comment that names one of the things above (reword it), a module of the layer with the name
# of a dependency, and a pure value of a refused module, such as `std::time::Duration` in the
# domain.
layers() {
  local domain=${1:-src/domain} application=${2:-src/application} manifest=${3:-Cargo.toml}
  local count outside_domain outside_application problems=''
  printf '\n$ scripts/check.sh layers: %s uses of the dependencies only (%s), %s only (%s)\n' \
    "$domain" "${DOMAIN_MAY_USE[*]}" "$application" "${APPLICATION_MAY_USE[*]}"
  count=$(dependency_names "$manifest" | wc -l)
  if [ "$count" -lt "$DEPENDENCY_FLOOR" ]; then
    problems="FAIL: $manifest names $count dependencies, fewer than the floor of $DEPENDENCY_FLOOR"
  fi
  if grep -qE '^\[(build-dependencies|target\.|dependencies\.|dev-dependencies\.)' "$manifest" 2>/dev/null \
    || [ -n "$(dependency_lines_not_read "$manifest")" ]; then
    problems="$problems
FAIL: $manifest has a dependency in a form that scripts/check.sh layers does not read:
$(dependency_lines_not_read "$manifest")"
  fi
  outside_domain=$(outside_names "$manifest" "${DOMAIN_MAY_USE[@]}")
  outside_application=$(outside_names "$manifest" "${APPLICATION_MAY_USE[@]}")
  problems="$problems
$(layer 'the domain' "$domain" "$DOMAIN_FLOOR" "$outside_domain" "$DOMAIN_STD_OUTSIDE" clock)
$(layer 'the application layer' "$application" "$APPLICATION_FLOOR" "$outside_application" "$APPLICATION_STD_OUTSIDE")"
  problems=$(printf '%s\n' "$problems" | grep -a . || true)
  if [ -n "$problems" ]; then
    printf '%s\n' "$problems" >&2
    echo "Put the serialization, the row type or the client in src/adapters, and convert there." >&2
    echo "The head of scripts/check.sh lists what each layer may use, and how to add a library." >&2
    return 1
  fi
  echo "PASS: the domain and the application layer name no library of an adapter"
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

audit() {
  run cargo audit --file Cargo.lock
  # The fuzz package has a lockfile of its own, with three packages that the program does not
  # have.
  run cargo audit --file fuzz/Cargo.lock
}

docs() {
  run python3 scripts/check_docs.py
  run python3 -m unittest discover -s tests/docs -t .
}

# The Read deny rules of a settings file, one in a line, sorted. The file is read as JSON, as
# Claude Code reads it: a file that is no JSON, a second `deny` key that replaces the first,
# or a list under another name gives no rule here, as it gives no rule to a session.
read_deny_rules() {
  { python3 - "$1" <<'PY' 2>/dev/null || true; } | LC_ALL=C sort -u
import json, sys
settings = json.load(open(sys.argv[1], encoding="utf-8"))
for rule in settings["permissions"]["deny"]:
    if isinstance(rule, str) and rule.startswith("Read("):
        print(rule)
PY
}

# The places that a session must not read are a list in two files: the settings of the
# project, which every clone gets, and the page that says what each rule is for. The arguments
# are for the test of this script: the settings file and the page. RULE_FLOOR is the number
# of the rules: a rule that is taken out of both files fails here.
RULE_FLOOR=10
rules() {
  local settings=${1:-.claude/settings.json} page=${2:-docs/concepts/privacy.md}
  local in_settings in_page count failed=0
  printf '\n$ scripts/check.sh rules: the Read deny rules of %s and of %s are the same\n' "$settings" "$page"
  in_settings=$(read_deny_rules "$settings")
  # In the page a rule is a code span that holds nothing but the rule.
  in_page=$(grep -oE '`Read\([^`]*\)`' "$page" 2>/dev/null | tr -d '`' | LC_ALL=C sort -u || true)
  count=$(printf '%s\n' "$in_settings" | grep -c . || true)
  if [ "$count" -lt "$RULE_FLOOR" ]; then
    echo "FAIL: $settings holds $count Read deny rules, fewer than the floor of $RULE_FLOOR" >&2
    failed=1
  fi
  if [ "$in_settings" != "$in_page" ]; then
    echo "FAIL: the Read deny rules of $settings (<) and the rules that $page lists (>) differ:" >&2
    diff <(printf '%s\n' "$in_settings") <(printf '%s\n' "$in_page") >&2 || true
    failed=1
  fi
  # A rule with the home directory of one person protects that person only, and it puts the
  # name of the account into a tracked file. `~/` is the home of whoever runs the session.
  if printf '%s\n' "$in_settings" | grep -qiE '/(home|users)/|\$\{?HOME'; then
    echo "FAIL: a Read deny rule of $settings names a home directory; write it with ~/" >&2
    failed=1
  fi
  if [ "$failed" != 0 ]; then return 1; fi
  echo "PASS: $count Read deny rules, the same in the settings and in the page"
}

privacy() {
  local status=0 terms
  rules || status=1
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
  run bash scripts/pii_canary.test.sh
  run bash scripts/check.test.sh
}

# The fuzz targets are named in four places, and their seeds and their lockfile are derived
# files. The argument is for the test of this script: the directory of the fuzz package.
targets() {
  local dir=${1:-fuzz} failed=0 here in_manifest in_files with_seeds
  printf '\n$ scripts/check.sh targets: the fuzz targets of %s, of its manifest, of its seeds and of this script are the same\n' "$dir"
  if [ "${#FUZZ_TARGETS[@]}" -lt "$FUZZ_FLOOR" ]; then
    echo "FAIL: FUZZ_TARGETS holds ${#FUZZ_TARGETS[@]} targets, fewer than the floor of $FUZZ_FLOOR" >&2
    failed=1
  fi
  here=$(printf '%s\n' "${FUZZ_TARGETS[@]}" | LC_ALL=C sort)
  in_manifest=$({ grep -A1 '^\[\[bin\]\]$' "$dir/Cargo.toml" 2>/dev/null || true; } | sed -n 's/^name = "\(.*\)"$/\1/p' | LC_ALL=C sort)
  in_files=$({ find "$dir/fuzz_targets" -maxdepth 1 -name '*.rs' 2>/dev/null || true; } | sed 's|.*/||; s|\.rs$||' | LC_ALL=C sort)
  with_seeds=$({ find "$dir/seeds" -mindepth 2 -maxdepth 2 -type f -size +0 2>/dev/null || true; } | sed 's|/[^/]*$||; s|.*/||' | LC_ALL=C sort -u)
  if [ "$here" != "$in_manifest" ]; then
    echo "FAIL: FUZZ_TARGETS of scripts/check.sh (<) and the [[bin]] entries of $dir/Cargo.toml (>) differ:" >&2
    diff <(printf '%s\n' "$here") <(printf '%s\n' "$in_manifest") >&2 || true
    failed=1
  fi
  if [ "$here" != "$in_files" ]; then
    echo "FAIL: FUZZ_TARGETS of scripts/check.sh (<) and the files of $dir/fuzz_targets (>) differ:" >&2
    diff <(printf '%s\n' "$here") <(printf '%s\n' "$in_files") >&2 || true
    failed=1
  fi
  if [ "$here" != "$with_seeds" ]; then
    echo "FAIL: FUZZ_TARGETS of scripts/check.sh (<) and the targets with a seed in $dir/seeds (>) differ:" >&2
    diff <(printf '%s\n' "$here") <(printf '%s\n' "$with_seeds") >&2 || true
    failed=1
  fi
  if [ $# -eq 0 ]; then
    run python3 scripts/fuzz_seeds.py --check || failed=1
    # A fuzz build that takes another version of a library tests another program.
    printf '\n$ the packages of fuzz/Cargo.lock are at the versions of Cargo.lock\n'
    python3 - <<'PY' || failed=1
import sys, tomllib
program = {}
for package in tomllib.load(open("Cargo.lock", "rb"))["package"]:
    program.setdefault(package["name"], set()).add(package["version"])
own = {"qrtrmastr-fuzz", "libfuzzer-sys", "arbitrary", "jobserver"}
other = sorted(
    f'{package["name"]} {package["version"]}'
    for package in tomllib.load(open("fuzz/Cargo.lock", "rb"))["package"]
    if package["name"] not in own and package["version"] not in program.get(package["name"], set())
)
if other:
    print("FAIL: fuzz/Cargo.lock holds a package at a version that Cargo.lock does not have:", file=sys.stderr)
    print("\n".join(other), file=sys.stderr)
    print("Copy Cargo.lock to fuzz/Cargo.lock and build the targets once; cargo keeps the versions.", file=sys.stderr)
    sys.exit(1)
PY
  fi
  if [ "$failed" != 0 ]; then return 1; fi
  echo "PASS: ${#FUZZ_TARGETS[@]} fuzz targets, each in the manifest, in a file and with a seed"
}

# Runs each fuzz target for a time budget, from the inputs that earlier runs found
# (fuzz/corpus, which git ignores and CI keeps in its cache) and from the committed seeds. A
# target fails on a stop of the program and on a property that does not hold.
fuzz() {
  local seconds=${QM_FUZZ_SECONDS:-$FUZZ_PR_SECONDS} target failed=0 artifact
  targets
  case "$seconds" in
    '' | *[!0-9]*) echo "FAIL: QM_FUZZ_SECONDS is not a number of seconds" >&2; return 2 ;;
  esac
  if ! cargo "+$FUZZ_TOOLCHAIN" fuzz --version > /dev/null 2>&1; then
    echo "scripts/check.sh fuzz needs cargo-fuzz and the toolchain $FUZZ_TOOLCHAIN (see docs/runbooks/develop.md)" >&2
    exit 2
  fi
  for target in "${FUZZ_TARGETS[@]}"; do
    mkdir -p "fuzz/corpus/$target"
    if ! run cargo "+$FUZZ_TOOLCHAIN" fuzz run "$target" "fuzz/corpus/$target" "fuzz/seeds/$target" -- \
      "-max_total_time=$seconds" -rss_limit_mb=2048 -print_final_stats=1; then
      failed=1
      # The input of the failure, as text that a log can carry. It is made by the fuzzer from
      # the synthetic seeds; no real mail is near it.
      while IFS= read -r artifact; do
        echo "FAIL: $target failed on the input $artifact, in base64:" >&2
        base64 -w 76 "$artifact" >&2
      done < <(find "fuzz/artifacts/$target" -type f 2>/dev/null)
    fi
  done
  if [ "$failed" != 0 ]; then
    echo "A failure is a defect of the reader or a property that is not true: docs/runbooks/develop.md says how to read it." >&2
    return 1
  fi
  echo "PASS: ${#FUZZ_TARGETS[@]} fuzz targets ran for $seconds seconds each without a failure"
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
  local fuzzing="${4:-$(dirname "${1:-.github/workflows/ci.yml}")/fuzz.yml}"
  local failed=0 group file found trigger
  for file in "$workflow" "$scan" "$fuzzing" "$hook"; do
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
  found="$found$(code_lines "$fuzzing" | grep -E "$RISKY_KEY" || true)"
  # This trigger runs the workflow of the default branch with the code of a pull request and
  # a token of the repository. No workflow here needs it.
  for file in "$workflow" "$scan" "$fuzzing"; do
    found="$found$(code_lines "$file" | grep -E 'pull_request_target' || true)"
  done
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
  for group in "${FUZZ_GROUPS[@]}"; do
    if [ "$(steps_that_run "$fuzzing" "$group")" != 1 ]; then
      echo "FAIL: $fuzzing does not run scripts/check.sh $group in one step" >&2
      failed=1
    fi
  done
  # The fuzz workflow: on a pull request and on a schedule, with the budget of each stage in
  # the file (REAS-TEST-003), and with the toolchain that this script names.
  for trigger in 'pull_request:' 'schedule:'; do
    if ! code_lines "$fuzzing" | grep -qE "^[0-9]+:  $trigger\$"; then
      echo "FAIL: $fuzzing does not run on $trigger" >&2
      failed=1
    fi
  done
  # One line sets the budget, for the whole job, to the two numbers of this script. A second
  # line, in a step, could set another.
  found="      QM_FUZZ_SECONDS: \${{ github.event_name == 'pull_request' && '$FUZZ_PR_SECONDS' || '$FUZZ_NIGHTLY_SECONDS' }}"
  if [ "$(code_lines "$fuzzing" | grep -c 'QM_FUZZ_SECONDS' || true)" != 1 ] \
    || ! grep -qxF -- "$found" "$fuzzing" 2>/dev/null; then
    echo "FAIL: $fuzzing does not set QM_FUZZ_SECONDS once, to $FUZZ_PR_SECONDS on a pull request and $FUZZ_NIGHTLY_SECONDS in the nightly run" >&2
    failed=1
  fi
  # A filter on a trigger, a schedule that is not daily, and a token that can write.
  if code_lines "$fuzzing" | grep -qE '^[0-9]+:[[:space:]]*(paths|paths-ignore|branches|branches-ignore|tags|types):'; then
    echo "FAIL: $fuzzing has a filter on a trigger; the targets run for every pull request" >&2
    failed=1
  fi
  if ! code_lines "$fuzzing" | grep -qE '^[0-9]+:    - cron: "[0-9]{1,2} [0-9]{1,2} \* \* \*"$'; then
    echo "FAIL: $fuzzing does not run once a day" >&2
    failed=1
  fi
  if ! code_lines "$fuzzing" | grep -qE '^[0-9]+:  contents: read$' \
    || code_lines "$fuzzing" | grep -qE 'write'; then
    echo "FAIL: $fuzzing does not give the job a token that can only read" >&2
    failed=1
  fi
  if ! code_lines "$fuzzing" | grep -qE "^[0-9]+: +toolchain: $FUZZ_TOOLCHAIN\$"; then
    echo "FAIL: $fuzzing does not install the toolchain $FUZZ_TOOLCHAIN of scripts/check.sh" >&2
    failed=1
  fi
  # The tool at a version, so that a run of another day builds with the same fuzzer.
  if ! code_lines "$fuzzing" | grep -qE '^[0-9]+: +tool: cargo-fuzz@[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "FAIL: $fuzzing does not install cargo-fuzz at a version" >&2
    failed=1
  fi
  if code_lines "$fuzzing" | grep -qE '^[0-9]+:  (rust|audit|deny|secrets|privacy|docs|dependency-review):$'; then
    echo "FAIL: $fuzzing has a job with the name of a required check" >&2
    failed=1
  fi
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
      "$workflow" | "$scan" | "$fuzzing") ;;
      *)
        echo "FAIL: $file is a workflow that scripts/check.sh does not know" >&2
        failed=1
        ;;
    esac
  done < <(find "$(dirname "$workflow")" -mindepth 1 -maxdepth 1)
  if [ $# -eq 0 ]; then
    # The targets group needs no toolchain, so it runs here, in a job that is required.
    targets || failed=1
    for file in CLAUDE.md README.md docs/runbooks/develop.md; do
      if ! grep -q 'scripts/check\.sh' "$file"; then
        echo "FAIL: $file does not point at scripts/check.sh" >&2
        failed=1
      fi
    done
  fi
  if [ "$failed" != 0 ]; then return 1; fi
  printf '\nPASS: %s groups in the workflow of pull requests, %s in the daily workflow, %s in the fuzz workflow, %s in the Stop hook\n' \
    "${#CI_GROUPS[@]}" "${#SCAN_GROUPS[@]}" "${#FUZZ_GROUPS[@]}" "${#HOOK_GROUPS[@]}"
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
  NOT_RUN+=("fuzz (scripts/check.sh fuzz runs the targets; CI runs them in the fuzz workflow)")
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
