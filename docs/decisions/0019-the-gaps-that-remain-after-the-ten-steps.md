---
type: Architecture Decision
title: The gaps that remain after the ten steps, sorted into work and into limits that the maintainer decides
description: Accepted on 2026-11-22 with the changes of ADR 0020. Each gap of each rule of REAS that the self-audit holds as partly met or not met after ADR 0015, and each finding of the design review, sorted into work with an order or into a limit that needs the maintainer's yes.
tags: [decision, reas, planning]
generated: { by: "process:claude-code", at: "2026-11-22T04:30:00Z" }
updated: 2026-11-22
sources:
  - id: audit
    resource: ../self-audit.md
    title: The self-audit, with the status and the gaps of each rule
  - id: review
    resource: ../reference/design-review.md
    title: The design review of the redactor and the Gmail adapter
  - id: adr-0015
    resource: 0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md
    title: ADR 0015, the ten steps
---

# ADR 0019: The gaps that remain after the ten steps, sorted into work and into limits that the maintainer decides

## Status

Accepted on 2026-11-22, with the changes of [ADR 0020](/decisions/0020-reas-1-9-0-draft-and-what-it-changes-in-the-open-gaps.md). The maintainer accepted part 1 as written, as the order of the work, and gave an answer to each line of part 2 that 1.9.0-draft still asks; the section "The answers of the maintainer" holds them. The questions that a line of part 1 gives to the maintainer in its column "Who" were not asked and are open: each is asked when its line starts. The record was proposed on 2026-11-21. The tables of part 1 and part 2 are the proposal as it was written and are not changed. No answer records the exception of the standard.

On 2026-11-22 the copy of the standard changed to 1.9.0-draft. This record reads 1.7.0-draft. [ADR 0020](/decisions/0020-reas-1-9-0-draft-and-what-it-changes-in-the-open-gaps.md) names each line below that the new version moves: lines 9, 10, 13, 15 and 16 of part 1, and of part 2 the lines on the unsafe code of the dependencies, on the one architecture and the release stage, and on the six mutants.

## Context

[ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) put the gaps of the first audit into ten steps. The ten steps are written. Steps 7 to 10 are in the pull requests #43 to #47, which wait for their merge, and the restore of the Compose database, a part of step 9, is the maintainer's to run. With those pull requests the [self-audit](/self-audit.md) holds 20 rules as met, 22 as partly met, 1 as not met and 1 as not applicable.

The maintainer decided on 2026-11-21 what comes after step 10, and [the log](/log.md) of that day holds the decision: one record that reads each remaining row again and sorts its gaps into work to do and into limits to propose, with no exception recorded without the maintainer's yes. This is that record.

ADR 0015 says two things that this record does not change. An exception is the maintainer's decision. The self-audit does not report the project as compliant with REAS while a row is "not met" or "partly met".

REAS lets a private tool record an exception for the CI jobs of REAS-SEC-001, REAS-SEC-005, REAS-TEST-003, REAS-NATIVE-002 and REAS-NATIVE-004 when its maintainer is the only source of outside data. Mail is outside data from other parties, so that clause does not apply here ([ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md), question 5). A "stated limit" below is therefore not that exception, also where it is about one of those five rules. It is a gap that stays open, and a row that stays "partly met", with a reason that the maintainer has read and accepted: a decision, where today it is only a sentence in a table.

The steps also found things that are no row of the audit: the 17 findings of the [design review](/reference/design-review.md), and the notes that the logs of 2026-11-21 hold as left open. Each finding is sorted below. The notes of the logs that a line names are sorted, and line 18 is for the others.

## Decision

If the maintainer accepts this record, then the project shall work through part 1 in its order, each line in one pull request or more, each under the commit protocol of [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md). When the maintainer accepts a line of part 2, the self-audit shall say "accepted on" with the date beside that gap. When the maintainer refuses a line of part 2, the line shall move into part 1.

### Part 1: what closes by work, in the proposed order

