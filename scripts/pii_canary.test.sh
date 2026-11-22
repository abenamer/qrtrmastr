#!/usr/bin/env bash
# Test of scripts/pii_canary.sh with planted violations (REAS-DOC-006). Each case makes a small
# repository of its own, with the files that the canary must read, plants one thing in it, and
# asserts that the canary fails and names the file, or passes. The repository of the project
# does not change.
#
# A planted value is put together from two pieces here, so that this file does not hold the
# shape itself: the canary reads this file too.
set -u
# Inside a git hook, git gives the hook the index and the directory of the repository in these
# variables, and each git command below would then write there. The test works on its own
# repositories only.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_PREFIX
cd "$(dirname "$0")/.."
canary="$PWD/scripts/pii_canary.sh"

work=$(mktemp -d)
trap 'chmod -R u+rw "$work" 2>/dev/null; rm -rf "$work"' EXIT
failed=0
cases=0
number=0

# The files that the canary must read, each with one harmless line.
MUST_READ=(.claude/settings.json CLAUDE.md AGENTS.md README.md .github/workflows/ci.yml
  scripts/pii_canary.sh standards/reas/README.md docs/index.md src/main.rs Cargo.toml)

# fresh: a new repository with the files above; its path is in $repo.
fresh() {
  local path
  number=$((number + 1))
  repo="$work/repo$number"
  mkdir "$repo"
  git -C "$repo" init --quiet
  for path in "${MUST_READ[@]}"; do
    mkdir -p "$repo/$(dirname "$path")"
    printf 'a harmless line\n' > "$repo/$path"
  done
}

# expect STATUS TEXT REPO [ARGUMENTS]: the canary, run in REPO over its index, exits with STATUS
# and prints TEXT. The files of REPO are staged first, unless STAGED is set.
expect() {
  local status=$1 text=$2 repo=$3 output actual
  shift 3
  cases=$((cases + 1))
  if [ -z "${STAGED:-}" ]; then git -C "$repo" add --all; fi
  output=$(cd "$repo" && "$canary" "$@" 2>&1)
  actual=$?
  if [ "$actual" != "$status" ] || ! printf '%s' "$output" | grep -qF -- "$text"; then
    echo "FAIL: case $cases: gave status $actual, expected $status and the text: $text" >&2
    printf '%s\n' "$output" | tail -5 >&2
    failed=1
  fi
}

# The values. Each is the shape of personal data and belongs to nobody: the phone number has
# an exchange that starts with 0, which the numbering plan gives to no line.
phone="212-012""-3456"
mail="someone""@""mailhost.org"
card="ending"" in 9876"
street="12 Madeup Real"" Street"
home_linux="/ho""me/somebody/qm-data"
home_mac="/Us""ers/somebody/Library"
home_with_signs="/Us""ers/some.body+x/Library"

# A repository with nothing planted passes, in both modes.
fresh
expect 0 "PASS: privacy canary (shape mode)" "$repo"
printf 'Zebediah Quuxley\n' > "$work/terms.txt"
expect 0 "PASS: privacy canary (terms mode)" "$repo" --terms "$work/terms.txt"

# Each shape, in each area that the canary reads: the areas it read before 2026-11-21 and the
# ones it did not read then.
for area in docs/page.md src/lib.rs tests/case.rs migrations/0001.sql README.md CHANGELOG.md \
  .claude/settings.json .claude/hooks/hook.sh .claude/rules/rule.md CLAUDE.md AGENTS.md \
  scripts/tool.sh .github/workflows/ci.yml standards/reas/standard/rule.md examples/one.rs \
  Cargo.toml compose.yaml new-at-the-top.txt; do
  for value in "$phone" "$mail" "$card" "$street" "$home_linux" "$home_mac" "$home_with_signs"; do
    fresh
    mkdir -p "$repo/$(dirname "$area")"
    printf 'a line with %s in it\n' "$value" >> "$repo/$area"
    expect 1 "$area:" "$repo"
  done
done

# The reserved forms, which cannot be a person's data, pass.
for value in "/ho""me/x/qm-data" "/ho""me/runner/work" "/Us""ers/alice/secrets" "(212) 555""-0142" \
  "pat""@""example.com" "service""@""freshdirect.com" "123 Imaginary"" Street" "~/qm-data/takeout"; do
  fresh
  printf 'a line with %s in it\n' "$value" >> "$repo/.claude/settings.json"
  expect 0 "PASS: privacy canary (shape mode)" "$repo"
done
# The path of an API that starts with the word in lower case is no home directory.
fresh
printf 'let path = "/us""ers/me/messages";\n' | sed 's/""//' >> "$repo/src/main.rs"
expect 0 "PASS: privacy canary (shape mode)" "$repo"
# A name that only starts like a reserved one is a finding, also with a sign that an account
# name can hold.
for value in "/ho""me/xavier/qm-data" "/ho""me/alice+work/qm-data" "/ho""me/user@host/qm-data" "/ho""me/x.y/qm-data" "/Us""ers/alice2/x"; do
  fresh
  printf 'a line with %s in it\n' "$value" >> "$repo/CLAUDE.md"
  expect 1 "CLAUDE.md:" "$repo"
done

# What the canary leaves out: an image, a lockfile and the synthetic inputs.
for left_out in logo.png Cargo.lock tests/fixtures/synthetic/input/receipt.html; do
  fresh
  mkdir -p "$repo/$(dirname "$left_out")"
  printf 'a line with %s in it\n' "$phone" > "$repo/$left_out"
  expect 0 "PASS: privacy canary (shape mode)" "$repo"
