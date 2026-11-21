---
type: Audit Report
title: REAS self-audit
description: Each rule of REAS 1.7.0-draft with its status in this project, the evidence for each result, and the limits of the audit.
tags: [quality, documentation, audit]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-21
sources:
  - id: reas
    resource: https://github.com/abenamer/reas
    title: REAS engineering standard, 1.7.0-draft
---

# REAS self-audit

**Recorded validation: Actions run [44821890273](https://github.com/abenamer/qrtrmastr/actions/runs/44821890273) on 2026-11-20 for commit `534da18` on `main`. Six jobs passed: `rust`, `audit`, `deny`, `docs`, `privacy`, `secrets`. The job `dependency-review` was skipped: it runs on a pull request only, and this run was a push. That run does not cover the change that vendored REAS 1.7.0-draft; the log entry of that change records its checks.**

The project was made against REAS 1.4.0-draft. On 2026-11-20 the vendored copy at `standards/reas/` changed to 1.7.0-draft, and the audit below was done against it. The project does not meet every rule of that version: the table says which, and [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) gives the order in which the gaps close. The language review does not establish ASD-STE100 compliance; see the [STE review](/ste-review.md).

## Audit against REAS 1.7.0-draft: 2026-11-20

Method: the audit read the rules in `standards/reas/standard/` and the code at commit `534da18`. It was done in two parts on the same day: the 40 rules of 1.6.0-draft first, from a copy of that version; then, after the copy of 1.7.0-draft, the four rules of the new Native code concept and the sentences that 1.7.0-draft changed in REAS-RUST-004, REAS-SEC-001 and REAS-SEC-004. Step 1 of ADR 0015, the lint table, step 2, the output contexts, step 3, the checked sizes (2026-11-21), and the first part of step 4, the documentation checker (2026-11-21), are done; their rows below describe the code after those changes. A count of lint findings is the number of lines that `cargo clippy --locked --lib --bins --message-format=short -- -W <lint>` printed for `src/` on 2026-11-20 with Rust 1.99.1; test code is not in a count. A status is one of four words:

* Met: the audit found the evidence that the rule's Verification section names.
* Partly met: the code follows the rule's text in the places the audit read, and a named part is absent.
* Not met: the rule applies and the audit found a gap in most of what it asks.
* Not applicable: the rule's Scope excludes this project; the row gives the reason.

A status is the result of a reading, not of a tool, except where the row names a command. A row that says "not examined" is not a pass.

### Core Rust

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-RUST-001 | Partly met | Newtypes with private fields and smart constructors in `src/domain/ids.rs`, `money.rs`, `states.rs`, `list.rs`; `text_enum!` deserializes through `FromStr`; `clippy::wildcard_enum_match_arm` is `deny` in `Cargo.toml`, with one `#[expect]` on the match over the external enum `ureq::Error`. Gap: a rejection test through the deserializer for each invariant was not examined. `cargo semver-checks` is not applicable: the package is not published. |
| REAS-RUST-002 | Met, by reading | Advisory. Use cases take adapters as type parameters; no benchmark is cited as a reason for a choice. |
| REAS-RUST-003 | Met | No blanket implementation and no generic repository, by reading. `clippy::same_name_method` is `deny` in `Cargo.toml`. |
| REAS-RUST-004 | Met | No `Arc<Mutex<T>>` outside test code, by reading. `clippy::needless_pass_by_value` and `clippy::redundant_clone` are `warn` in `Cargo.toml`; one `#[expect]` with its reason, on `Redactor::new`, which takes the terms from its caller. |
| REAS-RUST-005 | Partly met | Typed errors with `thiserror` in the library; `anyhow` only in `src/main.rs`; a failure exit status; `clippy::unwrap_used` is `deny` and `clippy::expect_used` is `warn` in `Cargo.toml`; 12 accepted `expect` calls outside tests, each below `#[expect]` with a reason, of which one is `domain::pattern::fixed`, through which the 35 patterns of the source are compiled. Gap: a test that produces each error variant was not examined. |
| REAS-RUST-006 | Partly met | The commands in `CLAUDE.md`; the Stop hook; the `rust` job. `Cargo.toml` has the `[lints]` table; `clippy::allow_attributes` and `clippy::allow_attributes_without_reason` are `deny`, and no `#[allow]` is left. Gap: the documentation build with warnings denied is in no hook and no job. |
| REAS-RUST-007 | Partly met | Blocking readers run in `spawn_blocking` (`takeout.rs`, `gmail.rs`, `main.rs`), one at a time for a command. Gaps: no shutdown test; cancellation is recorded for the mail readers (the receiver is gone) and not for a shutdown deadline. |
| REAS-RUST-008 | Partly met | `Money` over `rust_decimal`, two decimals, `numeric(10,2)` in PostgreSQL, `checked_add` with `MoneyError::Overflow`, no float. Gaps: the type carries no currency (every amount is a US dollar amount, and the type's comment says so; no check refuses another currency); `Money::saturating_add` and `Money::abs_diff` saturate and return no error; `clippy::float_arithmetic` is not set for the module (0 findings); the three tests the rule names were not examined. |
| REAS-RUST-009 | Not met | 405 public items without a doc comment (`missing_docs`), 38 fallible public functions without an `# Errors` section, 6 without a `# Panics` section. The three lints are not set. No documentation build in CI. |

### State machines

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-STATE-001 | Met, by reading | `ParseState::first_parse` and `reparse` with `TransitionError`; `text_enum!` in `src/domain/states.rs`; tests per transition. |
| REAS-STATE-002 | Partly met | Advisory. Persisted state is a runtime enum, which the rule asks for. Gap: the sender check (the From mailbox and the DKIM pass) is a value on the message, `SenderCheck`, and not a state type that the parser's parameter requires; [ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md) parses a receipt without a DKIM pass on purpose and tags its order. |
| REAS-STATE-003 | Partly met | `PostgresStore::finish_parse` is a conditional update on identifier, state and version, and reports `StoreError::Conflict` when no row changed; `a_stale_version_changes_no_row` in `tests/database.rs` proves a stale writer changes nothing. Gap: no test in which two writers read the same row and exactly one succeeds. |

### Architecture

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-ARCH-001 | Met, by reading | [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md); `src/main.rs` is the one composition root. |
| REAS-ARCH-002 | Partly met | `src/domain` imports no SQLx, file, network or clock type. Gaps: the domain derives `serde::Serialize` (see REAS-DATA-001); one package holds all layers, so no dependency-graph check exists, and none is planned ([ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md) rejects a workspace). |
| REAS-ARCH-003 | Met | The five answers are in [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md); the fifth was added on 2026-11-20. |
| REAS-ARCH-004 | Partly met | Adapters are named for what they adapt. Gaps: `RawMessageStore::known_keys` and `body_by_key` pass a message key as `String` and `&str` although `MessageKey` exists; `MailSource` passes the bytes of a mail as `RawMail`, which is the transport's shape by design (the reader behind `MailReader` is the trust boundary). |
| REAS-ARCH-005 | Met, by reading | Advisory. The rules in `src/domain/recommend.rs` take `as_of` and a seed as arguments. |
| REAS-ARCH-006 | Met, by reading | Advisory. No `dyn Port`. |

### Persistence

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-DATA-001 | Partly met | SQL in `src/adapters/postgres/`; migrations only add; each index names its query; parameterized queries. Gap: domain types carry serde attributes (`#[serde(rename = "product_id")]` in `src/domain/list.rs`, `#[serde(tag, content, rename_all)]` in `src/domain/ids.rs`, `#[serde(transparent)]` on the newtypes) because `qm due --json` serializes them directly. |
| REAS-DATA-002 | Partly met | `GroceryStore::finish_parse` is one port operation whose adapter owns the transaction; no SQLx type crosses a port; the stale-version test asserts that the attempt row rolled back. Gap: no test that fails a later write inside the transaction for a cause other than the version. |
| REAS-DATA-003 | Not met | `compose.yaml` has the named volume `qm_postgres_data`. Gaps: no backup procedure, no backup outside the volume, no restore test. The runbook is milestone 3 of the plan. |
| REAS-DATA-004 | Partly met | `Cargo.lock` committed; `--locked` in CI; `cargo sqlx prepare --check` on a new database in CI; `SQLX_OFFLINE=true`. `rust-toolchain.toml` pins Rust 1.99.1 with Clippy and rustfmt, the `rust` job installs that version and fails when it differs from the file, and the `Dockerfile` pins the image of Rust 1.99 by digest. Gaps: the image names the minor version, so its patch version is not compared with the file; the target and the native dependencies are not recorded in one place. `rust-version` is not applicable (not published); the package has no Cargo features. |
| REAS-DATA-005 | Met, by reading | A mail is the event and its message key is the identifier; the insert is keyed by it; `importing_the_same_export_twice_stores_everything_once` in `tests/database.rs`. `qm fetch` uses the same insert; no test delivers one mail twice through the Gmail adapter. |

### Security at seams

The exposure is in [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md), question 5: other parties can supply outside data, through mail. The exception that the standard gives a tool whose only source of outside data is its maintainer does not apply.

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-SEC-001 | Partly met | `SecretString` for the database URL, the OAuth client secret and the Gmail tokens; a filter keeps the HTTP library and the HTML parser from printing bytes of a message or a token ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)); a timeout, a size limit for each call and a limit on the messages of a run in the Gmail adapter; `cargo deny check` and `cargo audit` on each pull request and each push to `main`; `deny.toml` admits one registry and no git source; no listener and no endpoint. Gaps: no daily schedule for the scan; no recorded review of the dependencies with a build script (50 packages in the lockfile) or a proc-macro (24); no list of the dependencies that read outside data and contain `unsafe`, and no review of them; no design review recorded in the four terms the rule names for the redactor and the Gmail adapter, which handle personal data and credentials. |
| REAS-SEC-002 | Partly met | Mail is decoded, the sender is checked, and the text is redacted in `PlatformMail` before the application stores it; the receipt parser turns stored text into domain types (`Money`, `Quantity`, `OrderNumber`) and treats it as outside data. Gaps: a receipt without a DKIM pass is parsed ([ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md)); bare `String` keys cross `RawMessageStore`; the `Deserialize` derives in `src/adapters/config.rs`, `redact.rs` and `gmail.rs` were not examined for invariants or for `deny_unknown_fields`. |
| REAS-SEC-003 | Partly met | The `regex` crate only, which is linear-time. `domain::pattern::SIZE_LIMIT`, 1 MiB, is the `size_limit` of each of the 35 patterns of the source, which `domain::pattern::fixed` compiles, and of each pattern that the redactor builds from a configured term or number; a term whose pattern is larger is refused by its position when the redactor is built. The largest pattern of the source compiled to about 160 kB on 2026-11-21, and the patterns of a term of 60 characters to about 140 kB. `clippy::arithmetic_side_effects`, `clippy::indexing_slicing` and `clippy::string_slice` are `deny` for the whole package in `Cargo.toml`, with no finding and no `#[expect]`; `clippy.toml` leaves an index free in test code. `overflow-checks = true` in the release profile. The position of a median is computed in one function, `middle` in `src/domain/recommend.rs`; a row of a receipt is read by slice patterns in `classify`. Tests at the bounds of a type: `a_sum_outside_the_range_of_the_type_gives_no_median_or_the_mean_and_no_panic`, `a_setting_at_the_bound_of_its_type_changes_a_status_and_does_not_stop_the_rule`, `amounts_at_the_ends_of_the_range_give_an_answer_and_no_panic`, `a_row_with_fewer_cells_than_a_rule_reads_matches_no_rule`, `a_term_whose_pattern_passes_the_size_limit_is_refused_by_its_position`. Gaps: no fuzz target (REAS-TEST-003, step 7 of ADR 0015). The lints do not cover the index of a capture group. Each one was read on 2026-11-21: every indexed group takes part in each match of its pattern. Three tests named `every_group_that_the_code_indexes_takes_part_in_every_match` hold that for the patterns where the engine can say it; for the two patterns of `src/domain/pack.rs` and the pattern of the card digits, which have one optional group that the code does not index, the reading is the evidence. The code of the project reads no declared length from outside data (the archive reader and the mail library do), so the rule's test of a declared length has no place in this package. |
| REAS-SEC-004 | Met | `unsafe_code = "forbid"` under `[lints.rust]` in `Cargo.toml`, for every target of the package; no unsafe code. The two Clippy lints of the rule are `deny`. |
| REAS-SEC-005 | Not applicable | No fast path, cache or second implementation of one function. The habit rule and the gap rule are different rules with different results. |
| REAS-SEC-006 | Met | The [application overview](/concepts/application.md) lists each output context with its encoder and its tests: the terminal, a log line, JSON, HTML, SQL, a URL, an HTTP header and a file name. Deviation, kept on purpose: the redactor changes a mail before storage ([ADR 0004](/decisions/0004-redaction-before-storage.md)), where the rule says to store a value as it was entered; the reason is privacy, and the stored text is still encoded at each context. Limits: the text of a panic is not encoded; the page of `qm due --html` has the encoding of HTML only, also when it is printed to a terminal and not to a file; the error code of an answer of Google is cut to the characters of an identifier when it is read, in `src/adapters/mail/gmail.rs`, and not at the sink. |