The order has six classes: first what can store or show personal data or a secret, then what protects the data and the access that exist, then what can stop a run or does not end one, then the readings that the audit did not make, then the checks, then the tests that replace code of the application.

| Order | Class | What | Rules, findings and notes | Who |
| --- | --- | --- | --- | --- |
| 1 | Personal data | An import refuses to run when the `[redaction]` section is empty or has a key that the program does not know. | F1; REAS-SEC-002 (`deny_unknown_fields` on the types of the configuration) | A session; the maintainer says what an empty kind means |
| 2 | Personal data | The redactor: a term in percent-encoded form in each stored part and attribute; the query string and the path of a link in the text body and outside three attributes; a card ending after a mask and a hyphen, and the forms of a card ending that the log already holds as missed; the forms of F4 that are new (an invisible character that is not one of six, another Unicode composition of a name, an address or a number that is not configured inside a `Message-ID`). For a phone number and a card ending the assertion gets a check of its own, so that a form that the pass does not know refuses the message. A new redaction version. | F2, F3, F5, the new parts of F4 | A session, after the maintainer's yes to each rule of the redactor |
| 3 | Personal data | A stored row of an older redaction version is found, and redacted again or removed, before it is read or written to a file. | F8 | A session; the maintainer decides what happens to a stored row |
| 4 | Personal data | `qm fixture add` writes only below a directory that git ignores. The terms cannot be printed with their debug form. A library is silent below a warning unless the rule of the log filter names it, and a test runs the filter as `main` builds it. The value of an unknown parameter of the database address does not reach the log. | F9, F13, F15; the log of step 10 | A session |
| 5 | Data that exists | The restore drill on the Compose database, and its record in the log. Whether a backup runs on a schedule. | REAS-DATA-003 | The maintainer |
| 6 | Data that exists | `qm fetch` takes the start of its search from a value that no sender sets. The proxy of the environment. What Compose gives each command: the state directory and the client secret. The token file is read only with the mode and the owner that the program gave it, and not through a link; the mode of the state directory is checked. A command ends the access. | F10, F14, F16, and of F11 the file and the missing command | A session; the maintainer for each one that stays |
| 7 | A run that stops | A size limit for one mail before it is redacted. An answer of Gmail above the limit as one refused message. The time of the mail reader, which grows with the square of the depth of nested tags. A fuzz property for a limit of memory or time. | F12; REAS-SEC-001, REAS-SEC-003, REAS-TEST-003; the logs of the limits and of step 9 | A session |
| 8 | A run that stops | A shutdown deadline for a run, its record, and a test of a shutdown while an import runs. | REAS-RUST-007 | A session |
| 9 | Readings | What the audit says was not examined, one rule in a pull request: a rejection test through the row mapping for each invariant, and the public fields of `Thresholds` (REAS-RUST-001); a test for each error variant (REAS-RUST-005); an assertion for each query, and a test that fails the second write of `resolve_alias` (REAS-DATA-001, REAS-DATA-002); the `Deserialize` types of the adapters and the port methods (REAS-SEC-002); the tests against the non-vacuity rules (REAS-TEST-001). | As named | A session |
| 10 | Readings | The build scripts of the 27 packages of the lockfile that are not compiled on Linux, and a full reading of the 18 macro crates that were searched with patterns. A record of which of the 32 packages with a mark a fuzz target reaches, and on which features of the processor the sanitizer build ran. | REAS-SEC-001, REAS-NATIVE-004 | A session |
| 11 | Readings | The 25 pages whose date is the day of their last commit are read against the code. The decisions before ADR 0015 state their requirements in the four shapes. | REAS-DOC-002, REAS-DOC-004 | A session |
| 12 | Readings | `cargo mutants`: one run of the whole program with one job; the unit test of the Takeout reader that failed once under load. | REAS-TEST-001; the log of step 10 | A session |
| 13 | Checks | The tools of CI: a version for `cargo-audit`; an install with a checksum for `sqlx-cli`, `cargo-fuzz` and `cargo-geiger`; `sqlx-cli` in the version of the library. `cargo mutants` on the lines of a change before its review, as a step of the commit protocol. | REAS-SEC-001, REAS-TEST-001; the logs of 2026-11-21 | A session; the maintainer for the change of the protocol |
| 14 | Checks | Gates that are missing: a floor and a planted case for the ignore lists of `cargo deny` and `cargo audit`; a count of the reserved forms that the canary lets pass; a job that builds the image; a comparison of the Rust version of the image with the toolchain file; the target and the native dependencies recorded in one place; Clippy and `cargo deny` for the fuzz package; the step of the unsafe count held in the `rust` job; the page on native code compared with the baseline; a test for the Stop hook and for the shell scripts that have none; a job that reads the code of the vendored examples, or a reason that none does. | REAS-DOC-006, REAS-DATA-004, REAS-NATIVE-001, REAS-NATIVE-002 | A session |
| 15 | Checks | The fuzz targets: one for the answers of the token endpoint; the inputs that a run finds kept and made small, not in a cache that ends after a week; the fuzz package in Dependabot. The fuzz job as a required check. | REAS-TEST-003 | A session; the maintainer for the setting, which the maintainer said on 2026-11-21 to make |
| 16 | Checks | The unsafe count: the two Unicode data packages that the counter does not scan; the fuzz package; the CI run with its revision in the audit row. | REAS-NATIVE-002 | A session |
| 17 | Tests | The tests replace no code of this application: the use case of the import takes the reader of a mail without a trait in front of it, and the test that wraps the parser registry goes, with the second check of the use case or with another way to hold it. | REAS-TEST-004 | A session; the maintainer says which of the two |
| 18 | Readings | Each other note that a log of 2026-11-20 or 2026-11-21 holds as left open is read, and gets a line of this record or a reason to stay. | The logs | A session; the maintainer for each one that stays |