done

# Term mode: a term in a file of a session is found, the place is named, and the term is not
# printed. A term in the owner part of a GitHub address is not a finding.
fresh
mkdir -p "$repo/.claude/rules"
printf 'written by Zebediah Quuxley\n' >> "$repo/.claude/rules/rule.md"
printf 'written by ZEBEDIAH QUUXLEY\n' >> "$repo/.claude/settings.json"
expect 1 ".claude/rules/rule.md:" "$repo" --terms "$work/terms.txt"
expect 1 ".claude/settings.json:" "$repo" --terms "$work/terms.txt"
cases=$((cases + 1))
if (cd "$repo" && "$canary" --terms "$work/terms.txt" 2>&1) | grep -qi 'quuxley'; then
  echo "FAIL: case $cases: the canary printed a term" >&2
  failed=1
fi
fresh
printf 'quuxley\n' > "$work/owner-terms.txt"
printf 'see https://github.com/quuxley/project\n' >> "$repo/README.md"
expect 0 "PASS: privacy canary (terms mode)" "$repo" --terms "$work/owner-terms.txt"
printf 'see https://gitlab.com/quuxley/project\n' >> "$repo/README.md"
expect 1 "README.md:" "$repo" --terms "$work/owner-terms.txt"

# The floor: a file that the canary must read and that is not tracked.
for needed in "${MUST_READ[@]}"; do
  fresh
  rm "$repo/$needed"
  expect 1 "the canary does not read $needed" "$repo"
done
# An area that is in the index and left out of the scan is the same failure. The copy of the
# canary below reads the old areas only, as the canary did before 2026-11-21.
fresh
mkdir -p "$work/old"
sed "s|done < <(git ls-files -z)|done < <(git ls-files -z -- 'tests' 'docs' 'src' 'migrations' 'README.md' 'CHANGELOG.md')|" "$canary" > "$work/old/pii_canary.sh"
chmod +x "$work/old/pii_canary.sh"
cases=$((cases + 1))
git -C "$repo" add --all
output=$(cd "$repo" && "$work/old/pii_canary.sh" 2>&1)
if [ $? != 1 ] || ! printf '%s' "$output" | grep -qF "the canary does not read .claude/settings.json"; then
  echo "FAIL: case $cases: a canary that reads the old areas only did not fail on its floor" >&2
  failed=1
fi

# The cap on what is left out. Eight files of the kinds that the canary leaves out pass; one
# more fails, whatever its kind: an exclusion that is added to the canary leaves out more.
fresh
for n in 1 2 3 4 5 6 7 8; do printf 'x\n' > "$repo/image$n.png"; done
expect 0 "PASS: privacy canary (shape mode)" "$repo"
printf 'x\n' > "$repo/image9.png"
expect 1 "the canary leaves out 9 tracked files, more than 8" "$repo"
cases=$((cases + 1))
if ! grep -qx 'LEFT_OUT_MAX=8' "$canary"; then
  echo "FAIL: case $cases: scripts/pii_canary.sh does not hold the line LEFT_OUT_MAX=8" >&2
  failed=1
fi

# A file with a byte that is no text, such as a seed that is an archive, is read as text: a
# shape in it is a finding with its place, and a reserved form in it passes.
fresh
mkdir -p "$repo/fuzz/seeds/takeout"
printf 'PK\003\004\000\000 a line with %s in it\n' "$mail" > "$repo/fuzz/seeds/takeout/archive"
expect 1 "fuzz/seeds/takeout/archive:" "$repo"
fresh
mkdir -p "$repo/fuzz/seeds/takeout"
printf 'PK\003\004\000\000 a line with %s in it\n' "pat""@""example.com" > "$repo/fuzz/seeds/takeout/archive"
expect 0 "PASS: privacy canary (shape mode)" "$repo"

# A tracked file where none may be tracked.
fresh
mkdir -p "$repo/tests/fixtures/local"
printf 'a receipt\n' > "$repo/tests/fixtures/local/receipt.html"
expect 1 "files under tests/fixtures/local are tracked" "$repo"
fresh
mkdir -p "$repo/backups"
printf 'a dump\n' > "$repo/backups/qm.dump"
expect 1 "files under backups are tracked" "$repo"
fresh
mkdir -p "$repo/backups"
: > "$repo/backups/.gitkeep"
expect 0 "PASS: privacy canary (shape mode)" "$repo"

# A file that cannot be read is a failure, not a file without findings. A superuser reads
# every file, so the case is left out for one.
if [ "$(id -u)" != 0 ]; then
  fresh
  git -C "$repo" add --all
  chmod 000 "$repo/CLAUDE.md"
  STAGED=1 expect 1 "grep could not scan every file" "$repo"
  STAGED=1 expect 1 "grep could not scan every file" "$repo" --terms "$work/terms.txt"
  chmod 600 "$repo/CLAUDE.md"
fi

# Usage.
fresh
expect 2 "usage:" "$repo" --other
expect 2 "usage:" "$repo" --terms
expect 2 "cannot read terms file" "$repo" --terms "$work/absent.txt"

if [ "$failed" != 0 ]; then
  echo "FAIL: scripts/pii_canary.sh does not do what its test expects" >&2
  exit 1
fi
if [ "$cases" -lt 170 ]; then
  echo "FAIL: only $cases cases ran" >&2
  exit 1
fi
echo "PASS: scripts/pii_canary.sh, $cases cases"
