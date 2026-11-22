#!/usr/bin/env python3
"""List the dependencies that outside data reaches and that hold unsafe code (REAS-SEC-001),
and mark what Miri cannot run in each (REAS-NATIVE-004).

    python3 scripts/outside_data_paths.py        print the table of docs/reference/native-code.md

The script reads three things: the dependency graph that `cargo metadata` gives for Linux on
x86_64, the counts of `unsafe-baseline.tsv`, and the source of each package in the registry
directory of cargo, which a build leaves there. It is not a check and no group of
`scripts/check.sh` runs it: run it when the baseline changes, and put its table in the page.

How a package gets into the table:

* A reader is a direct dependency of the program that takes bytes or text of another party:
  a mail, an archive, an answer of the Gmail API, or a stored message or order that is read
  back. `READERS` names each one and what it reads.
* A package is on a path of outside data when a reader is that package or depends on it, over
  any number of steps, through a normal dependency. A build dependency and a procedural macro
  run on the build machine and not on a mail, so the walk does not go through them. This is
  the dependency graph, not a trace of the data: it names a package when the reader can call
  it, also when no byte of a mail gets there.
* The table has the package when the baseline counts an unsafe item in a file of its build.

What the marks mean. Miri runs Rust code. It does not run a call into a foreign function, an
intrinsic of one processor, or assembly. The script looks for the text of each of the three in
the Rust files of the package, outside its tests, benchmarks and examples, and counts the C
and assembly files that the package carries. A mark says that the text is there, not that the
build uses it or that a mail reaches it.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "unsafe-baseline.tsv"
PLATFORM = "x86_64-unknown-linux-gnu"

# Each direct dependency that takes data of another party, and what it takes.
READERS = {
    "mail-parser": "the bytes of a mail",
    "scraper": "the HTML of a mail",
    "ego-tree": "the HTML of a mail",
    "zip": "a Takeout archive",
    "regex": "the text of a mail",
    "unicode-normalization": "the text of a mail",
    "serde_json": "an answer of the Gmail API",
    "serde": "an answer of the Gmail API",
    "base64": "an answer of the Gmail API",
    "ureq": "the connection to the Gmail API",
    "url": "the address that the sign-in to Google gives back",
    "sqlx": "a stored message or order",
    "jiff-sqlx": "a stored message or order",
    "jiff": "a date of a mail",
    "rust_decimal": "an amount of a receipt",
    "sha2": "the bytes of a mail",
    "hex": "the hash of a mail",
    "strsim": "a product name",
    "tokio": "a mail on its way from the source to the import",
    "secrecy": "a token of an answer of Google",
    "anyhow": "the error of an import, with the text of the library that failed",
    "tracing": "a log line of a library that reads a mail or an answer",
    "tracing-subscriber": "a log line of a library that reads a mail or an answer",
}
# The direct dependencies that take no data of another party. A direct dependency that is in
# neither list stops the script: somebody must decide where it belongs.
NOT_READERS = {"clap", "directories", "dotenvy", "figment", "getrandom", "thiserror"}

MARKS = {
    "foreign function": re.compile(r'extern\s+"(?:C|C-unwind|system)"|#\[link\('),
    "processor intrinsic": re.compile(r"\b(?:core|std)::arch\b"),
    "assembly": re.compile(r"\b(?:global_)?asm!\s*[({]"),
}
NOT_THE_LIBRARY = ("tests", "benches", "examples", "fuzz")
NATIVE_SOURCE = (".c", ".cc", ".cpp", ".S", ".s", ".asm")


def baseline() -> dict[tuple[str, str], int]:
    counts = {}
    for line in BASELINE.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            name, version, used, _unused, _forbids = line.split("\t")
            counts[(name, version)] = int(used)
    return counts


def marks_of(directory: str) -> list[str]:
    """What Miri cannot run, by the text of the package's own source."""
    found = {name: 0 for name in MARKS}
    native = 0
    for path, folders, files in os.walk(directory):
        folders[:] = [f for f in folders if not (path == directory and f in NOT_THE_LIBRARY)]
        for file in files:
            if file.endswith(NATIVE_SOURCE):
                native += 1
            if file.endswith(".rs"):
                text = Path(path, file).read_text(encoding="utf-8", errors="replace")
                for name, pattern in MARKS.items():
                    if pattern.search(text):
                        found[name] += 1
    out = [f"{name} ({count} files)" for name, count in found.items() if count]
    if native:
        out.append(f"C or assembly sources ({native} files)")
    return out


def main() -> int:
    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", PLATFORM],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}

    def is_macro(identity: str) -> bool:
        return any("proc-macro" in target["kind"] for target in packages[identity]["targets"])

    def normal(identity: str) -> list[str]:
        return [dep["pkg"] for dep in nodes[identity]["deps"]
                if any(kind["kind"] is None for kind in dep["dep_kinds"]) and not is_macro(dep["pkg"])]

    direct = {packages[identity]["name"]: identity for identity in normal(metadata["resolve"]["root"])}
    undecided = sorted(set(direct) - set(READERS) - NOT_READERS)
    if undecided or sorted(set(READERS) - set(direct)):
        print("FAIL: READERS and NOT_READERS do not name each direct dependency once:",
              undecided, sorted(set(READERS) - set(direct)), file=sys.stderr)
        return 1
    reached: dict[str, set[str]] = {}
    for reader in READERS:
        stack = [direct[reader]]
        seen = set()
        while stack:
            identity = stack.pop()
            if identity not in seen:
                seen.add(identity)
                reached.setdefault(identity, set()).add(reader)
                stack.extend(normal(identity))
    counts = baseline()
    rows = []
    for identity, readers in reached.items():
        package = packages[identity]
        used = counts.get((package["name"], package["version"]), 0)
        if used:
            marks = marks_of(os.path.dirname(package["manifest_path"]))
            rows.append((package["name"], package["version"], used, sorted(readers), marks))
    rows.sort()
    blind = sum(1 for row in rows if row[4])
    # The graph also names a dependency of a feature that is off. The counter names a package
    # that a build can use; the second number is the packages of the graph that it names.
    named = sum(1 for identity in reached
                if (packages[identity]["name"], packages[identity]["version"]) in counts)
    print(f"<!-- {len(rows)} packages, {sum(row[2] for row in rows)} unsafe items, {blind} packages with a mark; "
          f"{len(reached)} packages on a path of outside data in the graph, {named} of them named by the counter; "
          f"{len(READERS)} readers -->")
    print("| Package | Version | Unsafe items | Reached from | What Miri cannot run |")
    print("| --- | --- | --- | --- | --- |")
    for name, version, used, readers, marks in rows:
        print(f"| `{name}` | {version} | {used} | {', '.join(f'`{r}`' for r in readers)} | {'; '.join(marks) or '—'} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
