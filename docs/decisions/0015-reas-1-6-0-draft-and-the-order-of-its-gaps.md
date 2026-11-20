---
type: Architecture Decision
title: REAS 1.6.0-draft, and the order in which its gaps close
description: Why the project moved from REAS 1.4.0-draft to 1.6.0-draft in one step, what the new version asks that the code does not do yet, which exposure applies, and the order of the changes that close the gaps.
tags: [decision, reas, standard, security]
generated: { by: "process:claude-code", at: "2026-11-20T22:30:00Z" }
updated: 2026-11-20
sources:
  - id: reas
    resource: https://github.com/abenamer/reas
    title: REAS engineering standard, 1.6.0-draft, commit 9df4053
---

# ADR 0015 — REAS 1.6.0-draft, and the order in which its gaps close

## Context

The project was made against REAS 1.4.0-draft, copied to `standards/reas/` on 2026-11-08. The standard then changed twice:

* 1.5.0-draft, 2026-11-18. A new concept, Security at seams, with REAS-SEC-002 to REAS-SEC-006: outside data is parsed at the trust boundary, sizes from outside data use checked arithmetic, unsafe code is confined, a duplicate implementation is tested against its reference, and output is encoded for its context. REAS-TEST-003 asks for a fuzz target for each adapter that reads outside bytes. REAS-ARCH-003 gained a fifth question: who can supply outside data. Its answer is the exposure, and each security rule scales by it.
* 1.6.0-draft, 2026-11-20. Every rule has one shape: Intent, Enforcement, Scope, Rules, optional Allowances, Verification. The Verification section names a lint, a CI job, a test or a review question. New rules: REAS-RUST-008 (money), REAS-RUST-009 (documentation of public items), REAS-DATA-005 (idempotent consumers), REAS-TEST-004 (doubles only at ports), REAS-TEST-005 (triage of a failing test), REAS-DOC-006 (a check where a check can exist). Older rules name lints that they did not name before.

The maintainer asked for the upgrade on 2026-11-20. On that day 1.6.0-draft was commit `9df4053` on a branch of the source, in a pull request that was open and whose five jobs had passed; the source's default branch held 1.5.0-draft.

The [self-audit](/self-audit.md) of 2026-11-20 read each rule of 1.6.0-draft against the code at commit `534da18`. The code follows the older rules, and most of the new text describes what the code already does: typed errors, private fields, a conditional update with a version, parameterized queries, redaction before storage. The gaps are of three kinds:

* A check that the rule names and the project does not run: a lint level, the documentation build, a daily scan, a fuzz job.
* A record that the rule asks for and nobody wrote: the review of build scripts, the list of dependencies with unsafe code, the restore test, `updated` dates.
* Code that does what an older version permitted: serde attributes on domain types, `String` keys across a port, arithmetic and indexing in the parser that are correct by reading and not checked.

The exposure is the answer to question 5 in [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md). Mail is outside data from other parties. REAS lets a private tool record an exception for the CI jobs of REAS-SEC-001, REAS-SEC-005 and REAS-TEST-003 only when its maintainer is the only source of outside data. That is not the case here.

## Decision

* The system shall follow REAS 1.6.0-draft, as vendored at `standards/reas/` from commit `9df4053`. `.claude/rules/rust.md` condenses that version.
* If the source's pull request merges with a different commit or a changed text, then the copy shall be made again from the merged commit, in its own pull request, and the audit rows that the change touches shall be read again.
* When a change adds or rewrites code, the new code shall follow 1.6.0-draft, also in a place where older code beside it does not. A change shall not make a count in the self-audit larger.
* The project records no exception to a security rule in this decision. An exception is the maintainer's decision. Until the maintainer makes one, a rule that the code does not meet is a gap in the self-audit, not an exception.
* The gaps close in the order of the table below, one pull request for each step, each under the commit protocol of [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md). When a step merges, the same change shall move its rows in the self-audit.
* Redaction before storage stays ([ADR 0004](/decisions/0004-redaction-before-storage.md)). REAS-SEC-006 says to store a value as it was entered and to encode it on output. The redactor removes personal data on purpose, before storage, and the stored text is still outside data that each sink must encode. The self-audit records this as a deviation from the rule's text, not as a gap to close.
* The Privacy section of `CLAUDE.md` wins over a rule of the standard. A value taken from a mail shall not go to a log, also not as the rejected value that REAS-SEC-002 gives to the log.
* The self-audit shall give each rule one status and its evidence, and shall not report the project as compliant with REAS while a row is "Not met" or "Partly met".

