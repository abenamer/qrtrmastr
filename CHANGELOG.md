# Changelog

## 0.1.0 — unreleased

- Scaffold (milestone 0): the Cargo package `qrtrmastr` with the binary `qm`, the REAS layout (`src/domain`, `src/application`, `src/adapters`, `src/main.rs`), identifier newtypes, exact money and quantities, the stored labels and the `ParseState` lifecycle, the `SchemaStore` port and the `Migrate` use case, `PostgresStore`, and `qm migrate`. Migrations `0001_raw_mail.sql` and `0002_grocery.sql` create the MVP schema. CI with seven required jobs (format, check, clippy, migrations, checked queries, tests; audit; deny; gitleaks; privacy canary; docs; dependency review), Dependabot, the Compose file with a named volume, and the Claude Code setup. [ADR 0001](docs/decisions/0001-ports-and-adapters-for-a-private-tool.md) and [ADR 0002](docs/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).
