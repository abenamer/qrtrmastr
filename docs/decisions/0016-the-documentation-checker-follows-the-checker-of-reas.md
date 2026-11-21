---
type: Architecture Decision
title: The documentation checker follows the checker of REAS and reads Markdown with a parser
description: Why the checker of the bundle was replaced by an adaptation of the checker of the REAS repository, why it now depends on markdown-it-py, which gates it has, and what it cannot see.
tags: [decision, documentation, dependencies, tooling]
generated: { by: "process:claude-code", at: "2026-11-21T20:00:00Z" }
updated: 2026-11-21
sources:
  - id: reas-checker
    resource: https://github.com/abenamer/reas/blob/a550838797e1e13dd00bde238b932ac1a9990157/scripts/check_docs.py
    title: The checker of the REAS repository at the vendored commit
  - id: checker
    resource: ../../scripts/check_docs.py
    title: The checker of this project
---

# ADR 0016 — The documentation checker follows the checker of REAS and reads Markdown with a parser

## Context

REAS-DOC-002 asks the checker for five things that the checker of this project did not do: an `updated` date on each concept, a report of a stale concept, reachability from the index, valid heading fragments, and (REAS-DOC-003) a failure on a retired term. REAS-DOC-006 asks that each gate has a floor and a planted violation. The checker of the project was 139 lines that found links with one regular expression. It could not find the anchor of a heading, a link in HTML or the text outside code.

The REAS repository has a checker that does these things for its own bundle. It reads Markdown with `markdown-it-py`, a CommonMark parser. Its license is MIT, and the vendored copy of the standard does not include it.

## Decision

* The checker of the project shall be an adaptation of the REAS checker at commit `a550838`, the commit of the vendored standard. The file says where it comes from.
* The bundle is `docs/`. The checker shall read the vendored tree under `standards/` for its links, its anchors and its rule declarations only, and shall not search it for a retired term: the tree is quoted material.
* The checker shall not check the shape of a rule (Intent, Enforcement and the other sections). This project declares no rule; the REAS repository checks its own.
* When a concept has no `updated` date, the checker shall fail. The REAS checker accepts a concept without one.
* When a gate found nothing to read (no concept, no local link, no fragment, no retired term, no rule declaration), the checker shall fail.
* The checker shall read no symbolic link, and shall print a control character of a document, and the command form `##[` of a CI runner, as an escape.
* `requirements-docs.txt` shall pin `markdown-it-py`, its one requirement `mdurl`, and `PyYAML` to exact versions. Dependabot proposes a newer one once a month, and the bump follows [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).
* The pinned version of `markdown-it-py` is 4.0.0, the version that the tests of the checker ran with on 2026-11-21. The REAS repository pins 3.0.0.

## Consequences

* The `docs` job and the Stop hook need `pip install -r requirements-docs.txt` once more. The job installs from the file on each run.
* Two Python packages more run in the `docs` job and on the development machine: `markdown-it-py` and `mdurl`. Neither is part of `qm`, of its image or of its build. The security review of 2026-11-21 found no advisory for the pinned versions.
* The install does not check a hash, so the package index is trusted for the content of the three versions.
* A change of the REAS checker does not reach this project by itself. When the vendored standard moves to a newer commit, read the difference of its checker and take what applies.
* The checker searches a retired term as plain text. [Develop and run](/runbooks/develop.md) lists what it cannot see.

## Alternatives

* Keep the old checker and add the gates with regular expressions. Rejected: a heading fragment and the text outside code need the structure of the document, and a second partial parser of Markdown is one more thing to get wrong.
* Run the REAS checker unchanged against this repository. Rejected: its bundle is `standard/`, it requires the shape of a rule in each concept of its type, and it would search the vendored standard for the retired terms of this project.
* Pin `markdown-it-py` 3.0.0, as REAS does. Not chosen: 4.0.0 is the version that was on the machine and that the tests of the checker ran with; 3.0.0 was not run here.