### Part 2: what would stay as a stated limit

Each line is a question to the maintainer, and each asks for a yes. A line that the maintainer accepts stays a gap in the self-audit. To accept a line is not to record the exception of the standard.

| Rule | What would stay open | The reason offered |
| --- | --- | --- |
| REAS-SEC-002 | A receipt without a DKIM pass is parsed, and its order is tagged `non-verified`. | [ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md) decided it: every FreshDirect mail before 2025-06-19 lacks a pass, and to leave those out removes eight years of history. The same decision lists what a forged receipt from the exact sender address can do and what is not prevented: its lines count in `qm due`, because no query leaves the orders without a pass out; and a sender who knows an order number can replace the genuine order of that number. The third thing of that list, an amount that stops the parse step, is closed since step 8. |
| REAS-STATE-002 (advisory) | The sender check is a value on the message and not a state type that the parser requires. | No earlier decision. With the line above accepted, a parser takes a message with or without a pass, so a state type would not keep any message out. The rule also asks that only the function that does the check makes the checked type, and REAS-SEC-001 asks the same for a verification step; for that half no reason is offered. The maintainer may refuse this line. |
| REAS-ARCH-002 | One package holds all layers; the layers check reads text and not the dependency graph; the application layer names the channel type of Tokio. | [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md) rejected a workspace with one crate for each layer as more ceremony than the size of the code warrants. The check has a list of what it does not see. For the channel type the only reason is that the mail source is a stream of the one runtime that the program has. |
| REAS-ARCH-004 | The mail source passes the bytes of a mail as they are. | The reader behind it is the trust boundary; before it, a mail is bytes. |
| REAS-SEC-001, REAS-NATIVE-001, REAS-NATIVE-002 | No unsafe code of the 107 dependencies on a path of outside data is reviewed. | The rule names three ways: `cargo vet`, a recorded review by hand, or another dependency. A review by hand of 31,594 unsafe items is not work that this project can do. `cargo vet` with the audits that other organizations publish is a new tool and a new store of records. This record does not decide it; it is the largest line that would stay. |
| REAS-NATIVE-001, REAS-NATIVE-004 | No fuzz target reaches the TLS library, whose path Miri cannot run. | No reason is offered beside the cost: a target would have to speak TLS to the library. The maintainer may refuse this line. |
| REAS-NATIVE-002, REAS-TEST-003 | One processor architecture is counted and fuzzed. No release stage. | CI runs on one architecture, and the project publishes no release; neither is planned. When either changes, the line moves to part 1. |
| REAS-SEC-003 | The lints do not cover the index of a capture group. | Each indexed group was read, and three tests hold the patterns where the engine can say it. |
| REAS-TEST-001 | Six mutants of the run of 2026-11-21 have no test. | Two change nothing; two need a fetch from Gmail, one a terminal, one a database between two migrations. The log of step 10 names each. |
| REAS-TEST-002 | Results before 2026-11-20 use two outcomes, not six. | A record of the past is not written again. |
| REAS-DOC-001 | Nobody checked the text against the rules and the dictionary of ASD-STE100. | The [STE review](/ste-review.md) asks first for the authorized Issue 9 reference, and says that nobody has checked the text against it. The documents say "targets", never "complies". |
| REAS-NATIVE-001 | The list of the dependencies on a path of outside data follows the dependency graph and not the data. | A trace of the data through 191 packages is not work that this project can do. The graph gives the larger set, so no package that a mail reaches is left out for this cause. |
| REAS-SEC-001 | The design review of the redactor and the Gmail adapter came after their code. | The past is not changed. A component that is new gets its review before its code ([ADR 0018](/decisions/0018-records-for-the-dependencies-the-two-sensitive-components-and-the-backup.md)). |
| Findings F4, F7, F11 | The redactor knows the configured terms and three shapes, and a text in another form is stored. Its assertion reads the text without tags. The token of the sign-in is for the whole mailbox and does not end. | Earlier decisions name each as a limit: [ADR 0004](/decisions/0004-redaction-before-storage.md), [ADR 0017](/decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md) and [ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md). The design review still gives each one what a fix must guarantee, so each is a question here and not a closed matter. |

