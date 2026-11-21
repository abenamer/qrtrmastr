#!/bin/bash
# Stop hook: the checks that must pass before a turn counts as done, run only over what the
# session changed. scripts/check.sh is the one list of the checks; this hook picks the groups
# by the paths that differ from the pushed branch, and `scripts/check.sh lists` fails when a
# check is named here and not there.
#   Rust, a migration or a Cargo file           the compile group
#   a Markdown file, the checker, its tests     the docs group
#   a hook, a check script, a workflow, or one
#   of the three documents that `lists` reads   the hooks and lists groups
#   always                                      the privacy group
# A change to scripts/check.sh runs its test here, not each group that it changed; CI runs
# every group.
# Exit 2 blocks the stop and sends the output to Claude; stop_hook_active stops the loop
# once Claude has already been sent back.
INPUT=$(cat)
if [ "$(echo "$INPUT" | jq -r '.stop_hook_active // false')" = "true" ]; then
  exit 0
fi
cd "$CLAUDE_PROJECT_DIR" 2>/dev/null || exit 0

# core.quotePath=false: git writes a name with a letter outside ASCII as it is, not in quotes
# with escapes, so the patterns below see its extension.
changed=$( { git -c core.quotePath=false diff --name-only HEAD 2>/dev/null; git -c core.quotePath=false ls-files --others --exclude-standard 2>/dev/null; } )
upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null)
if [ -n "$upstream" ]; then
  changed="$changed"$'\n'"$(git -c core.quotePath=false diff --name-only "$upstream...HEAD" 2>/dev/null)"
fi

rust=$(echo "$changed" | grep -E '^(src|tests|migrations|examples|benches|\.sqlx|\.cargo)/|^Cargo\.(toml|lock)$|^build\.rs$|^clippy\.toml$|^rust-toolchain\.toml$' | head -1)
docs=$(echo "$changed" | grep -iE '\.(md|markdown)"?$|^scripts/check_docs\.py$|^tests/docs/|^requirements-docs\.txt$' | head -1)
hooks=$(echo "$changed" | grep -E '^\.claude/hooks/|^scripts/check(\.test)?\.sh$|^scripts/pii_canary(\.test)?\.sh$|^\.github/workflows/|^CLAUDE\.md$|^README\.md$|^docs/runbooks/develop\.md$' | head -1)

failed=0
if [ -n "$rust" ]; then
  scripts/check.sh compile 1>&2 || failed=1
fi
if [ -n "$docs" ]; then
  scripts/check.sh docs 1>&2 || failed=1
fi
if [ -n "$hooks" ]; then
  scripts/check.sh hooks 1>&2 || failed=1
  scripts/check.sh lists 1>&2 || failed=1
fi
# The privacy canary: shapes always; your real terms when the terms file exists
# (docs/concepts/privacy.md).
scripts/check.sh privacy 1>&2 || failed=1

if [ "$failed" = 1 ]; then
  echo "A check above failed. Fix it before finishing; scripts/check.sh names each group." >&2
  exit 2
fi
exit 0
