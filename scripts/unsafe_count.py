#!/usr/bin/env python3
"""The count of unsafe items for each package that the counter names, and its baseline
(REAS-NATIVE-002).

    python3 scripts/unsafe_count.py --write BASELINE REPORT   write the baseline from a report
    python3 scripts/unsafe_count.py --check BASELINE REPORT   fail when the report is not the
                                                              baseline

REPORT is the JSON that `cargo geiger --output-format Json` prints. `scripts/check.sh unsafe`
runs the counter and calls this script; `scripts/check.sh unsafe write` writes the baseline.

The baseline is a derived file (docs/reference/files.md): do not edit it by hand. It has one
line for each package that the counter names: the name, the version, the unsafe items in the
files that the build uses, the unsafe items in the other files of the package, and whether the
package forbids unsafe code with an attribute in its source. An item is a function, an
expression, an implementation, a trait or a method that the counter marks as unsafe.

The check asks for the same lines, not for lines that are not larger. A count that is larger
needs a reason and a review, and a count that is smaller needs a new baseline, so that the
file is the record of the current count. The counter counts; it does not review, and it does
not read the code that a build script or a macro writes.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

KINDS = ("functions", "exprs", "item_impls", "item_traits", "methods")
HEADER = (
    "# The unsafe items of each package that the counter names (REAS-NATIVE-002).\n"
    "# Written by `scripts/check.sh unsafe write`. Do not edit by hand.\n"
    "# name\tversion\tin files that the build uses\tin other files\tforbids unsafe in its source\n"
)


class ReportError(Exception):
    """The report of the counter cannot be used."""


def total(counts: dict) -> int:
    """The unsafe items of one group of files, over the five kinds. A count is a whole number
    that is not negative: a text, a fraction or a negative number is no count."""
    numbers = [counts[kind]["unsafe_"] for kind in KINDS]
    if any(type(number) is not int or number < 0 for number in numbers):
        raise ReportError("a count of the report is not a whole number from 0 up")
    return sum(numbers)


def rows_of(report_text: str) -> dict[tuple[str, str], tuple[int, int, bool]]:
    """The count of each package of a report, by name and version."""
    try:
        report = json.loads(report_text)
        packages = report["packages"]
        rows = {}
        for entry in packages:
            identity = entry["package"]["id"]
            key = (str(identity["name"]), str(identity["version"]))
            unsafety = entry["unsafety"]
            if type(unsafety["forbids_unsafe"]) is not bool:
                raise ReportError("the report does not say yes or no for a package that forbids unsafe code")
            row = (total(unsafety["used"]), total(unsafety["unused"]), unsafety["forbids_unsafe"])
            if key in rows:
                raise ReportError(f"the report names {key[0]} {key[1]} twice")
            rows[key] = row
        without = report["packages_without_metrics"]
        if type(without) is not list:
            raise TypeError("packages_without_metrics is not a list")
    except (ValueError, KeyError, TypeError) as error:
        raise ReportError(f"the report is not the JSON of the counter: {error!r}") from error
    if without:
        raise ReportError(f"the counter has no count for {len(without)} packages")
    return rows


def baseline_text(rows: dict[tuple[str, str], tuple[int, int, bool]]) -> str:
    lines = [
        f"{name}\t{version}\t{used}\t{unused}\t{'yes' if forbids else 'no'}"
        for (name, version), (used, unused, forbids) in sorted(rows.items())
    ]
    return HEADER + "\n".join(lines) + "\n"


COUNT = re.compile(r"0|[1-9][0-9]*")


def rows_of_baseline(text: str) -> dict[tuple[str, str], tuple[int, int, bool]]:
    """The lines of a baseline. The text must be the text that this script writes for those
    lines: the header, one line for each package, in the order of the names, and nothing else."""
    rows = {}
    for number, line in enumerate(text.split("\n"), start=1):
        if not line or line.startswith("#"):
            continue
        cells = line.split("\t")
        if (len(cells) != 5 or cells[4] not in ("yes", "no")
                or not COUNT.fullmatch(cells[2]) or not COUNT.fullmatch(cells[3])):
            raise ReportError(f"line {number} of the baseline is not a line that this script writes")
        key = (cells[0], cells[1])
        if key in rows:
            raise ReportError(f"the baseline names {cells[0]} {cells[1]} twice")
        rows[key] = (int(cells[2]), int(cells[3]), cells[4] == "yes")
    if text != baseline_text(rows):
        raise ReportError("the baseline is not the text that this script writes; write it again with `scripts/check.sh unsafe write`")
    return rows


def differences(baseline: dict, report: dict) -> tuple[list[str], list[str]]:
    """What the report has that the baseline has not: the growths, and the other changes."""
    growths, others = [], []
    for key in sorted(set(baseline) | set(report)):
        name = f"{key[0]} {key[1]}"
        if key not in baseline:
            used, unused, _ = report[key]
            target = growths if used or unused else others
            target.append(f"{name}: not in the baseline; {used} unsafe items in the files of the build, {unused} in other files")
        elif key not in report:
            others.append(f"{name}: in the baseline and not in the build")
        elif baseline[key] != report[key]:
            was, now = baseline[key], report[key]
            target = growths if now[0] > was[0] or now[1] > was[1] or (was[2] and not now[2]) else others
            target.append(
                f"{name}: {now[0]} in the files of the build, was {was[0]}; {now[1]} in other files, was {was[1]}; "
                f"forbids unsafe: {'yes' if now[2] else 'no'}, was {'yes' if was[2] else 'no'}"
            )
    return growths, others


def own_package_errors(rows: dict, root: str) -> list[str]:
    """The package of the program has no unsafe item in a file of its build."""
    own = [(key, row) for key, row in rows.items() if key[0] == root]
    if len(own) != 1:
        return [f"the report names the package {root} {len(own)} times, not once"]
    (_, (used, _, _)) = own[0]
    if used:
        return [f"{root} has {used} unsafe items in the files of its build; the package forbids unsafe code"]
    return []


def main(argv: list[str]) -> int:
    options = {}
    args = []
    rest = iter(argv)
    for arg in rest:
        if arg in ("--root", "--floor") and arg not in options:
            options[arg] = next(rest, "")
        else:
            args.append(arg)
    # The root and the floor have no default: a check without them is a check of less.
    if (len(args) != 3 or args[0] not in ("--write", "--check") or not options.get("--root")
            or not re.fullmatch(r"[1-9][0-9]*", options.get("--floor", ""))):
        print("usage: scripts/unsafe_count.py {--write|--check} BASELINE REPORT --root NAME --floor COUNT")
        return 2
    mode, baseline_path, report_path = args[0], Path(args[1]), Path(args[2])
    floor = int(options["--floor"])
    try:
        rows = rows_of(report_path.read_text(encoding="utf-8"))
        # A count over fewer packages than the floor is a counter that did not read the build.
        if len(rows) < floor:
            raise ReportError(f"the report counts {len(rows)} packages; the floor is {floor}")
        errors = own_package_errors(rows, options["--root"])
        if errors:
            raise ReportError(errors[0])
        with_unsafe = sum(1 for used, _, _ in rows.values() if used)
        items = sum(used for used, _, _ in rows.values())
        summary = (
            f"{len(rows)} packages counted, {with_unsafe} with unsafe items in the files that the build uses, "
            f"{items} such items; {options['--root']} has none"
        )
        if mode == "--write":
            with open(baseline_path, "w", encoding="utf-8", newline="") as file:
                file.write(baseline_text(rows))
            print(f"wrote {baseline_path}: {summary}")
            return 0
        if not baseline_path.is_file():
            raise ReportError(f"{baseline_path} is not there; `scripts/check.sh unsafe write` writes it")
        # The text as it is in the file: no line end of another system is read as this one.
        with open(baseline_path, encoding="utf-8", newline="") as file:
            baseline = rows_of_baseline(file.read())
    except (OSError, UnicodeDecodeError, ReportError) as error:
        print(f"FAIL: {error}")
        return 1
    growths, others = differences(baseline, rows)
    for line in growths:
        print("ERROR: more unsafe code than the baseline:", line)
    for line in others:
        print("ERROR: not the baseline:", line)
    if growths:
        print(
            "FAIL: the unsafe surface grew. A growth needs a reason in docs/log.md and a review "
            "(REAS-NATIVE-002); read docs/reference/native-code.md, then write the baseline again "
            "with `scripts/check.sh unsafe write`"
        )
        return 1
    if others:
        print("FAIL: the baseline is not the current count; write it again with `scripts/check.sh unsafe write`")
        return 1
    print(f"PASS: the unsafe count is the baseline: {summary}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
