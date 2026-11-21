#!/usr/bin/env bash
# The privacy canary, half of the redaction guarantee (docs/concepts/privacy.md). Two modes:
#
#   scripts/pii_canary.sh                 shape mode, what CI runs: no tracked file under
#                                         tests/fixtures/local or backups (except .gitkeep), and
#                                         no tracked file matches the shape of a phone number, a
#                                         masked card number, an email address, a street address
#                                         or the home directory of a person.
#   scripts/pii_canary.sh --terms FILE    term mode, what the local hook runs: every line of
#                                         FILE (your real address, phone, name, email) must be
#                                         absent from every tracked file. FILE lives outside the
#                                         repository and is never committed.
#
# Both modes read every tracked file, also the settings and hooks of a Claude session, the
# workflows, the scripts and the vendored standard. Left out: images, lockfiles, and the
# synthetic input fixtures, which carry fake personal data on purpose.
#
# The canary fails closed: a grep that cannot read a file, or any status other than "no match",
# is a failure, never a pass. A run that does not read each of the files in MUST_READ fails too,
# and so does a run that leaves out more tracked files than LEFT_OUT_MAX. File names are passed as arguments, never through a pipe, so a name
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
done < <(git ls-files -z)
# A floor (REAS-DOC-006): a canary that read nothing, or not these files, did not pass. Each of
# these is a file that the canary did not read before 2026-11-21, or the root of an area.
MUST_READ=(.claude/settings.json CLAUDE.md AGENTS.md README.md .github/workflows/ci.yml
  scripts/pii_canary.sh standards/reas/README.md docs/index.md src/main.rs Cargo.toml)
for needed in "${MUST_READ[@]}"; do
  found=0
  for path in "${scan[@]}"; do
    if [ "$path" = "$needed" ]; then found=1; break; fi
  done
  if [ $found = 0 ]; then
    echo "FAIL: the canary does not read $needed; is it tracked, and is the scan set whole?" >&2
    failed=1
  fi
done
if [ ${#scan[@]} -eq 0 ]; then
  echo "FAIL: nothing to scan" >&2
  exit 1
fi
# The files that the three exclusions leave out, counted: 6 lockfiles, one of them the
# lockfile of the fuzz package, and 2 synthetic inputs on 2026-11-21. An exclusion that is added to the list above leaves out more files than this and
# fails, unless this number is raised in the same change, where a reviewer sees both. A new
# synthetic input or lockfile raises the number by one.
LEFT_OUT_MAX=8
tracked=$(git ls-files -z | tr -cd '\0' | wc -c)
left_out=$((tracked - ${#scan[@]}))
if [ "$left_out" -gt "$LEFT_OUT_MAX" ]; then
  echo "FAIL: the canary leaves out $left_out tracked files, more than $LEFT_OUT_MAX; a new exclusion needs LEFT_OUT_MAX raised in the same change" >&2
  failed=1
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
    # Only grep's own messages: the output also holds the lines that matched in other files.
    *) echo "FAIL: grep could not scan every file (status $status):" >&2; printf '%s\n' "$hits" | grep -a '^grep: ' >&2; return 2 ;;
  esac
}

if [ "$mode" = shape ]; then
  # Phone: 10 US digits with separators or parentheses. Email. Masked card: stars or x's then four
  # digits, or "ending in NNNN". Street address: a number, a word or two, and a street suffix.
  # Home directory: /home/NAME or /Users/NAME, which holds the account name of a person; write
  # ~/ in a rule or a document instead. The two roots are matched in this spelling only: the
  # path of the Gmail API in the source starts with the same word in lower case.
  patterns=(
    '\(?[2-9][0-9]{2}\)?[-. ][0-9]{3}[-. ][0-9]{4}'
    '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
    '([*xX]{2,}|ending in )[0-9]{4}'
    '\b[0-9]{1,5} ([A-Z][a-z]+ ){1,3}(Street|St\.|Avenue|Ave\.|Road|Rd\.|Boulevard|Blvd\.|Place|Pl\.|Court|Ct\.|Drive|Dr\.|Lane|Ln\.|Parkway|Pkwy\.)(\b|$)'
    '/(home|Users)/[A-Za-z0-9._+@-]+'
  )
  # Shapes that cannot be a person's data and are allowed to appear in tests and documents:
  #   - addresses in the RFC 2606 reserved domains (.example, .test, .invalid, example.com/net/org);
  #   - the platforms' own sender addresses (freshdirect.com and the six restaurant platforms of
  #     ADR 0007), which are configuration; the domain must end the match, so a longer domain
  #     that only starts with one of them is still a finding;
  #   - phone numbers in 555-0100 to 555-0199, the range reserved for fiction;
  #   - the test card 4242 4242 4242 4242, whose last four the synthetic data uses: only as a whole
  #     card-shaped match (the hit is path:line:match, so the mask follows a colon and 4242 ends it);
  #   - invented street names ("Imaginary Street", "Example Street") used by the synthetic data;
  #   - a home directory of a made-up account (x, you, user, name, alice) or of the account of
  #     a CI runner; the name must end the match.
  # Anything else of these shapes is a finding, including a real street, a real number, a
  # personal mail domain.
  reserved='@([A-Za-z0-9.-]+\.)?(example|test|invalid|localhost)([^A-Za-z0-9]|$)|@example\.(com|net|org)([^A-Za-z0-9]|$)|@([A-Za-z0-9.-]+\.)?(freshdirect|seamless|grubhub|toasttab|squareup|clover|doordash)\.com$|555[-. ]?01[0-9]{2}|:([*xX]{2,}|ending in )4242$|\b(Imaginary|Example|Fictional) (Street|Avenue|Plaza|Road|Place|Lane|Drive)\b|:/(home|Users)/(x|you|user|name|alice|runner)$'
  # Each match is judged on its own (grep -o), so a real value cannot hide behind a fictional one
  # on the same line, and only file and line are printed: a matched value may be the real thing.
  for pattern in "${patterns[@]}"; do
    # -a: a file with a byte that is no text, such as a seed of a fuzz target that is an
    # archive, is read as text. Without it grep says only that the file matches, and a
    # reserved form in it could not be told from a real value.
    if hits=$(scan_for -aEo "$pattern"); then
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
    # -a: a file with a NUL or a byte that is not UTF-8 is scanned as text. Without it grep
    # prints "binary file matches" in place of the line, and the filter below would drop it.
    if hits=$(scan_for -aFi "$term"); then
      # The owner segment of a GitHub address is a public account name; a family name inside it
      # (the project's own repository links) is not a leak. The segment is letters, digits and
      # hyphens only, as GitHub admits; anything else on the line still counts.
      if ! stripped=$(printf '%s\n' "$hits" | sed -E 's#github\.com/[A-Za-z0-9-]+##g'); then
        echo "FAIL: could not judge the hits of a term" >&2
        failed=1
        continue
      fi
      remaining=$(printf '%s\n' "$stripped" | grep -aFi -- "$term")
      case $? in
        0)
          # The term itself is personal data: report where, not what.
          echo "FAIL: a configured personal-data term appears in:" >&2
          printf '%s\n' "$remaining" | cut -d: -f1,2 >&2
          failed=1
          ;;
        1) ;;
        *)
          echo "FAIL: could not judge the hits of a term" >&2
          failed=1
          ;;
      esac
    elif [ $? -eq 2 ]; then
      failed=1
    fi
  done < "$terms_file"
fi

if [ $failed = 0 ]; then echo "PASS: privacy canary ($mode mode)"; fi
exit $failed
