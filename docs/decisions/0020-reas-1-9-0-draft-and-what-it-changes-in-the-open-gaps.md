---
type: Architecture Decision
title: REAS 1.9.0-draft, and what it changes in the open gaps
description: The copy of REAS 1.9.0-draft, the rules that it adds or changes, what moves in the plan of ADR 0019, and the judgments that the maintainer recorded on 2026-11-22.
tags: [decision, reas, planning]
generated: { by: "process:claude-code", at: "2026-11-23T02:30:00Z" }
updated: 2026-11-22
sources:
  - id: reas
    resource: https://github.com/abenamer/reas
    title: REAS engineering standard, 1.9.0-draft, commit bfb99b7
  - id: audit
    resource: ../self-audit.md
    title: The self-audit, with the rows that this version changes
  - id: adr-0019
    resource: 0019-the-gaps-that-remain-after-the-ten-steps.md
    title: ADR 0019, the proposed plan for the gaps
---

# ADR 0020: REAS 1.9.0-draft, and what it changes in the open gaps

## Status

The copy of the standard is done, on 2026-11-22. Accepted on 2026-11-22, later the same day: the changes to the lines 9, 10, 13, 15 and 16 of part 1 of [ADR 0019](/decisions/0019-the-gaps-that-remain-after-the-ten-steps.md) and the new line for `cargo vet`, which the maintainer accepted with part 1; the consequence of each component, as proposed; Mozilla, Google and the Bytecode Alliance as the sources of audits for `cargo vet`, where the maintainer decides about each package that no audit covers before it is exempted; and a change of the count of a dependency as a report, with line 16 and after `cargo vet` is set up, while the project's own crates still fail on any unsafe code. Two lines of part 2 that the table below takes out, the review of the unsafe code of the dependencies and the release stage, needed no answer: the standard no longer asks for them. The sixth answer of REAS-ARCH-003 and the shape of the self-audit were answered after that, the same day: the answer is in ADR 0001, with three decisions more than the five that this record proposed, and the short form of the self-audit is line 23 of ADR 0019. The headings below say "Proposed" as they were written.

## Context

The maintainer asked on 2026-11-22 to make the project compliant with REAS. The copy of the standard in the repository was 1.7.0-draft. The source was at 1.9.0-draft on that day, on its default branch: commit `bfb99b7`, the merge of its pull request #6. A project cannot follow a version that it does not hold, so the copy came first.

What the two new versions change:

* 1.8.0-draft adds the concept Foundations with REAS-FOUND-001 to REAS-FOUND-003: no exception for a first version, no foreseeable rework, and the first instance of a pattern as the template. REAS-ARCH-003 has a sixth question.
* 1.9.0-draft says what REAS is for: it runs bounded checks and shows what departs, and the maintainer decides what to investigate. REAS-FOUND-004 separates two questions. A property of the system decides whether a check applies. The consequence of a failure, at one component, decides how much of the check runs. REAS-FOUND-005 says that a check is not an investigation, and how short a report is. REAS-ARCH-003 has a seventh question. A rule can have a Limits section.
* REAS-SEC-001 no longer asks for a list and a review of the unsafe code of each dependency on a path of outside data. It asks for `cargo vet` with the audits of organizations that the project names, and for an own review only where no trusted audit covers a build script or a procedural macro, or where a finding stays open.
* REAS-NATIVE-002 keeps two counts. The count of the project's own crates is a baseline that fails the build when it grows. The count of a dependency is reported, and a change is a review event and not a build failure.
* REAS-TEST-001 makes the mutation pass Required for decision code in a consequential component and Advisory elsewhere. A missed mutant gets one of three outcomes in the pull request: killed, equivalent, or accepted survivor. REAS-TEST-002 reports the result of the tool and the disposition of the maintainer as two statements.
* REAS-TEST-003 asks for one run of 30 to 60 seconds for each target on each pull request, from a corpus that is kept, with a report of what ran. It no longer asks for a nightly stage and a release stage. A longer campaign is a recorded decision for a consequential component. Other architectures are due where the code has SIMD paths or paths that depend on the byte order.
* REAS-RUST-005, REAS-STATE-003 and REAS-SEC-002 name what an error and a log line carry so that a person can start: the operation, the expected and the observed condition, the identifier of the record, and no sensitive value.

