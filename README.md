# Quartermaster

A personal tool that turns FreshDirect receipt emails into a PostgreSQL order history and recommends the next cart. It runs on the maintainer's machine. It is written in Rust (Tokio, SQLx) and follows the [REAS engineering standard](https://github.com/abenamer/reas), vendored at `standards/reas/`.

The MVP reads a one-time Google Takeout export of the receipts, redacts personal data before anything is stored, parses each receipt into an order with its lines, resolves renamed products through aliases, and recommends items whose usual gap between purchases has elapsed. It places no orders and talks to no retailer.

The documentation is an [Open Knowledge Format bundle](docs/index.md) that targets ASD-STE100 Simplified Technical English. Full STE compliance is not verified; see the [self-audit](docs/self-audit.md).

## Privacy

Real receipts never enter this repository, even redacted; it may become public. The redaction terms and the mail export live outside it. See [Privacy](docs/concepts/privacy.md).

## Quick start

Nothing is installed on the host; the database and the command run as containers.

1. `cp .env.example .env` and set `POSTGRES_PASSWORD`.
2. `docker compose up -d` starts the database; `docker compose build qm` builds the command.
3. `docker compose run --rm qm migrate`
4. Export your FreshDirect mail and write your redaction terms as [the runbook](docs/runbooks/takeout-export.md) describes, then `docker compose run --rm qm import --mbox /data/<path>` and `docker compose run --rm qm due`.

The [command line reference](docs/reference/cli.md) lists every command. [Develop and run](docs/runbooks/develop.md) covers the toolchain and the checks.

## Checks

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo clippy --all-targets -- -D warnings
    cargo test --lib
    DATABASE_URL=... cargo test --test database
    cargo deny check
    cargo audit --file Cargo.lock
    python3 scripts/check_docs.py
    scripts/pii_canary.sh

Every pull request runs these in GitHub Actions; see [ADR 0002](docs/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).

## License

MIT. The vendored REAS text is CC BY 4.0 and its examples MIT; see `standards/reas/`.
