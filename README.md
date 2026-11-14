# Quartermaster

A personal tool that turns FreshDirect receipt emails into a PostgreSQL order history and recommends the next cart. It runs on the maintainer's machine. It is written in Rust (Tokio, SQLx) and follows the [REAS engineering standard](https://github.com/abenamer/reas), vendored at `standards/reas/`.

The MVP reads a one-time Google Takeout export of the receipts, redacts personal data before anything is stored, parses each receipt into an order with its lines, resolves renamed products through aliases, and recommends items whose usual gap between purchases has elapsed. It places no orders and talks to no retailer.

The documentation is an [Open Knowledge Format bundle](docs/index.md) that targets ASD-STE100 Simplified Technical English. Full STE compliance is not verified; see the [self-audit](docs/self-audit.md).

## What you need: a Google Takeout export of your FreshDirect mail

FreshDirect has no public API and this tool does not scrape its site, so your receipts come from your own mailbox. The tool does not log in to Gmail either: you export the mail once with Google Takeout and give the tool the file. Nothing leaves your machine.

1. In Gmail, create a filter for `from:service@freshdirect.com` that applies a new label, `FreshDirect`, and tick "Also apply filter to matching conversations" so the old receipts get the label too.
2. Open <https://takeout.google.com>. Deselect everything, select Mail, choose "All Mail data included", deselect everything there and select only the label `FreshDirect`. Request one export as a .zip. Google takes hours to a day to build it and sends you a mail when it is ready.
3. Download the archive and unzip it into your exports directory, `~/qm-data/` by default, for example `~/qm-data/takeout-2026-11/`. The export is the file `Takeout/Mail/FreshDirect.mbox` inside it. It carries your name, address, phone and card digits: it never enters this repository, and the tool redacts every message before it stores anything.
4. Write your redaction terms into `~/.config/qrtrmastr/config.toml`: your name, email, phone, each line of your delivery address and the last four digits of your cards, in a `[redaction]` section. The import refuses to run without them, and its error prints the section to add.

The runbook [Export your FreshDirect mail and import it](docs/runbooks/takeout-export.md) has the same steps with the expected output. Repeating the export later and importing it again adds only the new messages.

## Privacy

Real receipts never enter this repository, even redacted; it may become public. The redaction terms and the mail export live outside it. See [Privacy](docs/concepts/privacy.md).

## Quick start

Nothing is installed on the host; the database and the command run as containers. Start the Takeout export first, since Google needs time to build it.

1. `cp .env.example .env` and set `POSTGRES_PASSWORD`.
2. `docker compose up -d` starts the database; `docker compose build qm` builds the command.
3. `docker compose run --rm qm migrate`
4. With the export unzipped and the terms written as above: `docker compose run --rm qm import --mbox /data/takeout-2026-11/Takeout/Mail/FreshDirect.mbox`, then `docker compose run --rm qm due`. Inside the container your exports directory is `/data` and your configuration directory is `/config`.

`docker compose run --rm qm --help` repeats what the tool needs. The [command line reference](docs/reference/cli.md) lists every command. [Develop and run](docs/runbooks/develop.md) covers the toolchain and the checks.

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
