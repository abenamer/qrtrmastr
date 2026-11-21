#!/usr/bin/env python3
"""Check the project's documentation bundle. This is not an STE language checker.

Adapted from the checker of the REAS standard (https://github.com/abenamer/reas,
`scripts/check_docs.py` at commit a550838, MIT). The bundle is `docs/`. The vendored copy of
the standard under `standards/` is scanned for its links and anchors only: it is quoted
material, and its own repository checks its rules.

Read each file with a Markdown extension, and no symbolic link. Parse CommonMark links,
images, headings, and HTML href/src/id attributes. Resolve heading fragments with
GitHub-style slugs. Do not fetch external URLs. Check that each concept has a
type and an `updated` date and can be reached from the index, that no document outside the
exempt list uses a retired term from the terminology table, that each file in the index of
git has an authority class in `docs/reference/files.md` (REAS-DOC-002), and that each gate
looked at something (REAS-DOC-006). Report a concept whose `updated` date is older than the threshold
for its type as a warning, not an error.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass, field
from datetime import date, datetime
import fnmatch
from html.parser import HTMLParser
import os
from pathlib import Path
import re
import subprocess
import unicodedata
from urllib.parse import unquote, urlsplit

from markdown_it import MarkdownIt
import yaml

BUNDLE = "docs"
VENDORED = "standards"
RULE = re.compile(r"^(REAS-[A-Z]+-\d{3})(?=$|[\s:])")
TERMINOLOGY = f"{BUNDLE}/terminology.md"
RETIRED_TERMS_HEADING = "## Retired terms"
# The page that gives each committed file its authority class (REAS-DOC-002).
FILES = f"{BUNDLE}/reference/files.md"
FILES_HEADING = "## The class of each file"
AUTHORITY_CLASSES = ("Authored", "Derived, committed", "Evidence", "Companion")
CODE_SPAN = re.compile(r"`[^`]+`")
# Documents that name a retired term on purpose: the table that retires it, the two records
# of what was done on a past day, and the language-review record.
RETIRED_TERM_EXEMPT = frozenset({
    TERMINOLOGY, f"{BUNDLE}/log.md", "CHANGELOG.md", f"{BUNDLE}/ste-review.md",
})
# Freshness threshold in days by concept type (REAS-DOC-002). Other types use
# FRESHNESS_DEFAULT_DAYS. A stale concept is a warning, not an error.
FRESHNESS_DAYS = {"Audit Report": 90, "Review Procedure": 180, "Technical Reference": 180}
FRESHNESS_DEFAULT_DAYS = 365
ACTOR = re.compile(r"^(?:human:[^\s]+|process:[^\s]+|[^\s/]+/[^\s/]+)$")
# Directories at the top of the repository that hold no document of the project. A directory
# of the same name further down is read like any other.
SKIPPED_DIRECTORIES = frozenset({".git", "target", ".venv", "node_modules"})
MARKDOWN_SUFFIXES = frozenset({".md", ".markdown"})
PRINTED_AS_IT_IS = frozenset(" ")
MARKDOWN = MarkdownIt("commonmark")


class UniqueKeyLoader(yaml.SafeLoader):
    """Reject duplicate YAML keys instead of silently using the last value."""


def unique_mapping(loader, node, deep=False):
    loader.flatten_mapping(node)
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in result:
            raise yaml.constructor.ConstructorError(
                None, None, f"Duplicate YAML key: {key}", key_node.start_mark
            )
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
)


def split_frontmatter(text: str):
    if not text.startswith("---\n"):
        return None, text, None
    match = re.search(r"\n---[ \t]*(?:\n|$)", text[4:])
    if match is None:
        return None, text, "Unclosed YAML frontmatter"
    raw = text[4:][:match.start()]
    body = text[4:][match.end():]
    try:
        metadata = yaml.load(raw, Loader=UniqueKeyLoader)
    except (yaml.YAMLError, TypeError, ValueError, RecursionError) as exc:
        return None, text, f"Invalid YAML: {exc}"
    if not isinstance(metadata, dict):
        return None, text, "Frontmatter must be a mapping"
    return metadata, body, None


class HTMLReferences(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []
        self.anchors = set()
        self.unreadable = 0

    def read(self, text: str):
        """Feed a piece of HTML. Some Python versions stop on a malformed declaration."""
        try:
            self.feed(text)
        except (AssertionError, ValueError):
            self.unreadable += 1
            self.reset()

    def handle_starttag(self, tag, attrs):
        for name, value in attrs:
            if value is None:
                continue
            if name in {"href", "src"}:
                self.links.append(value)
            if name == "id" or (tag == "a" and name == "name"):
                self.anchors.add(value)

    handle_startendtag = handle_starttag


@dataclass
class Document:
    headings: list[tuple[int, str]]
    links: list[str]
    anchors: set[str]
    plain: str
    unreadable_html: int = 0


@dataclass
class Counts:
    """What each gate looked at. A gate that looked at nothing fails (REAS-DOC-006)."""

    concepts: int = 0
    links: int = 0
    fragments: int = 0
    retired_terms: int = 0
    documents_read_for_terms: int = 0
    rules: int = 0
    files_with_a_class: int = 0

    def floor_errors(self) -> list[str]:
        errors = []
        if self.concepts == 0:
            errors.append(f"Floor: no concept was found in {BUNDLE}/")
        if self.links == 0:
            errors.append("Floor: no local link was checked")
        if self.fragments == 0:
            errors.append("Floor: no local fragment was checked")
        if self.retired_terms == 0 or self.documents_read_for_terms == 0:
            errors.append("Floor: no document was read for a retired term")
        if self.rules == 0:
            errors.append(f"Floor: no rule declaration was found; is {VENDORED}/ present?")
        if self.files_with_a_class == 0:
            errors.append("Floor: no file was given an authority class")
        return errors

    def summary(self) -> str:
        return (
            f"{self.concepts} concepts, {self.links} local links, {self.fragments} fragments, "
            f"{self.retired_terms} retired terms in {self.documents_read_for_terms} documents, "
            f"{self.rules} rule declarations, {self.files_with_a_class} files with an authority class"
        )


@dataclass
class Result:
    errors: list[str] = field(default_factory=list)
    counts: Counts = field(default_factory=Counts)


def parse_document(body: str, meta: dict | None = None) -> Document:
    """Read the links, the anchors and the prose of a document.

    The prose is what a retired term is searched in: the text outside code, the title of a
    link, a block or a tag of HTML as it is written, and the title and the description of
    the front matter.
    """
    tokens = MARKDOWN.parse(body)
    headings = []
    links = []
    anchors = set()
    used_slugs = set()
    html = HTMLReferences()
    plain: list[str] = []
    if isinstance(meta, dict):
        plain.extend(str(meta[key]) for key in ("title", "description") if key in meta)

    def read_inline(children):
        for child in children or []:
            if child.type == "link_open":
                links.append(child.attrGet("href") or "")
                plain.append(child.attrGet("title") or "")
            elif child.type == "image":
                links.append(child.attrGet("src") or "")
                plain.append(child.attrGet("title") or "")
            elif child.type == "html_inline":
                html.read(child.content)
                plain.append(child.content)
            read_inline(child.children)

    def inline_text(children) -> str:
        parts = []
        for child in children or []:
            if child.type == "text":
                parts.append(child.content)
            elif child.type in {"softbreak", "hardbreak"}:
                # The end of a line stands between two words.
                parts.append(" ")
            parts.append(inline_text(child.children))
        return "".join(parts)

    for index, token in enumerate(tokens):
        if token.type == "heading_open":
            inline = tokens[index + 1]
            text = "".join(
                child.content for child in inline.children or []
                if child.type in {"text", "code_inline", "image"}
            )
            headings.append((int(token.tag[1:]), text))
            base = "".join(
                char for char in text.lower()
                if char in "-_ " or unicodedata.category(char)[0] not in "PSZC"
            ).replace(" ", "-")
            slug = base
            suffix = 0
            while slug in used_slugs:
                suffix += 1
                slug = f"{base}-{suffix}"
            used_slugs.add(slug)
            anchors.add(slug)
        elif token.type == "html_block":
            html.read(token.content)
            plain.append(token.content)
        if token.type == "inline":
            read_inline(token.children)
            plain.append(inline_text(token.children))
    links.extend(html.links)
    anchors.update(html.anchors)
    return Document(headings, links, anchors, re.sub(r"\s+", " ", " ".join(plain)), html.unreadable)


def retired_terms(text: str) -> list[tuple[str, str]] | None:
    """Read the Retired terms table from the terminology concept.

    Return None when the section is absent, else the (term, replacement) rows.
    """
    lines = text.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == RETIRED_TERMS_HEADING)
    except StopIteration:
        return None
    rows = []
    table_lines = []
    for line in lines[start + 1:]:
        if line.startswith("#"):
            break
        if line.lstrip().startswith("|"):
            table_lines.append(line)
    for line in table_lines[2:]:
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and cells[0]:
            rows.append((cells[0], cells[1]))
    return rows


def authority_rows(text: str) -> list[tuple[str, str, str]] | str | None:
    """Read the table of authority classes: (path, class, how the file is written) rows.

    Return None when the section is absent, and a text that says what is wrong when the
    section is not one plain table: the reader takes each line that starts with `|`, so a
    row in a comment or in a code block would count as a row.
    """
    lines = text.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == FILES_HEADING)
    except StopIteration:
        return None
    table_lines = []
    for line in lines[start + 1:]:
        if line.startswith("#"):
            break
        if "<!--" in line or line.lstrip().startswith(("```", "~~~")):
            return "The section must hold no comment and no code block"
        if line.lstrip().startswith("|"):
            if not line.startswith("|"):
                return "A line of the table must start at the margin"
            table_lines.append(line)
    if not table_lines:
        return []
    header = [cell.strip() for cell in table_lines[0].strip().strip("|").split("|")]
    if header[:2] != ["Path", "Class"] or len(header) != 3:
        return "The table must start with the header row: Path, Class and one more column"
    rows = []
    for line in table_lines[2:]:
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        cells += [""] * (3 - len(cells))
        rows.append((cells[0], cells[1], " | ".join(cells[2:])))
    return rows


def path_matches(pattern: str, path: str) -> bool:
    """A pattern is one file, a directory with a slash at its end, or a name with `*`.

    `*` stands for any text, a slash too. No other character is special; `authority_errors`
    refuses a pattern with `?` or `[`, which the matcher would read as wildcards.
    """
    if pattern.endswith("/"):
        return path.startswith(pattern)
    if "*" in pattern:
        return fnmatch.fnmatchcase(path, pattern)
    return path == pattern


def committed_files(root: Path) -> list[str] | None:
    """The entries in the index of git: each committed file, and a new file once it is staged.

    A directory that is not a repository, such as the fixture of a test, gives every file
    below it. Return None when git cannot list the files of a repository.
    """
    if (root / ".git").exists():
        try:
            listed = subprocess.run(
                ["git", "-C", str(root), "ls-files", "-z", "--cached"],
                capture_output=True,
            )
        except OSError:
            return None
        if listed.returncode != 0:
            return None
        # Every entry counts, also a submodule and a file that is not in the working tree.
        return sorted(
            {name for name in listed.stdout.decode("utf-8", "surrogateescape").split("\0") if name}
        )
    files = []
    for directory, directories, names in os.walk(root):
        # As for the Markdown files: a build or tool directory is left out at the top only.
        at_the_top = Path(directory) == root
        directories[:] = sorted(
            d for d in directories if not (at_the_top and d in SKIPPED_DIRECTORIES)
        )
        for name in names:
            files.append((Path(directory) / name).relative_to(root).as_posix())
    return sorted(files)


def authority_errors(rows: list[tuple[str, str, str]], files: list[str]) -> tuple[list[str], int]:
    """Give each file the class of the first row that matches it (REAS-DOC-002).

    Return the errors and the number of files that got a class.
    """
    errors = []
    patterns = []
    for cell, authority, how in rows:
        if not re.fullmatch(r"`[^`]+`", cell):
            errors.append(f"{FILES}: '{cell}': The path of a row must be one code span")
            continue
        pattern = cell.strip("`")
        if not pattern.strip("*/") or "?" in pattern or "[" in pattern:
            # A row for everything would give a class to a file that nobody looked at.
            errors.append(
                f"{FILES}: '{pattern}': A path must name a file, a directory or a part of a "
                "name; `*` is the one wildcard"
            )
            continue
        if authority not in AUTHORITY_CLASSES:
            errors.append(f"{FILES}: '{pattern}': '{authority}' is not an authority class")
        elif authority == "Derived, committed" and not CODE_SPAN.search(how):
            errors.append(
                f"{FILES}: '{pattern}': A derived file must name, in a code span, the command "
                "or the record that says how it is written"
            )
        elif authority == "Companion" and not how:
            errors.append(f"{FILES}: '{pattern}': A companion file must name what it summarizes")
        patterns.append(pattern)
    first_matches = {pattern: 0 for pattern in patterns}
    with_a_class = 0
    for name in files:
        for pattern in patterns:
            if path_matches(pattern, name):
                first_matches[pattern] += 1
                with_a_class += 1
                break
        else:
            errors.append(f"{name}: No authority class; add a row to {FILES}")
    for pattern, count in first_matches.items():
        if count == 0:
            errors.append(
                f"{FILES}: '{pattern}': No file has this row as its first match; "
                "remove the row or move it above the row that takes its files"
            )
    return errors, with_a_class


def parse_updated(value) -> date | None:
    """A day as YAML reads `2026-11-21`, or the same text in quotes. A time is not a day."""
    if isinstance(value, datetime):
        return None
    if isinstance(value, date):
        return value
    if isinstance(value, str) and re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        try:
            return date.fromisoformat(value)
        except ValueError:
            return None
    return None


def markdown_files(root: Path) -> list[Path]:
    """Every file with a Markdown extension in any letter case, outside the skipped directories."""
    found = []
    for path in root.rglob("*"):
        if path.suffix.lower() not in MARKDOWN_SUFFIXES:
            continue
        if path.relative_to(root).parts[0] in SKIPPED_DIRECTORIES:
            continue
        if path.is_symlink() or path.is_file():
            found.append(path)
    return sorted(found)


def printable(text: str) -> str:
    """A message for the terminal or the CI log: a control character is written as an escape.

    A link or a heading of a document goes into a message. A line break in one would start a
    line of its own, which a CI system can read as a command.
    """
    escaped = "".join(
        char if char.isprintable() or char in PRINTED_AS_IT_IS else f"\\u{ord(char):04x}"
        for char in text
    )
    # The older command form of a CI runner is `##[name]` at any place of a line.
    return escaped.replace("##[", "#\\u0023[")


def freshness_warnings(root: Path, today: date | None = None) -> list[str]:
    """Report concepts whose `updated` date is older than the threshold for their type."""
    today = today or date.today()
    root = root.resolve()
    bundle = root / BUNDLE
    warnings = []
    if not bundle.is_dir():
        return warnings
    for path in markdown_files(root):
        if not path.is_relative_to(bundle) or path.is_symlink() or path.suffix != ".md":
            continue
        if path.name == "index.md" or path == bundle / "log.md":
            continue
        try:
            meta, _, _ = split_frontmatter(path.read_text(encoding="utf-8"))
        except (UnicodeError, OSError):
            continue
        if not isinstance(meta, dict) or "updated" not in meta:
            continue
        updated = parse_updated(meta["updated"])
        if updated is None or updated > today:
            continue
        kind = meta.get("type")
        limit = FRESHNESS_DAYS.get(kind, FRESHNESS_DEFAULT_DAYS) if isinstance(kind, str) else FRESHNESS_DEFAULT_DAYS
        age = (today - updated).days
        if age > limit:
            rel = path.relative_to(root).as_posix()
            warnings.append(
                f"{rel}: updated {updated} is {age} days old; the limit for {meta.get('type')} is {limit}"
            )
    return warnings


def valid_timestamp(value) -> bool:
    if isinstance(value, str):
        if "T" not in value:
            return False
        try:
            value = datetime.fromisoformat(value.replace("Z", "+00:00"))
        except ValueError:
            return False
    return isinstance(value, datetime) and value.utcoffset() is not None


def metadata_errors(meta: dict) -> list[str]:
    errors = []
    events = []
    if "generated" in meta:
        events.append(("generated", meta["generated"]))
    verified = meta.get("verified", [])
    if isinstance(verified, dict):
        verified = [verified]
    if not isinstance(verified, list):
        errors.append("verified must be a mapping or a list")
    else:
        events.extend(("verified", event) for event in verified)
    for name, event in events:
        if not isinstance(event, dict):
            errors.append(f"{name} must be a mapping")
            continue
        actor = event.get("by")
        if not isinstance(actor, str) or not ACTOR.fullmatch(actor):
            errors.append(f"Invalid {name}.by")
        if not valid_timestamp(event.get("at")):
            errors.append(f"Invalid {name}.at: use a datetime with a UTC offset")
    sources = meta.get("sources", [])
    if not isinstance(sources, list) or any(
        not isinstance(source, dict)
        or not isinstance(source.get("resource"), str)
        or not source["resource"].strip()
        for source in sources
    ):
        errors.append("Sources must include a resource")
    return errors


def check(root: Path, today: date | None = None) -> Result:
    """Run every gate. `Result.errors` is empty when the bundle passes."""
    today = today or date.today()
    root = root.resolve()
    bundle = root / BUNDLE
    result = Result()
    errors = result.errors
    counts = result.counts
    if not bundle.is_dir():
        errors.append(f"The {BUNDLE}/ OKF bundle is missing")
        return result
    documents = {}
    concepts = set()
    seen = {}
    graph = {}
    terminology_text = None
    for path in markdown_files(root):
        rel = path.relative_to(root).as_posix()
        if path.suffix != ".md":
            errors.append(f"{rel}: A Markdown file must have the extension .md")
            continue
        if path.is_symlink():
            # A link can point outside the repository; the checker reads no such file.
            errors.append(f"{rel}: A Markdown file must not be a symbolic link")
            continue
        try:
            content = path.read_text(encoding="utf-8")
        except (UnicodeError, OSError) as exc:
            errors.append(f"{rel}: Cannot read: {exc}")
            continue
        meta, body, err = split_frontmatter(content)
        if err:
            errors.append(f"{rel}: {err}")
        document = parse_document(body, meta)
        if document.unreadable_html:
            errors.append(f"{rel}: Cannot read a piece of HTML; its links and anchors are not checked")
        documents[path] = document
        graph[path] = set()
        if path.is_relative_to(bundle):
            if path.name == "index.md":
                if path.parent == bundle:
                    if meta is None or set(meta) != {"okf_version"} or str(meta.get("okf_version")) != "0.2":
                        errors.append(f"{rel}: Bundle index must contain only okf_version: '0.2'")
                elif meta is not None:
                    errors.append(f"{rel}: Nested index must not contain frontmatter")
            elif path == bundle / "log.md":
                if meta is not None:
                    errors.append(f"{rel}: Log must not contain frontmatter")
                for level, heading in document.headings:
                    if level == 2:
                        try:
                            if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", heading):
                                raise ValueError
                            date.fromisoformat(heading)
                        except ValueError:
                            errors.append(f"{rel}: Log date must use YYYY-MM-DD: {heading}")
            else:
                concepts.add(path)
                if not isinstance(meta, dict) or not isinstance(meta.get("type"), str) or not meta["type"].strip():
                    errors.append(f"{rel}: Concept requires a non-empty type")
                if meta:
                    errors.extend(f"{rel}: {error}" for error in metadata_errors(meta))
                    if "updated" not in meta:
                        errors.append(f"{rel}: Concept requires an updated date")
                    else:
                        updated = parse_updated(meta["updated"])
                        if updated is None:
                            errors.append(f"{rel}: Invalid updated: use a date as YYYY-MM-DD")
                        elif updated > today:
                            errors.append(f"{rel}: updated {updated} is in the future")
                if not body.strip():
                    errors.append(f"{rel}: Empty concept")
                if rel == TERMINOLOGY:
                    terminology_text = body
        for _, heading in document.headings:
            if not heading.startswith("REAS-"):
                continue
            match = RULE.match(heading)
            if match is None:
                errors.append(f"{rel}: Invalid rule declaration: {heading}")
                continue
            rule = match.group(1)
            if rule in seen:
                errors.append(f"{rel}: Duplicate {rule} also in {seen[rule]}")
            seen[rule] = rel
    counts.concepts = len(concepts)
    counts.rules = len(seen)
    for path, document in documents.items():
        rel = path.relative_to(root).as_posix()
        for uri in document.links:
            try:
                parsed = urlsplit(uri)
            except ValueError:
                errors.append(f"{rel}: Invalid link URI: {uri}")
                continue
            if parsed.scheme or parsed.netloc:
                continue
            counts.links += 1
            target = unquote(parsed.path)
            try:
                if not target:
                    actual = path
                elif target.startswith("/"):
                    base = bundle if path.is_relative_to(bundle) else root
                    actual = (base / target.lstrip("/")).resolve()
                else:
                    actual = (path.parent / target).resolve()
                if not actual.is_relative_to(root):
                    errors.append(f"{rel}: Link leaves repository: {uri}")
                    continue
                exists = actual.exists()
                is_dir = exists and actual.is_dir()
            except (ValueError, OSError):
                # A null byte or a name that the file system refuses.
                errors.append(f"{rel}: Broken local link: {uri}")
                continue
            # A file is no directory: a slash, a dot or a further segment after its name is broken.
            last = target.split("/")[-1] if target else None
            through_a_file = ".." in target and any(
                part.is_file() for part in (path.parent / target.lstrip("/")).parents
            )
            if not exists or through_a_file or (not is_dir and last in {"", "."}):
                errors.append(f"{rel}: Broken local link: {uri}")
                continue
            if is_dir:
                actual = next(
                    (candidate for name in ("index.md", "README.md")
                     if (candidate := actual / name) in documents), actual
                )
            if actual in documents:
                graph[path].add(actual)
            if parsed.fragment:
                counts.fragments += 1
                fragment = unquote(parsed.fragment)
                if actual not in documents or fragment not in documents[actual].anchors:
                    errors.append(f"{rel}: Broken local fragment: {uri}")
    index = bundle / "index.md"
    if not index.is_file():
        errors.append(f"{BUNDLE}/index.md: Bundle index is missing")
    else:
        reachable = set()
        pending = [index]
        while pending:
            path = pending.pop()
            if path in reachable or not path.is_relative_to(bundle):
                continue
            reachable.add(path)
            pending.extend(graph.get(path, set()) - reachable)
        for path in sorted(concepts - reachable):
            errors.append(
                f"{path.relative_to(root).as_posix()}: Concept is not reachable from {BUNDLE}/index.md"
            )
    if terminology_text is None:
        errors.append(f"{TERMINOLOGY}: The terminology concept is missing")
    else:
        retired = retired_terms(terminology_text)
        if retired is None:
            errors.append(f"{TERMINOLOGY}: Retired terms section is missing")
        elif not retired:
            errors.append(f"{TERMINOLOGY}: Retired terms table is empty")
        elif any(re.search(r"[`*_<>\[\]]", term) for term, _ in retired):
            # The term is searched as plain text, so a term in code or emphasis finds nothing.
            errors.append(f"{TERMINOLOGY}: A retired term must be plain text, with no markup")
        else:
            counts.retired_terms = len(retired)
            for path, document in documents.items():
                rel = path.relative_to(root).as_posix()
                if rel in RETIRED_TERM_EXEMPT or rel.startswith(f"{VENDORED}/"):
                    continue
                counts.documents_read_for_terms += 1
                plain = document.plain.lower()
                for term, replacement in retired:
                    if term.lower() in plain:
                        errors.append(f"{rel}: Retired term '{term}'; use {replacement}")
    files_page = root / FILES
    if not files_page.is_file() or files_page.is_symlink():
        errors.append(f"{FILES}: The page of the authority classes is missing")
    else:
        rows = authority_rows(files_page.read_text(encoding="utf-8", errors="replace"))
        files = committed_files(root)
        if rows is None:
            errors.append(f"{FILES}: The section '{FILES_HEADING}' is missing")
        elif isinstance(rows, str):
            errors.append(f"{FILES}: {rows}")
        elif not rows:
            errors.append(f"{FILES}: The table of the authority classes is empty")
        elif files is None:
            errors.append(f"{FILES}: git cannot list the files of the repository")
        else:
            found, counts.files_with_a_class = authority_errors(rows, files)
            errors.extend(found)
    errors.extend(counts.floor_errors())
    return result


def validate(root: Path, today: date | None = None) -> list[str]:
    return check(root, today).errors


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--strict-freshness", action="store_true",
                        help="treat a stale concept as an error instead of a warning")
    args = parser.parse_args(argv)
    result = check(args.root)
    errors = result.errors
    warnings = freshness_warnings(args.root)
    for warning in warnings:
        print("WARN:", printable(warning))
    if args.strict_freshness:
        errors.extend(warnings)
    for error in errors:
        print("ERROR:", printable(error))
    if errors:
        print(f"FAIL: {len(errors)} errors")
        return 1
    print("PASS: OKF metadata, updated dates, index reachability, retired terms, "
          "local links and fragments, authority classes")
    print(f"CHECKED: {result.counts.summary()}")
    print("NOT CHECKED: full OKF conformance, STE language, external URLs, technical accuracy")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