Two findings of the design review need no line. F6, the phone digits with one more digit after them in a body, was decided by the maintainer on 2026-11-21 and is in ADR 0004. F17 says that no exception is recorded; it is a statement about the others.

### The answers of the maintainer: 2026-11-22

The maintainer read each line with its text and answered one line at a time.

Part 1 is accepted as written, with the lines 9, 10, 13, 15 and 16 as ADR 0020 changes them. The acceptance is of the lines and their order. It answers no question that a line gives to the maintainer: what an empty kind of term means (line 1), each rule of the redactor (line 2), what happens to a stored row (line 3), the drill and the schedule of a backup (line 5), what stays of line 6, the change of the protocol (line 13), the required check (line 15), which of the two ways (line 17) and what stays of line 18. Four lines are new:

| Order | Class | What | From | Who |
| --- | --- | --- | --- | --- |
| 19, before line 10 | Checks | `cargo vet` with the audits of Mozilla, Google and the Bytecode Alliance, and `cargo vet check` in CI. The first run reports how many packages the three sources cover and which are left; the maintainer decides about those before one is exempted. | REAS-SEC-001 in 1.9.0-draft | A session; the maintainer for what no audit covers |
| 20 | Types | An unchecked and a checked message type for the sender check, and the parameter of a parser says which one it takes. | REAS-STATE-002; the maintainer refused the limit | A session |
| 21 | Types | The port of the mail source does not name the channel type of Tokio. | REAS-ARCH-002; the maintainer refused this part of the limit | A session |
| 22 | Readings | The review of the documents against ASD-STE100 Issue 9, with its record in the STE review. It waits for the document, which the maintainer gets and keeps outside the repository. | REAS-DOC-001; the maintainer refused the limit | The maintainer, then a session |

