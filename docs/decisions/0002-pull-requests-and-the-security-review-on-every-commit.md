---
type: Architecture Decision
title: Pull requests into main and the security review on every commit
description: How every commit that reaches main is checked, which jobs run, what runs on the maintainer's machine before a commit, and why the AI review action is not used.
tags: [decision, ci, security, process]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
---

# ADR 0002 — Pull requests into main and the security review on every commit

## Context

The maintainer asked for a security review on every commit, run through GitHub Actions, and chose pull requests with required checks over direct commits to `main`. The repository may become public, so the review must also guard against personal data reaching git. REAS-SEC-001 asks for a dependency vulnerability scan; REAS-DATA-004 asks for `--locked` builds and checked-query metadata that matches the migrations; REAS-TEST-002 asks that a passed check be recorded with its date and commit.

## Decision

* Nothing is committed to `main` directly. Every change is a branch and a pull request. Branch protection requires the jobs below before a merge and forbids force pushes and deletion; that is the control. The guard hook on the maintainer's machine refuses the common forms of a push to `main` and of a forced push as a best-effort backstop, and the first commit, which created `main` through a renamed `bootstrap` branch so that no push ever targeted `main`, is the one exception.
* `.github/workflows/ci.yml` runs on every pull request and on the push of the merge commit to `main`. Every action is pinned to a commit, with the tag in a comment. Jobs get a read-only token.

| Job | What it checks | Fails on |
| --- | --- | --- |
| `rust` | `cargo fmt --check`, `cargo check --all-targets --locked`, `cargo clippy -D warnings`, migrations applied to an empty PostgreSQL 17, `cargo sqlx prepare --check`, `cargo test --locked` | any |
| `audit` | `cargo audit --file Cargo.lock` against the RustSec database | a vulnerability not ignored in `.cargo/audit.toml` with a reason |
| `deny` | `cargo deny check`: advisories, licenses on an allow list, bans, sources limited to crates.io | any |
| `secrets` | gitleaks over the full history | any secret |
| `privacy` | `scripts/pii_canary.sh` in shape mode: no tracked file under `tests/fixtures/local/` or `backups/`, no personal-data shape in tracked files; fails closed on a file it cannot read | any match or scan error |
| `docs` | `scripts/check_docs.py` and its tests | a structural error in the bundle |
| `dependency-review` | GitHub's dependency review on pull requests | a new dependency with a moderate or worse advisory |

* Before each commit, on the maintainer's machine: the Stop hook runs `cargo check`, the docs checker and the privacy canary in both modes; the maintainer or the assistant runs the Claude Code `/security-review` skill over the pending changes and records its findings and their resolution in the day's log entry. The rule is literal (decided 2026-11-14 after a three-line follow-up commit went in without one): every commit, including documentation-only changes and changes to the canary or the hooks; a fix the review asks for goes into the same commit; a follow-up commit gets its own review. The bootstrap commit that created `main` is the one commit without a review, because no base existed to review against.
* Dependabot opens weekly pull requests for Cargo and for the actions. `#![forbid(unsafe_code)]` in the crate root.

## Consequences

* Every commit on `main` has passed all seven jobs; the Actions run is the evidence the self-audit cites.
* A solo project pays the ceremony of a pull request per change. The maintainer chose that over direct commits.
* The term-mode canary cannot run in CI, since CI must not know the maintainer's address. It runs locally, and the synthetic fixtures carry fake data of the same shapes so a redactor regression is caught in CI anyway.
* The audit ignore for RUSTSEC-2026-0459 is a standing exception with a stated exit condition; Dependabot will surface the sqlx release that removes it.
* The guard hook is a pattern match over command text and cannot be complete. The first security review listed command variants that walk past it; the patterns were widened, and the documents name the Read denies, the permission prompts and branch protection as the controls it backs up.

## Alternatives

* The `anthropics/claude-code-security-review` GitHub Action, an AI review of each pull request. Not adopted: it needs an API key in repository secrets and sends every diff to Anthropic. The `/security-review` skill run locally gives the same review without either. The decision can be revisited.
* Direct commits to `main` with CI on push, as nbastats does. Rejected by the maintainer in favour of a hard gate.
* CodeQL for Rust. Not adopted yet; it is enabled when GitHub offers it for Rust on this repository's plan.
