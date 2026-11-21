---
type: Engineering Standard
title: Documentation and knowledge structure
description: ASD-STE100 language review, the REAS OKF profile, rule writing, and checks over rules.
tags: [documentation, asd-ste100, okf]
updated: 2026-11-20
sources:
  - id: ste
    resource: https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf
    title: ASD-STE100 Issue 9
  - id: okf
    resource: https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md
    title: OKF v0.2 specification
---

# Documentation and knowledge structure

<a id="reas-doc-001"></a>
## REAS-DOC-001: Target ASD-STE100

### Intent

A procedure that one reader can misread is a procedure that will be misread.
Controlled language removes the second reading.

### Enforcement

Required.

### Scope

Applies to procedures, reference text, and rule text in this repository.
Does not apply to historical changelog entries, license text, or quoted material.

### Rules

Write procedures and reference text with the same terms and direct sentences.
Give one instruction in each procedural sentence.
Put conditions before the related instruction.
Keep explanations separate from instructions.
Use the official Issue 9 rules and dictionary for the language review.
Record its scope, findings, exceptions, and result in the [STE review record](ste-review.md).

Do not report full compliance from sentence length or a word list alone.
Do not change a code identifier to satisfy a language preference.
A technical correction and a language correction are separate review steps.

### Verification

- Record: the review record names the documents reviewed, the reference issue, and the result.
- Review question: which sentence in this change gives more than one instruction?

<a id="reas-doc-002"></a>
## REAS-DOC-002: Keep an OKF knowledge bundle

### Intent

A reader finds every concept from one index, and every metadata claim has evidence behind it.
A reader also knows which files are authored, which are generated, and when each page was last verified.

### Enforcement

Required.

### Scope

Applies to every Markdown document under `standard/` and to every committed file the bundle describes.
Does not apply to code comments.

### Rules

Use `standard/` as the knowledge bundle.
Give each concept valid YAML frontmatter with a non-empty `type` string.
Keep `index.md` at the bundle root with `okf_version: "0.2"`.
Make each concept reachable from that index through local Markdown links.
Use `index.md` and `log.md` only for their reserved purposes.

In bundle documents, resolve a path that starts with `/` against the bundle root.
In repository documents outside the bundle, resolve that path against the repository root.
Keep local link paths and heading fragments valid.
Attribute external sources.
For `generated` and `verified` events, include an actor in `by` and a timestamp with a UTC offset in `at`.
Make sure that the evidence supports each metadata claim.

Give each concept an `updated` date in `YYYY-MM-DD` form.
Set it to the day the content was last verified against reality, not the day of a typo fix.
The checker reports a concept whose `updated` date is older than the threshold for its type: 90 days for an audit report, 180 days for a technical reference or review procedure, 365 days for other concepts.
A stale concept is triaged: verify it and move the date, or retire it.

Give each committed file one authority class, and treat it accordingly:

| Class | Meaning | Examples |
| --- | --- | --- |
| Authored | A person or agent writes it; it is the source of truth. | Rules, templates, source code, `Cargo.toml` |
| Derived, committed | A command writes it from an authored source; a hand edit is lost on the next run. The file or its directory names the command. | `Cargo.lock` from `cargo`, `.sqlx/` from `cargo sqlx prepare`, seed corpus files |
| Evidence | A person records the result of a run; it describes a revision and does not change the code. | `standard/self-audit.md`, `standard/ste-review.md` |
| Companion | A reader-facing summary of authored content; it carries no rule of its own. | `README.md`, `examples/README.md` |

Edit a derived file only through its command.
A companion file points at the authored file it summarizes.

### Verification

- CI: `scripts/check_docs.py` checks frontmatter, index reachability, links, fragments, `updated` dates, and reports stale concepts.
- Review question: which class is each committed file in this change, and was a derived file edited by hand?

<a id="reas-doc-003"></a>
## REAS-DOC-003: Use the same technical terms

### Intent

One concept with two names reads as two concepts.
An example is an instruction: a reader copies what it shows, including a term or an operation the rules forbid.

### Enforcement

Required.

### Scope

Applies to rule text, terminology, templates, agent instructions, and code examples in this repository.
Does not apply to quoted material or to the retired-terms table itself.

### Rules