### Native code

The project writes no native code. Its dependencies hold some, on paths that outside data reaches. The exception for the CI jobs of REAS-NATIVE-002 and REAS-NATIVE-004 does not apply, for the same reason as above.

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-NATIVE-001 | Partly met | Advisory. No change has added native code to the project. Gap: nothing records the native code of the dependencies. Known from `cargo metadata` for Linux on x86_64: the TLS library uses `ring`, which has C and assembly behind a `links` key; every byte from the Gmail API passes it, and the database connection when it uses TLS. The unsafe code of the mail parser, the HTML parser, the archive reader, the pattern engine and the crates they use was not counted; a search of their sources by the security review found the word in each, which is not a count. |
| REAS-NATIVE-002 | Not met | The count of the package is zero by `unsafe_code = "forbid"` in `Cargo.toml`; no committed baseline records that zero. No count exists for the dependencies on a path of outside data: `cargo geiger` 0.13.0 is on the development machine and has not run on this project. No CI job. |
| REAS-NATIVE-003 | Met | No unsafe code; `unsafe_code = "forbid"` under `[lints.rust]` in `Cargo.toml`, for every target of the package; `clippy::undocumented_unsafe_blocks` and `clippy::multiple_unsafe_ops_per_block` are `deny`. |
| REAS-NATIVE-004 | Not met | No fuzz target for a parser of outside bytes (REAS-TEST-003). No module holds unsafe code, so no Miri job is due. The path through `ring` is Miri-blind; this row is the only place that says so, and no sanitizer build exists. |

