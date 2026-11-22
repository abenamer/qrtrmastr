---
type: Technical Reference
title: Files and their authority class
description: Each committed file of the repository with its authority class, who or what writes it, and how to change it.
tags: [documentation, repository, files]
generated: { by: "process:claude-code", at: "2026-11-21T20:50:00Z" }
updated: 2026-11-21
sources:
  - id: reas-doc-002
    resource: ../../standards/reas/standard/documentation.md
    title: REAS-DOC-002, the four authority classes
  - id: checker
    resource: ../../scripts/check_docs.py
    title: The documentation checker, which reads the table of this page
---

# Files and their authority class

REAS-DOC-002 gives each committed file one authority class. The class says who writes the file and what a change to it means. The four classes and their meanings are the rule's. Three parts of the meanings below are this project's additions: a copy of a source outside the repository is derived, an agent can record evidence, and the result of a reading is evidence like the result of a run.

| Class | Meaning | How to change the file |
| --- | --- | --- |
| Authored | A person or an agent writes it. It is the source of truth. | Edit it, in a pull request. |
| Derived, committed | A command writes it from an authored source, or it is a copy of a source outside the repository. An edit by hand is lost when the command runs again. | Run the command, or make the copy again. Do not edit the file. |
| Evidence | A person or an agent records the result of a run or of a reading, or what a change did. It describes a revision and does not change the code. | Add to it when a new run, reading or change exists. Do not rewrite an old record to make it agree with new code. |
| Companion | A summary of authored content for a reader. It carries no rule of its own. | Change the authored file first, then the summary. |

## The class of each file

The documentation checker reads this table (`scripts/check_docs.py`, in the `docs` job). A path is one file, a directory with a slash at its end, or a part of a name with `*`, which stands for any text, a slash too. Upper case and lower case differ. A file takes the class of the first row that matches it, from the top; so a file with its own class stands above the row of its directory.

The checker fails when:

* an entry in the index of git matches no row. A new file counts once it is staged; a submodule and a staged file that is not in the working tree count too;
* no file takes a row as its first match;
* a class is not one of the four;
* the third cell of a derived row holds no code span. The span is meant to name the command, or the record that says how the copy is made; the checker does not know what the span says;
* the third cell of a companion row is empty;
* a path is not one code span, is a `*` alone, or holds `?` or `[`;
* the page, this section or the table is missing, the table does not start with its header row, a line of it does not start at the margin, or the section holds a comment or a code block.

A new file at the top of the repository, and a file in a new directory at the top, needs a row here in the same change. A new file below a directory that has a row takes the class of that row, and nobody is asked.