The standard has no status "compliant". Its own self-audit is a table of checks with one of six outcomes for each. The [self-audit](/self-audit.md) of this project keeps its four words for a rule, and the sentence of [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) holds: the project is not reported as compliant while a row is "not met" or "partly met".

## Decision

* The system shall follow REAS 1.9.0-draft, as vendored at `standards/reas/` from commit `bfb99b72b99ea1c021fd83ceb675a0afb3d7c2ff`.
* When a change adds or rewrites code, the new code shall follow 1.9.0-draft.
* The self-audit shall hold a row for each of the 49 rules of 1.9.0-draft.
* If the maintainer accepts a part below, then the record of that part shall say "accepted on" with the date. A part that the maintainer refuses is taken out.

### Proposed: what moves in ADR 0019

| Line of ADR 0019 | What 1.9.0-draft changes | Proposal |
| --- | --- | --- |
| Part 2, "No unsafe code of the 107 dependencies on a path of outside data is reviewed" | The rule no longer asks for that review. It asks for `cargo vet`. | The line goes. A new line of work replaces it: set up `cargo vet` with imported audits, run `cargo vet check` in CI, and record an own review only for what no trusted audit covers. The maintainer names the organizations to trust. ADR 0019 left `cargo vet` undecided; the rule now names it. Named on 2026-11-22: Mozilla, Google and the Bytecode Alliance. |
| Part 1, line 10, the build scripts of 27 packages and the full reading of 18 macro crates | An imported audit stands in for an own review. | The line waits for `cargo vet`. An own reading is due only for a package that no trusted audit covers. |
| Part 1, line 16, and the `unsafe` group | A change of the count of a dependency is a review event, not a build failure. | The group today fails on a change of any package. That is more than the rule asks, and it is not what the rule describes. The maintainer chooses: keep the failure, or make the count of a dependency a report. Chosen on 2026-11-22: a report, with line 16 and after `cargo vet` is set up. |
| Part 2, "One processor architecture is counted and fuzzed. No release stage." | No release stage is asked. A second architecture is due where the code has SIMD paths or paths that depend on the byte order. | The release stage goes from the line. The program has no such path of its own; some dependencies have. Whether the sentence means the dependencies is not clear from the text, so the architecture stays a question to the maintainer. Answered on 2026-11-22: one architecture is accepted as a limit, and moves to work if `qm` runs on another architecture. |
| Part 1, line 15, the fuzz targets | A report of the executions, the size of the corpus and whether the run reached its end. Kept inputs in artifact storage or a corpus repository. | Both are added to the line. The run prints the final statistics of the fuzzer; no report says whether the run reached the end of the corpus. The cache of CI is not storage that stays. |
| Part 1, line 13, `cargo mutants` on the lines of a change | Required for decision code in a consequential component, with three outcomes for a missed mutant. Not a gate of CI. | The line follows the judgment of the next table. No job of CI is added for it. |
| Part 2, "Six mutants of the run of 2026-11-21 have no test" | Each missed mutant gets a disposition, and the maintainer gives it. | Two are equivalent, and the log of step 10 says why. The other four fit none of the three outcomes as the log describes them: each needs a test that the project cannot run today. They stay open as reported survivors until the maintainer disposes of them. Disposed on 2026-11-22: the two of the hours of overlap are to be killed with line 6; the other two stay reported survivors. |
| Part 1, line 9, a test for each error variant | A test asserts the typed variant and the diagnostic facts, not the wording. | The reading of line 9 also asks, for each variant, whether it carries the operation, the expected and the observed condition. |

### Proposed: the consequence of each component

REAS-FOUND-004 asks for a failure path of one sentence for each consequential component, and says that a session proposes it and the maintainer records it. Until the maintainer records a judgment, a component that handles outside data, credentials or money is treated as consequential. A "not consequential" below, when the maintainer accepts it, makes the mutation pass Advisory for that component and leaves its fuzz run at the baseline; the fuzz target, the limits and the tests stay, because they apply by property.

The maintainer accepted the table on 2026-11-22 as it stands; it is the record of the judgment.

