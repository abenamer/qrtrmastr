#!/usr/bin/env bash
# Writes the lists of `qm due` as a web page and says where it is (ADR 0012).
# Usage: scripts/due_page.sh [FILE]   default: ~/qm-data/next-order.html
# The page holds your purchase history; keep it outside the repository.
set -euo pipefail
out="${1:-${QM_DATA_DIR:-$HOME/qm-data}/next-order.html}"
# The path is the caller's: made absolute before the script moves to the repository, so that
# a relative path never puts the page inside the repository by accident.
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
cd "$(dirname "$0")/.."
tmp="$(mktemp "${out}.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
# -T: no terminal, so the page arrives byte for byte. 40 extras: the other choices.
docker compose run --rm -T qm due --html --top 40 > "$tmp"
grep -q '^<!doctype html>' "$tmp" || { echo "qm due --html printed no page" >&2; exit 1; }
mv "$tmp" "$out"
trap - EXIT
echo "wrote $out"
if command -v wslpath >/dev/null 2>&1; then
  echo "open in Windows: $(wslpath -w "$out")"
fi
