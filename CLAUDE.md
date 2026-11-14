# Claude instructions

This project follows the [REAS standard](https://github.com/abenamer/reas). Its text is vendored at [standards/reas/standard/index.md](standards/reas/standard/index.md), and the rules for Rust load from `.claude/rules/rust.md` when you touch Rust. Start at [docs/index.md](docs/index.md).

@AGENTS.md

# Commands

- Format, check, lint: `cargo fmt --all`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`
- Unit tests, no database: `cargo test --lib`; one module: `cargo test --lib <module>`
- Integration tests: `DATABASE_URL=... cargo test --test database`, each test in a fresh database (see [docs/runbooks/develop.md](docs/runbooks/develop.md))
- Supply chain: `cargo deny check`, `cargo audit --file Cargo.lock`
- Privacy canary: `scripts/pii_canary.sh` (shapes), `scripts/pii_canary.sh --terms ~/.config/qrtrmastr/canary-terms.txt` (your real terms)
- Docs: `python3 scripts/check_docs.py` and `python3 -m unittest discover -s tests/docs -t .`
- Run: `qm migrate`, later `qm import --mbox <path>`, `qm due`, `qm cart`

# Commit protocol (ADR 0002)

1. Work on a branch from `main`. Nothing is committed to `main` directly; branch protection enforces it, and the guard hook refuses the common forms of such a push as a backstop.
2. Before each commit, run the `/security-review` skill over the pending changes and record its findings and what was done about them in that day's entry of [docs/log.md](docs/log.md), even when it found nothing. The rule is literal: every commit, including a one-line follow-up, a documentation-only change and a change to the privacy canary or the hooks. A fix the review asks for goes into the same commit, and a follow-up commit gets its own review. Two commits were made without one: the bootstrap commit that created `main`, before a base existed to review against, and a three-line canary follow-up on 2026-11-13 that was reviewed retroactively the next day; neither is a precedent.
3. Run every command above that applies to the change, and report only the ones that ran.
4. Push the branch and open a pull request. Merge only when all CI jobs pass. The seven jobs are required by branch protection.
5. A Dependabot pull request is never merged as Dependabot left it: its commit had no review, and the seven jobs are a toolchain, not the review. Redo the bump on a branch from `main` under points 1 to 4: check that the pinned commit is the one the tag names on the source repository (`git ls-remote --tags`; for an annotated tag the `^{}` line is the commit) and that it matches what Dependabot proposed, read the release notes between the two versions for changed inputs, permissions or behaviour, then review, log and open the pull request. The tag check shows what the tag names today, not that the tag is honest; the release notes and the match with Dependabot's proposal are the stronger evidence. Dependabot closes its own pull request once `main` carries the version. Push nothing to a Dependabot branch; Dependabot stops maintaining a branch that someone else has changed. Version updates arrive grouped once a month; a security update arrives on its own and is handled the same way.

# Conventions

- Layout: `src/domain` (pure), `src/application` (use cases and ports), `src/adapters` (PostgreSQL, mail, CLI), `src/main.rs` (composition root). Add a port only at a genuine boundary, name the concrete adapter, add no forwarding layers.
- Lifecycles are enums with explicit transitions (`domain::states`). Use the identifier newtypes in `domain::ids`; never pass a bare `i64` or `String` key.
- Every change updates the OKF bundle in `docs/` the same day: the log, the changelog and the concept or reference it touches, in the words of [docs/terminology.md](docs/terminology.md). ASD-STE100 Issue 9 is a target; never claim verified compliance.
- A recommendation rule or parser version is backtested or fixture-tested and recorded in an ADR before it is the default.
- Report only the checks you executed, with their output.

# Privacy

- Real receipts never enter git, even redacted; the repository may become public. Redacted real fixtures live in `tests/fixtures/local/` (gitignored). CI runs on `tests/fixtures/synthetic/` only.
- Never print a raw mail export, `.mbox` or `.eml` file, or anything under the Takeout directory: they carry the maintainer's address and phone. The Read deny and the permission prompts are the controls; the guard hook refuses the common forms as a backstop. Inspect redacted rows in the database instead.
- Never log a message body, a redaction term or a database URL.

# Gotchas

- Migrations only add. A check constraint that lists values is named `<table>_<column>_check` and matches a `text_enum!` in `domain::states`.
- `cargo audit` ignores RUSTSEC-2026-0459 with the reason in `.cargo/audit.toml`; the SQLite crate is in the lockfile through sqlx and is never compiled.
- The hooks in `.claude/settings.json` format Rust after each edit and run the checks and the privacy canary before a turn ends; reading `.env`, the local fixtures, `backups/` or the mail export is denied there, and the guard hook refuses the common forms of `docker compose down -v`, `docker volume rm`, a forced push, a push to main, and a command that prints an export. The hook is best effort; the denies, the prompts and branch protection are the controls.