### Validation and evidence

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-TEST-001 | Partly met | Unit tests in each module; `tests/database.rs` against PostgreSQL; a failing double in `src/application/migrate.rs` and `import.rs`; one property test in `src/domain/identity.rs`. Gaps: `cargo mutants` has not run; the tests were not examined for the non-vacuity rules or for the three values at each threshold. No sealed state trait exists, so no compile-fail test is due. |
| REAS-TEST-002 | Partly met | The log and this file name the date and the commit or run of each result. Gap: results before 2026-11-20 use "passed" and "not run" only; the six outcomes are used from this entry on. |
| REAS-TEST-003 | Not met | No fuzz target. The adapters and parsers that read bytes from another party: the Takeout reader, the MIME reader, the classifier, the redactor, the FreshDirect parser, the size-text reader in `src/domain/pack.rs`, and the JSON of the Gmail adapter. |
| REAS-TEST-004 | Partly met | The doubles in `src/application/migrate.rs` and `import.rs` stand for the database and the mail source. Gap: `MailReader` and `ReceiptParsing` are traits in front of code this application owns (the classifier with the redactor, and the parser registry), and `ByteReader` in `src/application/import.rs` is a double of the first. |
| REAS-TEST-005 | Met, by record | No test was skipped, weakened or deleted to pass; the log names each test that a review added. No commit message names a class, because no failing test was changed. |

