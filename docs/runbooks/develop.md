---
type: Runbook
title: Develop
description: Toolchain, database, checks, and the pull-request flow for a change.
tags: [runbook, development]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: compose
    resource: ../../compose.yaml
    title: Compose file
  - id: ci
    resource: ../../.github/workflows/ci.yml
    title: CI workflow
---

# Develop

## Prerequisites

* Rust stable with `rustfmt` and `clippy` (`rustup component add rustfmt clippy`).
* `cargo install sqlx-cli --no-default-features --features postgres,rustls --locked`.
* `cargo install cargo-audit cargo-deny --locked`.
* Python 3 with `pip install -r requirements-docs.txt` for the docs checker.
* A PostgreSQL 15 or newer. Either of the two options below.

## Database option A: Docker Compose

1. `cp .env.example .env` and set `POSTGRES_PASSWORD`.
2. `docker compose up -d`. The service listens on `127.0.0.1:5434` (`QM_DB_PORT`), and keeps its data in the named volume `qm_postgres_data`.
3. Set `DATABASE_URL=postgres://qm:<password>@127.0.0.1:5434/qrtrmastr` in `.env`.

Never run `docker compose down -v`: it deletes the volume. The guard hook refuses its common forms as a backstop; the rule is yours to keep.

## Database option B: a user-owned local server

When Docker is not running, a PostgreSQL cluster owned by your user works for development and tests. It needs no sudo.

1. `/usr/lib/postgresql/15/bin/initdb -D ~/qm-data/pgdev -U $USER --auth=trust --encoding=UTF8`
2. `/usr/lib/postgresql/15/bin/pg_ctl -D ~/qm-data/pgdev -o "-p 5433 -k /tmp -c listen_addresses=127.0.0.1" -l ~/qm-data/pgdev.log start`
3. `psql -h 127.0.0.1 -p 5433 -d postgres -c "create database qrtrmastr"`
4. `DATABASE_URL=postgres://$USER@127.0.0.1:5433/qrtrmastr`

Expected result: `psql -h 127.0.0.1 -p 5433 -d qrtrmastr -c "select 1"` prints `1`. Stop it with `pg_ctl -D ~/qm-data/pgdev stop`. Trust authentication on loopback is acceptable only for a single-user development machine.

## Make a change

1. Branch from `main`: `git switch -c <topic> main`.
2. Change the code and the documents it touches in the same commit (the log, the changelog, the concept or reference).
3. Run the checks:

       cargo fmt --all
       cargo check --all-targets
       cargo clippy --all-targets -- -D warnings
       cargo test --lib
       DATABASE_URL=... cargo test --test database
       cargo deny check
       cargo audit --file Cargo.lock
       python3 scripts/check_docs.py
       python3 -m unittest discover -s tests/docs -t .
       scripts/pii_canary.sh

   Expected result: every command exits 0. The integration tests create a fresh database per test from `DATABASE_URL` and drop it after.
4. Run the `/security-review` skill in Claude Code over the pending changes. Record the result in the log entry of the day.
5. Commit, push the branch, and open a pull request: `gh pr create --fill`.
6. Merge when all CI jobs pass. The jobs are listed in [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).

## Recover

* A failed migration leaves `_sqlx_migrations` with `success = false` for that version; fix the SQL and run `qm migrate` again.
* To start the development database over: `psql -h 127.0.0.1 -p 5433 -d postgres -c "drop database qrtrmastr"` then step 3 above. Never do this to the Compose volume without a backup.
