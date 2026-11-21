#!/usr/bin/env bash
# Test of scripts/check.sh: its usage, and the `lists` group with planted violations
# (REAS-DOC-006). Each case copies the two workflows and the Stop hook, plants one violation in
# a copy, and asserts that `lists` fails and names it. The files of the repository do not change.
set -u
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0
cases=0
mkdir "$work/wf" "$work/hook"
ci="$work/wf/ci.yml"
scan="$work/wf/scan.yml"
hook="$work/hook/stop-check.sh"

fresh() {
  rm -f "$work"/wf/* "$work"/hook/*
  cp .github/workflows/ci.yml "$ci"
  cp .github/workflows/scan.yml "$scan"
  cp .claude/hooks/stop-check.sh "$hook"
}

# expect STATUS TEXT COMMAND...: the command exits with STATUS and prints TEXT.
expect() {
  local status=$1 text=$2 output actual
  shift 2
  cases=$((cases + 1))
  output=$("$@" 2>&1)
  actual=$?
  if [ "$actual" != "$status" ] || ! printf '%s' "$output" | grep -qF -- "$text"; then
    echo "FAIL: case $cases: $* gave status $actual, expected $status and the text: $text" >&2
    printf '%s\n' "$output" | tail -5 >&2
    failed=1
  fi
}
lists() { scripts/check.sh lists "$ci" "$hook" "$scan"; }

# Usage.
expect 2 "usage: scripts/check.sh {rust|planted|toolchain|compile|database|sqlx|deny|audit|docs|privacy|hooks|lists|all}" \
  scripts/check.sh no-such-group
expect 2 "needs DATABASE_URL" env -u DATABASE_URL scripts/check.sh database
expect 2 "needs DATABASE_URL" env -u DATABASE_URL scripts/check.sh sqlx
expect 1 "QM_CANARY_TERMS names a file that cannot be read" env QM_CANARY_TERMS="$work/absent.txt" scripts/check.sh privacy
# Each group that the head of the script describes is a group that it runs, and the reverse.
described=$(sed -n 's|^#   scripts/check\.sh \([a-z]*\) .*|\1|p' scripts/check.sh | tr '\n' ' ')
cases=$((cases + 1))
if [ "$described" != "rust planted toolchain compile database sqlx deny audit docs privacy hooks lists all " ]; then
  echo "FAIL: the head of scripts/check.sh describes: $described" >&2
  failed=1
fi
# The toolchain group: the file and rustc agree here; a workflow that installed another
# version fails, and one that installed the pinned version for a target passes.
# A stand-in for rustc gives the version, so that this test needs no toolchain: the job that
# runs it in CI installs none.
pinned=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
mkdir "$work/bin" "$work/other"
printf '#!/bin/sh\necho "rustc %s (stand-in)"\n' "$pinned" > "$work/bin/rustc"
printf '#!/bin/sh\necho "rustc 1.0.0 (stand-in)"\n' > "$work/other/rustc"
chmod +x "$work/bin/rustc" "$work/other/rustc"
expect 0 "PASS: toolchain" env PATH="$work/bin:$PATH" scripts/check.sh toolchain
expect 0 "PASS: toolchain" env PATH="$work/bin:$PATH" INSTALLED="$pinned-x86_64-unknown-linux-gnu" scripts/check.sh toolchain
for other in stable "" "${pinned}0" "1.0.0"; do
  expect 1 "the workflow installed another toolchain" env PATH="$work/bin:$PATH" INSTALLED="$other" scripts/check.sh toolchain
done
expect 1 "do not name the same toolchain" env PATH="$work/other:$PATH" scripts/check.sh toolchain

# The files of the repository pass, and so do their copies.
expect 0 "PASS: lists" scripts/check.sh lists
fresh
expect 0 "PASS: lists" lists

# A step of a workflow that runs a command of the script without the script.
for planted in \
  '      - run: cargo test --locked' \
  '        run: python scripts/check_docs.py' \
  '      - run: scripts/pii_canary.sh' \
  '        run: cargo deny check' \
  '          cargo clippy --all-targets' \
  '          active=$(rustc --version | cut -d" " -f2)' \
  '      - run: bash scripts/check.test.sh'; do
  for target in "$ci" "$scan"; do
    fresh
    printf '%s\n' "$planted" >> "$target"
    expect 1 "names a check of its own" lists
  done
done
# A comment can name a command.
fresh
printf '%s\n' '      # cargo test runs through scripts/check.sh' >> "$ci"
expect 0 "PASS: lists" lists

# A group that the workflow no longer calls: the line removed, made a comment, or made a text.
for group in toolchain rust sqlx database audit docs privacy hooks lists; do
  fresh
  grep -v "run: scripts/check\.sh $group\$" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
  fresh
  sed "s|^\( *\)run: scripts/check\.sh $group\$|\1# run: scripts/check.sh $group|" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
  fresh
  sed "s|^\( *\)run: scripts/check\.sh $group\$|\1run: scripts/check.sh $group \|\| true|" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
done
# A group in a second step.
for group in rust lists; do
  fresh
  printf '      - run: scripts/check.sh %s\n' "$group" >> "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
done
# A line that makes a job or a step pass without its check.
for planted in \
  "    if: github.event_name == 'schedule'" \
  '        if: false' \
  '      - if: ${{ false }}' \
  '        continue-on-error: true' \
  '    continue-on-error: true' \
  '        shell: echo {0}' \
  '    needs: secrets' \
  '    needs: [rust, docs]' \
  "    if: github.event_name == 'pull_request'"; do
  for target in "$ci" "$scan"; do
    fresh
    printf '%s\n' "$planted" >> "$target"
    expect 1 "can make a job pass without its check" lists
  done
done
# The workflow of pull requests without one of its triggers.
for trigger in '  pull_request:' '  push:'; do
  fresh
  grep -vxF "$trigger" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run on" lists
done
# The deny job without its action, with the action in a comment only, and with another command.
for target in ci scan; do
  fresh
  grep -v 'EmbarkStudios/cargo-deny-action@' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
  fresh
  sed 's|^\( *\)- uses: EmbarkStudios|\1# - uses: EmbarkStudios|' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
  fresh
  sed 's/^\( *\)command: check$/\1command: list/' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
done
# The daily workflow without its audit, and without its schedule.
fresh
grep -v 'run: scripts/check\.sh audit$' .github/workflows/scan.yml > "$scan"
expect 1 "does not run scripts/check.sh audit in one step" lists
fresh
grep -vxF '  schedule:' .github/workflows/scan.yml > "$scan"
expect 1 "does not run on a schedule" lists
# A third workflow, which no line of the script reads; also with a name that starts with a dot.
for name in other.yml .hidden.yml; do
  fresh
  cp "$scan" "$work/wf/$name"
  expect 1 "is a workflow that scripts/check.sh does not know" lists
  rm -f "$work/wf/$name"
done
# A job of the daily workflow with the name of a required check.
fresh
sed 's/^  daily-audit:$/  audit:/' .github/workflows/scan.yml > "$scan"
expect 1 "has no job named daily-audit" lists
fresh
sed 's/^  daily-deny:$/  daily-deny:\n    name: deny/' .github/workflows/scan.yml > "$scan"
expect 1 "has a job with the name of a required check" lists

# The Stop hook: a check of its own, and a group that it no longer calls.
fresh
printf '%s\n' 'cargo check --all-targets 1>&2 || failed=1' >> "$hook"
expect 1 "names a check of its own" lists
for group in compile docs privacy hooks lists; do
  fresh
  grep -v "scripts/check\.sh $group" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
  fresh
  sed "s|^\( *\)scripts/check\.sh $group 1>&2 .*|\1# scripts/check.sh $group 1>\&2 \|\| failed=1|" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
  fresh
  sed "s|^\( *\)scripts/check\.sh $group 1>&2 .*|\1scripts/check.sh $group 1>\&2 \|\| true|" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
done
# A file that is not there.
fresh
rm "$ci"
expect 1 "cannot read" lists

if [ "$failed" != 0 ]; then
  echo "FAIL: scripts/check.sh does not do what its test expects" >&2
  exit 1
fi
if [ "$cases" -lt 100 ]; then
  echo "FAIL: only $cases cases ran" >&2
  exit 1
fi
echo "PASS: scripts/check.sh, $cases cases"