### Documentation

| Rule | Status | Evidence or gap |
| --- | --- | --- |
| REAS-DOC-001 | Partly met | The text targets ASD-STE100. The [STE review](/ste-review.md) says that nobody has checked it against the official rules and dictionary. |
| REAS-DOC-002 | Partly met | The bundle in `docs/` with its index. Since 2026-11-21 `scripts/check_docs.py` ([ADR 0016](/decisions/0016-the-documentation-checker-follows-the-checker-of-reas.md)) checks, for each concept, the front matter, an `updated` date (present, a day as `YYYY-MM-DD`, not after today), the actor and the time of each event, and that the index reaches the concept through links in the bundle; for each document, the local links and the heading fragments. A concept is each file of `docs/` except the log and a file named `index.md`. The checker reports a concept that is older than the threshold of its type as a warning, and `--strict-freshness` makes the warning an error. 26 concepts got their date on 2026-11-21. For 25 it is the day of the last commit that changed the file, which is the last day that somebody wrote the page against the code; nobody read those 25 pages again on that day. The terminology has the day of this change, which added its table of retired terms. Gap: no file has an authority class, and no page lists the classes. |
| REAS-DOC-003 | Met | [Project terminology](/terminology.md) with its table of retired terms, 2 rows on 2026-11-21. The checker fails on a retired term in the text that a reader sees outside code, in each document except the table itself, the log, the changelog, the language review and the vendored standard. It searches plain text: [Develop and run](/runbooks/develop.md) lists what it cannot find, such as a term with an invisible character in it. Whether a change brings a term that the table does not define is a question for the review of that change. |
| REAS-DOC-004 | Partly met | The log, the changelog and the concept change with the code, by `.claude/rules/docs.md`. Gap: the decisions before ADR 0015 state their requirements as prose, not in the four shapes. |
| REAS-DOC-005 | Met | `scripts/check_docs.py` and its tests in the `docs` job; the language review has its own record. |
| REAS-DOC-006 | Not met | The checks are listed in `CLAUDE.md`, in the workflow and in the Stop hook, three lists that can drift; the Stop hook starts the documentation checker for fewer paths than the checker reads. No table of paths to jobs. The documentation checker has a floor for each gate since 2026-11-21 (it fails when it found no concept, no local link, no fragment, no retired term or no rule declaration, and its tests hold the counts of the repository) and a planted violation for each gate in `tests/docs/test_docs.py`, and five tests read the exit status and the output of the command's entry point. The guard hook has its own test file. The other gates have neither: the lint levels, the privacy canary and its exclusions, `cargo deny` and `cargo audit` with their ignore lists. |