| Step | Rules | What changes | Size on 2026-11-20 |
| --- | --- | --- | --- |
| 1 | REAS-RUST-001, 003, 004, 005, 006; REAS-SEC-003, 004 | A `[lints]` table in `Cargo.toml` with the levels the rules name; `overflow-checks = true` for the release profile; `#[expect]` with a reason in place of `#[allow]` and on each accepted `expect` | 1 wildcard arm, 3 values passed by value, 5 `#[allow]`, 39 `expect` calls |
| 2 | REAS-SEC-006 | A list of the output sinks; encoding of control characters where the terminal table and the log write a text from a receipt; one test for each sink | Not counted |
| 3 | REAS-SEC-003 | Checked arithmetic and `get` in the modules that read outside data, with the two lints at `deny` there; `size_limit` on each pattern that outside data reaches | 52 and 28 lint findings; 31 patterns |
| 4 | REAS-DOC-002, 003, 006; REAS-SEC-001; REAS-RUST-006; REAS-DATA-004 | `updated` dates and their check; the paths above in the privacy canary; retired terms and their check; one script that lists the checks, used by CI, the hook and `CLAUDE.md`; a planted violation for each gate; a daily `deny` run; the documentation build in CI; a toolchain file | 30 bundle files without a date; 3 lists of checks; the privacy canary does not scan `standards/`, `.claude/`, `CLAUDE.md` or `AGENTS.md` |
| 5 | REAS-RUST-009 | A doc comment on each public item, `# Errors` and `# Panics` sections, the three lints | 405, 38 and 6 lint findings |
| 6 | REAS-DATA-001; REAS-SEC-002; REAS-ARCH-004 | Wire types for `qm due --json` in the CLI adapter, so that the domain types carry no serde attribute; `MessageKey` in place of `String` across `RawMessageStore` | 5 domain files with serde; 2 port methods |
| 7 | REAS-TEST-003 | A fuzz package with one target for each reader of outside bytes, a seed corpus built from the synthetic fixtures, and a job with a time budget | 7 entry points |
| 8 | REAS-RUST-008 | The currency in `Money`, a named rounding mode at each rounding point, an error in place of `saturating_add`, the three tests, the float lint in the module | 1 module, its callers |
| 9 | REAS-SEC-001; REAS-DATA-003 | Records: the review of the build scripts and proc-macros in the dependency graph, the list of dependencies that read outside data and contain unsafe code, the design review of the redactor and the Gmail adapter, the backup and restore runbook with one recorded restore | 50 and 24 packages; 1 drill |
| 10 | REAS-TEST-001, 004; REAS-STATE-003; REAS-DATA-002 | One `cargo mutants` run with its survivors read; the real `PlatformMail` in place of the `ByteReader` double; a test of two writers; a test that fails a later write in the transaction | Not counted |

The order puts first what is cheap and stops new gaps (the lints). Step 2 is second because the security review of this decision named it: the table that `qm due` prints is the one sink where a text from a receipt reaches a terminal with no encoding. Then comes what protects the parser of outside data, then the records. Step 7 needs a nightly toolchain and `cargo fuzz`, which the project does not use today. Step 9 needs Docker Compose with the real volume and the maintainer at the keyboard for the restore.

## Consequences

* Until the ten steps merge, the project follows a standard that its code does not meet in full. The self-audit says where. `CLAUDE.md` and the README say that the project follows REAS; they do not say that it complies.
* The copy is of a commit that the source had not merged. If the text changes before the merge, the audit must be read again for the changed rules.
* The lints of step 1 run under `-D warnings` in CI, so each later change meets them or carries an `#[expect]` with a reason that a reader can judge.
* Steps 2, 3, 6 and 8 change code that parses receipts and prints the next order. Each needs its tests to hold without a change to an assertion; REAS-TEST-005 applies if one fails.
* The vendored examples include a fuzz package and two small binary corpus files. Nothing in this project builds or runs them. The `dependency-review` job and GitHub's dependency graph read their two lockfiles.

## Alternatives

* Stay on 1.4.0-draft. Rejected: the maintainer asked for the upgrade, and the new security rules describe the exact risks of this project, a parser of mail that other parties wrote.
* Copy 1.5.0-draft from the source's default branch, a merged commit. Rejected: 1.6.0-draft contains it, the maintainer pushed 1.6.0-draft the same day, and a second upgrade a few days later would repeat the audit.
* Close every gap in one pull request. Rejected: the change would touch most files of the project, and the maintainer asked on 2026-11-20 for small pull requests.
* Record exceptions now for the fuzz job, the dependency reviews and the restore test. Not done: REAS gives the exposure decision to the maintainer, and the audit should show a gap as a gap until that decision exists.

## Runs

Measured on 2026-11-20 at commit `534da18` with Rust 1.99.1. Each count is the number of lines that `cargo clippy --locked --lib --bins --message-format=short -- -W <lint>` printed for `src/`.

| Lint | Findings |
| --- | --- |
| `clippy::wildcard_enum_match_arm` | 1 |
| `clippy::same_name_method` | 0 |
| `clippy::needless_pass_by_value` | 3 |
| `clippy::redundant_clone` | 0 |
| `clippy::unwrap_used` | 0 |
| `clippy::expect_used` | 39 |
| `clippy::allow_attributes` | 5 |
| `clippy::float_arithmetic` | 0 |
| `clippy::arithmetic_side_effects` | 52 |
| `clippy::indexing_slicing` | 28 |
| `missing_docs` | 405 |
| `clippy::missing_errors_doc` | 38 |
| `clippy::missing_panics_doc` | 6 |

`cargo metadata --locked` lists 335 packages for all targets and features: 50 have a build script and 24 are proc-macros. Not run: `cargo mutants`, `cargo fuzz`, `cargo geiger`, `cargo vet`, the documentation build with warnings denied.