Use the [terminology table](terminology.md) for terms with a Rust-specific or REAS-specific meaning.
Add a term when its meaning could change the interpretation of a rule.
Add or correct the term in the same change that introduces or redefines it, before the change is accepted.
Do not copy the general STE dictionary into the table.
Keep technical verbs and technical nouns in different tables.
Use established software terms.
Do not give one concept two different names without an explicit alias entry.
Write code identifiers, commands, and paths exactly as they appear in the code.

Record each retired term in the retired-terms table with the term that replaces it.
A retired term appears in rule text only inside quotation marks, next to its replacement, when the text explains the replacement.
Name the role, not a local value: write "the default branch", not one project's branch name.
State the rule, not its strictness: the enforcement level lives in the Enforcement section, and the rule text does not call itself mandatory or advisory.
Write each example so that a reader can copy it: an example never shows an operation that a rule forbids, unless the example is marked as the counterexample.

Keep each rule identifier bound to the same requirement.
Follow the [identifier policy](rule-identifiers.md) for historical references and retired rules.

### Verification

- CI: the checker fails on a retired term outside the exempt documents.
- Review question: does this change introduce a term the table does not define, or show an operation a rule forbids without marking it as a counterexample?

<a id="reas-doc-004"></a>
## REAS-DOC-004: Update documentation with the code

### Intent

A document that describes behavior the code no longer has misleads its reader.
Updating it in the same change is the only time the author still knows what changed.

### Enforcement

Required.

### Scope

Applies to every change that alters observable behavior, an interface, an operating procedure, or a decision.
Does not apply to a refactoring with no observable change.

### Rules

Update domain, architecture, API, operation, and decision documents in the same change as the code.
Make instructions reproducible.
Give prerequisites, ordered steps, and expected results.
For a procedure that changes persistent state, give recovery steps.
Also give recovery steps when a failure can stop a procedure between its steps.

State a requirement in a decision document in one of these shapes, so that it names its trigger and its observable response:

- "The system shall <response>."
- "When <trigger>, the system shall <response>."
- "While <state>, the system shall <response>."
- "If <condition>, then the system shall <response>."

A requirement is testable, names no solution, has one reading, contradicts no other requirement, and states a measurable threshold where one matters.

### Verification

- Review question: which document describes the behavior this change alters, and is it in the diff?
- Review question: for each requirement in the decision document, what observation passes or fails it?

<a id="reas-doc-005"></a>
## REAS-DOC-005: Use different checks for structure and language

### Intent

A structural checker proves that links resolve and metadata parses.
It does not prove that a sentence is correct or that a term is approved, and a report that mixes the two overclaims.

### Enforcement

Required.

### Scope

Applies to every report about this repository's documentation.
Does not apply to code documentation built by rustdoc.

### Rules

Run the documentation checker and its tests.
The [checker reference](checker.md) states the supported syntax and validation limits.
Keep different records for the technical review and the STE language review.
Do not use structural validity as proof of technical accuracy or STE compliance.

### Verification

- CI: `scripts/check_docs.py` and `python -m unittest discover -s tests`.
- Record: the self-audit reports the structural checks and the language review as separate rows.

<a id="reas-doc-006"></a>
## REAS-DOC-006: Enforce with a check where a check can exist

### Intent

A process rule defers the next violation; a check prevents it.
A rule that restates what a check enforces drifts from the check and then contradicts it.

### Enforcement

Required.

### Scope

Applies to every rule in this standard and to every gate the checker or CI runs.
Does not apply to a rule that only a reviewer can judge, such as whether an abstraction helps the reader.

### Rules

When a rule can be checked mechanically, add the check to the checker, a lint setting, or a CI job.
Then the rule's Verification section names the check and the rule text does not restate what the check enforces.
Give each gate a concrete floor: a non-zero minimum count that the gate asserts, so that a gate which scans nothing cannot pass.
Give each gate a planted-violation test: the test introduces a violation, asserts that the gate fails, and removes it.
Scan the gate's own file where a violation could appear there.
Record each allowlist count in a committed baseline that may only shrink or stay; a growth needs a justification comment on the changed line and a tracker reference.
When a defect class recurs, add a check that makes the next instance a build failure, not another reminder.

### Verification

- Test: the checker's unit tests include a planted violation for each gate.
- Review question: for each new rule, which check enforces it, and if none can, why is a reviewer's judgment the only possible check?

## Quality test

Can a reader find the check that enforces each rule and the date on which each page was last verified?