| Component | Proposed | Failure path, or the reason |
| --- | --- | --- |
| The redactor, `src/adapters/mail/redact.rs` | Consequential | A form of a term that the redactor does not know is stored with the mail, and a person who reads the database, a backup or a fixture reads the address, the phone number or the card ending of the maintainer. |
| The Gmail adapter, `src/adapters/mail/gmail.rs` | Consequential | A defect that writes the token to a log, or that leaves the token file open to another user, gives a person who reads that log or that file the mail of the whole mailbox. The token cannot change or send mail. |
| The mail readers and the classifier | Not consequential | A defect stores a wrong kind or refuses a mail. The redactor runs after them on each stored part, so no path from a defect here to personal data was found; the fuzz targets and the limits stay, because they apply by property. |
| The receipt parser, the money type and the reconciliation | Not consequential | A defect gives a wrong line in `qm due`. The program pays nothing and orders nothing; the maintainer reads the list and places the order by hand. |
| The store adapter | Not consequential | A defect loses or mixes rows of receipts. The history can be imported again from the mail. The backup is a different matter, which REAS-DATA-003 holds. |
| The recommendation rules | Not consequential | The same reason as the parser. |

### Proposed: the two new answers of REAS-ARCH-003

The answers are the maintainer's, and [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md) records them. The maintainer gave both on 2026-11-22.

6. Which decisions are costly to reverse, or will later code copy? Proposed: the redaction before storage, because a stored row keeps the form that its redaction version gave it; the message key, because each stored row is found by it; the schema, because migrations only add; the split into domain, application and adapters; the form of a parser and of its fixture test, because each new platform copies it. Answered on 2026-11-22, in ADR 0001: these five, and the sign-in to Gmail, the date library and the names of the keys of the `[redaction]` section.
7. Which components are consequential? Proposed: the table above. Accepted on 2026-11-22, and in ADR 0001.

### What this record does not decide

* It does not say that the project complies with REAS.
* It records no exception.
* It does not start the setup of `cargo vet`, and it does not change the `unsafe` group. Until `cargo vet` is set up, no trusted audit exists, and each new dependency with a build script or a procedural macro gets a recorded own review before it is added, as before.
* It does not change the shape of the self-audit. The standard cut its own audit to the evidence of the current revision, and REAS-FOUND-005 asks for a short report. The self-audit of this project has long rows; whether it gets the shorter shape was a question to the maintainer, who said yes on 2026-11-22 (ADR 0019, line 23).

## Consequences

* The self-audit has 49 rows. After the copy: 20 met, 26 partly met, 2 not met and 1 not applicable. After the answers of the maintainer of 2026-11-22: 21, 26, 1 and 1. REAS-FOUND-004 is partly met: the judgment is recorded, and the effort does not follow it yet. REAS-ARCH-003 is met again: the seven answers are recorded. Before the copy it had 44: 20, 22, 1 and 1. The five rules of Foundations are new rows. With the copy, REAS-ARCH-003 moved from "met" to "partly met", because two of seven answers were not recorded. REAS-FOUND-004 was "not met" until the maintainer recorded the judgments.
* The largest line that ADR 0019 would have left open, the review of the unsafe code of 107 dependencies, is no longer asked. The setup of `cargo vet` takes its place as work.
* No rule of 1.9.0-draft makes a status better by itself: where a gap went, another part of the same row is still open.
* The rows whose rule did not change between the two versions were not read again. Their text is the reading of 2026-11-20 with the changes of the ten steps.
* Nothing that `qm` does changes.

## Alternatives

* Work on the gaps of ADR 0019 against 1.7.0-draft first. Rejected: three lines of that plan answer sentences that the standard no longer has, and the largest of them is work that the project cannot do.
* Write ADR 0019 again. Rejected: it is a record of the reading of 2026-11-21, and the maintainer has it for a decision. This record names each line that moves.
* Record the failure paths as decided. Rejected: REAS-FOUND-004 gives the judgment to the maintainer.

## Runs

No run of the program. The copied tree was compared with `git archive` of the commit on 2026-11-22, and the two were equal. The checks of the change are in [the log](/log.md) of that day.
