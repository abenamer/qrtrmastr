# Quartermaster

A personal tool that turns FreshDirect receipt emails into a PostgreSQL order history and recommends the next cart. It runs on the maintainer's machine. It is written in Rust (Tokio, SQLx) and follows the [REAS engineering standard](https://github.com/abenamer/reas), vendored at `standards/reas/`.

The MVP reads a one-time Google Takeout export of the receipts, redacts personal data before anything is stored, parses each receipt into an order with its lines, resolves renamed products through aliases, and recommends items whose usual gap between purchases has elapsed. It places no orders and talks to no retailer.

The documentation is an [Open Knowledge Format bundle](docs/index.md) that targets ASD-STE100 Simplified Technical English. Full STE compliance is not verified; see the [self-audit](docs/self-audit.md).

## Privacy

Real receipts never enter this repository, even redacted; it may become public. The redaction terms and the mail export live outside it. See [Privacy](docs/concepts/privacy.md).

## Quick start

1. Create a database and set `DATABASE_URL` as [Develop](docs/runbooks/develop.md) describes.
2. `cargo run -- migrate`

The import, review and recommendation commands arrive with the next milestones; the [command line reference](docs/reference/cli.md) lists what exists.

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