Three lines get more work: line 2 also writes the list of the forms that the redactor covers into the privacy page and makes the assertion of the redactor read each stored character, also inside a tag, in a form that does not depend on the pass (finding F7); line 6 also gets a test that kills the two mutants of the hours of overlap of `qm fetch`; line 16 makes a change of the count of a dependency a report, after `cargo vet` is set up; the project's own crates still fail on any unsafe code. For finding F4 the maintainer chose no second check that is independent of the configured terms; the check of its own that line 2 gives the assertion for a phone number and a card ending stays.

The lines of part 2:

| Line of part 2 | Answer |
| --- | --- |
| REAS-SEC-002, a receipt without a DKIM pass is parsed | Accepted as a limit. |
| REAS-STATE-002, the sender check is a value | Refused. It is line 20. |
| REAS-ARCH-002, one package, a check that reads text, the channel type of Tokio | The one package and the text check: accepted as a limit. The channel type: refused. It is line 21. |
| REAS-ARCH-004, the mail source passes bytes | Accepted as a limit. Line 17 goes on as planned. |
| The unsafe code of the 107 dependencies | No answer is due: 1.9.0-draft no longer asks for it. Line 19 takes its place. |
| No fuzz target reaches the TLS library | Accepted as a limit, with a condition: line 19 shows a trusted audit for `ring` and for the TLS library. If none covers them, the question goes back to the maintainer. |
| One processor architecture; no release stage | The release stage: 1.9.0-draft no longer asks for it. One architecture: accepted as a limit. It moves to part 1 when `qm` runs on another architecture. |
| REAS-SEC-003, the index of a capture group | Accepted as a limit. |
| REAS-TEST-001, six mutants | A disposition for each, as REAS-TEST-001 asks in 1.9.0-draft. `hex_pair` and the arm `tr` of `own_rows`: equivalent. The two of the hours of overlap of `qm fetch`: to be killed, with line 6. The question of `qm add` and the list of the applied migrations: reported survivors, left open. |
| REAS-TEST-002, results before 2026-11-20 | Accepted as a limit. |
| REAS-DOC-001, no check against ASD-STE100 | Refused. It is line 22. |
| REAS-NATIVE-001, the list follows the dependency graph | Accepted as a limit. |
| REAS-SEC-001, the design review came after the code | Accepted as a limit until each finding of that review is corrected or accepted. The sentence then leaves the row of the self-audit. |
| Finding F4 | The list of the covered forms is written with line 2. What is outside that list is accepted as a limit. |
| Finding F7 | Refused. It is corrected with line 2. |
| Finding F11 | After line 6, the token that reads the whole mailbox and does not end is accepted as a limit. |

### What this record does not decide

* It records no exception. The word stays with the maintainer.
* It does not say that the project complies with REAS, with part 1 done or not.
* As proposed, it did not decide `cargo vet`. Since 2026-11-22 line 19 does.

## Consequences

* Part 1 is more than twenty pull requests: line 9 alone is five. Lines 1 to 4, 6 and 7 change what `qm` stores, prints, fetches or accepts; line 8 changes how a run ends, and line 17 the import use case. Line 5 is a drill. The others add readings, tests, checks and records.
* Line 2 needs a new redaction version. A message that is stored is not redacted again by a later import (finding F8), so line 3 decides what a new version means for the rows that exist.
* With part 2 accepted as it stands, a gap of thirteen rules stays open with a reason and a date, and the rows of those rules stay "partly met" for as long as the reason holds.
* The order is by class, and inside a class by how much a defect can do. A line can move: the maintainer may want the drill of line 5 first, because it needs no code.

## Alternatives

* Close every gap. Not proposed: two lines of part 2 need three things that the project does not have today, a second architecture in CI, a release, and a check against the authorized reference of the language standard.
* Stop at ADR 0015 and leave the rows as they are. Not proposed: a gap that is open without a decision reads the same as a gap that was forgotten, and lines 1 to 4 and 6 are defects that the reviews found.
* One pull request for all of part 1. Not proposed: ADR 0015 chose one pull request for each step, because one change would touch most files and the maintainer asked for small pull requests.

## Runs

No run. This record is a reading of the self-audit, the design review and the logs of 2026-11-21, at the commit of step 10.
