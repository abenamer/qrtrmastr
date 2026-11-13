#!/usr/bin/env bash
# The privacy canary, half of the redaction guarantee (docs/concepts/privacy.md). Two modes:
#
#   scripts/pii_canary.sh                 shape mode, what CI runs: no tracked file under
#                                         tests/fixtures/local or backups (except .gitkeep), and
#                                         no tracked fixture, document, migration or source file
#                                         matches the shape of a phone number, a masked card
#                                         number, an email address or a street address.
#   scripts/pii_canary.sh --terms FILE    term mode, what the local hook runs: every line of
#                                         FILE (your real address, phone, name, email) must be
#                                         absent from every tracked file. FILE lives outside the
#                                         repository and is never committed.
#
# The canary fails closed: a grep that cannot read a file, or any status other than "no match",
# is a failure, never a pass. File names are passed as arguments, never through a pipe, so a name
# with a space or a non-ASCII byte is scanned like any other.
#
# Exit 0 when clean, 1 when something matched or could not be scanned, 2 on usage error.
set -u
set -o pipefail

mode=shape
terms_file=""
case "${1:-}" in
  "") ;;
  --terms) terms_file="${2:-}"; mode=terms; [ -n "$terms_file" ] || { echo "usage: $0 [--terms FILE]" >&2; exit 2; } ;;
  *) echo "usage: $0 [--terms FILE]" >&2; exit 2 ;;
esac

cd "$(git rev-parse --show-toplevel)" || exit 2
failed=0

# Directories whose contents must never be tracked.
if [ -n "$(git ls-files tests/fixtures/local)" ]; then
  echo "FAIL: files under tests/fixtures/local are tracked; real receipts never enter git:" >&2
  git ls-files tests/fixtures/local >&2
  failed=1
fi
tracked_backups=$(git ls-files -z backups | tr '\0' '\n' | grep -v '^backups/\.gitkeep$' || true)
if [ -n "$tracked_backups" ]; then
  echo "FAIL: files under backups are tracked; a database dump is treated like the database:" >&2
  echo "$tracked_backups" >&2
  failed=1
fi

# Tracked text files that could carry personal data, as NUL-separated names into an array.
scan=()
while IFS= read -r -d '' path; do
  case "$path" in
    *.png|*.jpg|*.gif|*.ico|*.lock) ;;
    # Synthetic inputs carry fake personal data on purpose; the redactor's outputs, snapshotted
    # under src/ and tests/, are what the canary guards (docs/concepts/privacy.md).
    tests/fixtures/synthetic/input/*) ;;
    *) scan+=("$path") ;;
  esac
done < <(git ls-files -z -- 'tests/fixtures' 'docs' 'src' 'migrations' 'README.md' 'CHANGELOG.md')
if [ ${#scan[@]} -eq 0 ]; then
  echo "PASS: nothing to scan"
  exit $failed
fi

# Runs grep over the scan set and classifies its status: 0 matched (prints the hits), 1 clean,
# anything else is an error that fails the run. $1 is the grep mode flag, $2 the pattern.
scan_for() {
  local hits status
  hits=$(grep "$1" -nH -- "$2" "${scan[@]}" 2>&1)
  status=$?
  case $status in
    0) printf '%s\n' "$hits"; return 0 ;;
    1) return 1 ;;
    *) echo "FAIL: grep could not scan every file (status $status):" >&2; echo "$hits" >&2; return 2 ;;
  esac
}

if [ "$mode" = shape ]; then
  # Phone: 10 US digits with separators or parentheses. Email. Masked card: stars or x's then four
  # digits, or "ending in NNNN". Street address: a number, a word or two, and a street suffix.
  patterns=(
    '\(?[2-9][0-9]{2}\)?[-. ][0-9]{3}[-. ][0-9]{4}'
    '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
    '([*xX]{2,}|ending in )[0-9]{4}'
    '\b[0-9]{1,5} ([A-Z][a-z]+ ){1,3}(Street|St\.|Avenue|Ave\.|Road|Rd\.|Boulevard|Blvd\.|Place|Pl\.|Court|Ct\.|Drive|Dr\.|Lane|Ln\.|Parkway|Pkwy\.)(\b|$)'
  )
  # Shapes that cannot be a person's data and are allowed to appear in tests and documents:
  #   - addresses in the RFC 2606 reserved domains (.example, .test, .invalid, example.com/net/org);
  #   - the merchant's own sender addresses at freshdirect.com, which are configuration;
  #   - phone numbers in 555-0100 to 555-0199, the range reserved for fiction;
  #   - the test card 4242 4242 4242 4242, whose last four the synthetic data uses;
  #   - invented street names ("Imaginary Street", "Example Street") used by the synthetic data.
  # Anything else of these shapes is a finding, including a real street, a real number, a
  # personal mail domain.
  reserved='@([A-Za-z0-9.-]+\.)?(example|test|invalid|localhost)([^A-Za-z0-9]|$)|@example\.(com|net|org)([^A-Za-z0-9]|$)|@([A-Za-z0-9.-]+\.)?freshdirect\.com([^A-Za-z0-9]|$)|555[-. ]?01[0-9]{2}|([*xX]{2,}|ending in )[^0-9]{0,8}4242|\b(Imaginary|Example|Fictional) (Street|Avenue|Plaza|Road|Place|Lane|Drive)\b'
  # Each match is judged on its own (grep -o), so a real value cannot hide behind a fictional one
  # on the same line, and only file and line are printed: a matched value may be the real thing.
  for pattern in "${patterns[@]}"; do
    if hits=$(scan_for -Eo "$pattern"); then
      hits=$(printf '%s\n' "$hits" | grep -Eiv -- "$reserved" | cut -d: -f1,2 | sort -u || true)
      if [ -n "$hits" ]; then
        echo "FAIL: a personal-data shape (/$pattern/) appears at:" >&2
        echo "$hits" >&2
        failed=1
      fi
    elif [ $? -eq 2 ]; then
      failed=1
    fi
  done
else
  [ -r "$terms_file" ] || { echo "cannot read terms file $terms_file" >&2; exit 2; }
  while IFS= read -r term || [ -n "$term" ]; do
    term=$(printf '%s' "$term" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    [ -n "$term" ] || continue
    if hits=$(scan_for -Fi "$term"); then
      # The term itself is personal data: report where, not what.
      echo "FAIL: a configured personal-data term appears in:" >&2
      printf '%s\n' "$hits" | cut -d: -f1,2 >&2
      failed=1
    elif [ $? -eq 2 ]; then
      failed=1
    fi
  done < "$terms_file"
fi

if [ $failed = 0 ]; then echo "PASS: privacy canary ($mode mode)"; fi
exit $failed