| Path | Class | Who or what writes it |
| --- | --- | --- |
| `Cargo.lock` | Derived, committed | `cargo` writes it from `Cargo.toml`. `cargo update` moves a version; a build with `--locked` does not change it. |
| `fuzz/Cargo.lock` | Derived, committed | `cargo` writes it. It starts as a copy of `Cargo.lock`: `cp Cargo.lock fuzz/Cargo.lock`, then `cargo +nightly-2026-11-15 fuzz build`, so the fuzz build has the versions of the program. |
| `unsafe-baseline.tsv` | Derived, committed | `scripts/check.sh unsafe write` writes it from the result of `cargo geiger` 0.13.0: the count of unsafe items for each package that the counter names. `scripts/check.sh unsafe` fails when the count is not the file. |
| `fuzz/seeds/` | Derived, committed | `python3 scripts/fuzz_seeds.py` writes the seeds of the fuzz targets from the synthetic receipts; its `--check` fails on a seed that was edited by hand. |
| `fuzz/` | Authored | The fuzz targets and their manifest. |
| `src/adapters/cli/snapshots/` | Derived, committed | The snapshot library writes it from the tables that `qm due` prints for the sample reports: `INSTA_UPDATE=always cargo test --lib the_tables_for_the_terminal`. With that setting the test passes whatever the program prints, so read the difference before you commit it: a change of a file is a change of what the user sees. |
| `src/adapters/receipts/snapshots/` | Derived, committed | The snapshot library writes it from the parser and the synthetic receipt: `INSTA_UPDATE=always cargo test --lib the_parsed_receipt_matches`. With that setting the test passes whatever the parser gives, so read the difference before you commit it: the file is what the test expects. The comment of the test names the command too. |
| `standards/reas/README.md` | Authored | It says which commit of the REAS repository the copy beside it is, and what was copied. |
| `standards/reas/` | Derived, committed | A copy of the REAS repository at the commit that `standards/reas/README.md` names. No script makes the copy. No file of it is edited here; [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) says when the copy is made again. |
| `docs/self-audit.md` | Evidence | The result of a reading of each REAS rule against the code at a named commit. |
| `docs/ste-review.md` | Evidence | The result of the language review. |
| `docs/log.md` | Evidence | For each change: the files, the checks that ran, and what the security review found. |
| `README.md` | Companion | A summary of the bundle for a first reader; it points at `docs/index.md`, the command line reference and the runbooks. |
| `CHANGELOG.md` | Evidence | For each change, what a user of `qm` sees of it. A line of a past change is not rewritten; `docs/log.md` holds the full record. |
| `docs/` | Authored | The OKF bundle: concepts, decisions, references, runbooks, the terminology and the index. |
| `src/` | Authored | The program. |
| `tests/fixtures/synthetic/json/` | Authored | The text of `qm due --json` for five sample reports: the reference for that interface. The serializer of the commit before the wire types wrote the files. Change one only together with the wire type in `src/adapters/cli/wire.rs`, and say so in the changelog. |
| `tests/` | Authored | The integration tests, the tests of the documentation checker, and the synthetic receipts. |
| `examples/` | Authored | The planted lint violations. |
| `migrations/` | Authored | A migration that is merged is not edited; a new migration adds to it. |
| `scripts/` | Authored | The checks, the privacy canary, the page script, the two scripts for the unsafe count and its paths, and the backup with its restore drill. |
| `.claude/` | Authored | The hooks, the rules for a session and the permission settings. |
| `.github/` | Authored | The two workflows and the Dependabot settings. |
| `.cargo/audit.toml` | Authored | The one advisory that the audit ignores, with its reason. |
| `backups/.gitkeep` | Authored | It keeps the directory, whose other content git ignores. |
| `AGENTS.md` | Authored | Instructions for an agent. |
| `CLAUDE.md` | Authored | Instructions for a session: the commands, the commit protocol, the privacy rules. |
| `Cargo.toml` | Authored | The dependencies and the lint table. |
| `Dockerfile` | Authored | The image of `qm`. |
| `LICENSE` | Authored | The license of the project. |
| `clippy.toml` | Authored | The three allowances for test code. |
| `compose.yaml` | Authored | The services of Docker Compose. |
| `deny.toml` | Authored | The settings of `cargo deny`. |
| `requirements-docs.txt` | Authored | The pinned Python packages of the documentation checker. |
| `rust-toolchain.toml` | Authored | The pinned Rust version. |
| `.dockerignore` | Authored | What the image build does not read. |
| `.env.example` | Authored | The environment variables with example values. The password and the two values of the Gmail client are empty. |
| `.gitignore` | Authored | What git does not track: the local fixtures, the backups, `.env`, the page of `qm due --html`, the Gmail token. |
| `.gitleaks.toml` | Authored | The settings of the secret scan. |

## What the table does not say

* The class is a statement about the file, not a control. The checker does not know whether a class is right, or whether a derived file was edited by hand. The test of the snapshot fails on a snapshot that was edited by hand; nothing compares the vendored copy with its source. The review question of the rule is "which class is each committed file in this change, and was a derived file edited by hand?".
* The Stop hook runs the checker for a change to a Markdown file, the checker or its tests. A new file of another kind without a row passes the hook and fails in the `docs` job.
* `.sqlx/` does not exist yet. When the first checked query is added, `cargo sqlx prepare` writes it, and it gets a row of the class "Derived, committed".
* A file that git ignores has no class: the local fixtures, the backups, `.env`, `target/`, and a new snapshot that a failed test leaves beside the committed one.
* `.claude/rules/rust.md` condenses the vendored standard and also holds instructions of this project, so it is authored, not a companion.