## Checks of 2026-11-13

The checks below ran on the scaffold worktree and on milestone 1 before their commits, with Rust 1.99.1 and a user-owned PostgreSQL 15.16 on port 5433 as `DATABASE_URL`. They are a record of that day against REAS 1.4.0-draft. They do not describe the code of today; the log has the checks of each later change.

| Check | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | Passed locally. |
| `cargo check --all-targets` | Passed locally, no warnings. |
| `cargo clippy --all-targets -- -D warnings` | Passed locally, no warnings. |
| `cargo test --lib` | Scaffold: 16 unit tests. Milestone 1: 71 unit tests, including the parser snapshot derived from redacted output and the redaction tests over the synthetic fixture and the review's bypass cases. |
| `cargo test --test database` | Scaffold: 3 integration tests. Milestone 1: 6, each in a fresh database on the port 5433 server; import twice stores once, the gap rule reads the stored purchases, a stale version changes no row. |
| `qm migrate`, run twice | First run applied `1 raw mail` and `2 grocery`; second run applied nothing and reported 2 already applied. |
| `cargo sqlx prepare --check -- --all-targets` | Passed locally; the scaffold has no checked query yet, so no `.sqlx/` directory exists. |
| `cargo deny check` | Passed locally: advisories, bans, licenses, sources. |
| `cargo audit --file Cargo.lock` | Passed locally with RUSTSEC-2026-0459 ignored; `.cargo/audit.toml` states the reason, and `cargo tree -i libsqlite3-sys -e normal,build` printed nothing. |
| `python3 scripts/check_docs.py` | Passed locally: OKF metadata, unique rule IDs, internal links. |
| `python3 -m unittest discover -s tests/docs -t .` | Passed locally, 6 tests. |
| `scripts/pii_canary.sh` | Passed locally over the staged files, shape mode, after the fail-closed rewrite; milestone 1 narrowed the exclusions to fictional shapes and a probe with a real-looking number still fails it. |
| `bash .claude/hooks/guard-bash.test.sh` | Passed locally; also a step of the `privacy` CI job. |
| `qm import`, `qm due`, `qm fixture add` end to end | Ran 2026-11-13 against the development database with a six-message synthetic export; the log entry of the day records the counts. |
| `/security-review` over the staged scaffold | Ran 2026-11-13: no finding at the reporting threshold; three hardening notes applied before the commit, recorded in the log entry of the day. |
| `/security-review` over milestone 1 | Ran 2026-11-13: one Medium finding (a redaction bypass), fixed before the commit with regression tests; two filtered candidates fixed as hardening; recorded in the log entry of the day. |
| Complete STE vocabulary and grammar review | Not run. See the review record. |

## Limits

The checker's metadata checks do not establish the truth of verification claims. The audit of 2026-11-20 is one reading by the assistant that wrote most of the code; nobody else has checked it. A status of "Met, by reading" has no test or lint behind it. The counts of lint findings describe commit `534da18` and change with the code. On 2026-11-20 four violations planted in a scratch copy of `src/lib.rs` (an `#[allow]`, an `unsafe` block, a wildcard arm, an `unwrap`) each failed `cargo clippy -- -D warnings`; the copy was not committed, so no committed test proves that the lints stay on.
