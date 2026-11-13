---
type: Audit Report
title: REAS self-audit
description: Revision-specific validation evidence and remaining limitations.
tags: [quality, documentation, audit]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: reas
    resource: https://github.com/abenamer/reas
    title: REAS engineering standard, 1.4.0-draft
---

# REAS self-audit

**Recorded validation: Actions run [43492663243](https://github.com/abenamer/qrtrmastr/actions/runs/43492663243) passed all seven jobs for the scaffold commit `85bcbc0` on pull request #1 (2026-11-13). Milestone 1 (`import-freshdirect`) has local evidence below; its pull request supplies the remote run.**

The project was made against REAS 1.4.0-draft, the version vendored at `standards/reas/`. The language review does not establish ASD-STE100 compliance; see the [STE review](/ste-review.md).

## Local evidence: 2026-11-13

The checks below ran on the scaffold worktree before its first commit, with Rust 1.99.1 and a user-owned PostgreSQL 15.16 on port 5433 as `DATABASE_URL`. A local result has no immutable source commit and is not release evidence; the pull request's Actions run replaces this table.

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

## Rules and where their evidence lives

| Rule | Evidence |
| --- | --- |
| REAS-RUST-001 | `src/domain/ids.rs`, `src/domain/money.rs`, `src/domain/states.rs`: private fields, smart constructors, exhaustive matches; their unit tests. |
| REAS-RUST-005 | `src/main.rs` returns a failure exit status; `Settings::from_env` has no default for `DATABASE_URL`. |
| REAS-RUST-006 | The commands in `CLAUDE.md`; the Stop hook runs `cargo check` before a turn ends. |
| REAS-STATE-001 | `ParseState::first_parse` and `reparse` with `TransitionError`; tests per transition. |
| REAS-STATE-003 | `PostgresStore::finish_parse`: the transition is `update ... where id = $3 and parse_state = $4 and version = $5`, success only when one row changed, else `StoreError::Conflict` and the transaction rolls back; `a_stale_version_changes_no_row` proves it. |
| REAS-ARCH-001 to 006 | [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md); `src/application/ports.rs`; `src/main.rs`. |
| REAS-DATA-001 | Every index in `migrations/` names the query it serves; constraints named `<table>_<column>_check`. |
| REAS-DATA-002 | `GroceryStore::finish_parse` is one port operation; the adapter owns the transaction that writes the attempt, the order, its lines and anomalies, and the state. No SQLx type crosses the port. |
| REAS-DATA-003 | `compose.yaml` with the named volume `qm_postgres_data`; the backup and restore runbook is milestone 3. |
| REAS-DATA-004 | `Cargo.lock` committed; `--locked` in CI; `cargo sqlx prepare --check` in CI; `SQLX_OFFLINE=true`. |
| REAS-TEST-001 | `tests/database.rs` against a real PostgreSQL; a failing double in `src/application/migrate.rs`. |
| REAS-TEST-002 | This file and the log name the date and, once pushed, the commit of each result. |
| REAS-SEC-001 | `cargo audit` and `cargo deny` in CI; `.env` denied to the assistant; `SecretString` for the database URL; `#![forbid(unsafe_code)]`; external mail is validated before the domain sees it (sender, DKIM pass, redaction with an assertion; parsers refuse unverified mail); every query is parameterized. |
| REAS-DOC-001 to 005 | This bundle; `scripts/check_docs.py` and its tests in CI. |

## Limits

The checker's metadata checks do not establish the truth of verification claims. Nothing has run in GitHub Actions yet; the first pull request supplies the remote evidence, and this file is updated with its run and commit.
