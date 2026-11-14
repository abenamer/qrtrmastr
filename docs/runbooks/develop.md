---
type: Runbook
title: Develop and run
description: Running qm through Docker Compose, the toolchain for development, the database options, the checks, and the pull-request flow.
tags: [runbook, development, docker]
generated: { by: "process:claude-code", at: "2026-11-13T23:30:00Z" }
sources:
  - id: compose
    resource: ../../compose.yaml
    title: Compose file
  - id: dockerfile
    resource: ../../Dockerfile
    title: Image build
  - id: ci
    resource: ../../.github/workflows/ci.yml
    title: CI workflow
---

# Develop and run

## Run it: Docker Compose

Nothing of the project is installed on the host. The database and the `qm` command run as containers, as nbastats does.

1. `cp .env.example .env` and set `POSTGRES_PASSWORD`. Set `QM_DATA_DIR` and `QM_CONFIG_DIR` if your Takeout exports and your `config.toml` are not under `~/qm-data` and `~/.config/qrtrmastr`.
2. `docker compose up -d`. This starts the database only; the `qm` service has a profile and does not start by itself. The database listens on `127.0.0.1:5434` (`QM_DB_PORT`) and keeps its data in the named volume `qm_postgres_data`.
3. `docker compose build qm`. The first build compiles every dependency and takes several minutes; later builds reuse the layers.
4. Run commands with `docker compose run --rm qm <command>`:

       docker compose run --rm qm migrate
       docker compose run --rm qm import --mbox /data/<path inside your exports directory>
       docker compose run --rm qm due
       docker compose run --rm qm fixture add <key> --dir /fixtures

   Expected result: the same output the native binary gives ([Command line](/reference/cli.md)). `--rm` removes the container when the command ends; the data stays in the database volume. Before the first `fixture add`, run `mkdir -p tests/fixtures/local` on the host: a bind mount Docker creates itself belongs to root and the container cannot write to it.

Inside the container, your exports directory is `/data` (read-only), your configuration directory is `/config` (read-only), and `tests/fixtures/local` of the repository is `/fixtures`. The container has a read-only root file system, no capabilities, and runs as an unprivileged user.

Never run `docker compose down -v`: it deletes the volume. The guard hook refuses its common forms as a backstop; the rule is yours to keep.

## Develop: toolchain

* Rust stable with `rustfmt` and `clippy` (`rustup component add rustfmt clippy`).
* `cargo install sqlx-cli --no-default-features --features postgres,rustls --locked`, `cargo install cargo-audit cargo-deny --locked`: toolchain utilities, not project binaries.
* Python 3 with `pip install -r requirements-docs.txt` for the docs checker.
* The project's own binary is never installed on the host: `cargo build` puts it under `target/`, and `docker compose run --rm qm` runs it.

## Develop: a database for the tests

The integration tests create a fresh database per test from `DATABASE_URL`, so they need a server where that user may create databases. Two options.

### Option A: the Compose database

With `docker compose up -d` running: `DATABASE_URL=postgres://qm:<password>@127.0.0.1:5434/qrtrmastr`. The `qm` user owns the database and may create test databases.

### Option B: a user-owned local server

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
       bash .claude/hooks/guard-bash.test.sh

   Expected result: every command exits 0.
4. Run the `/security-review` skill in Claude Code over the pending changes. Record the result in the log entry of the day.
5. Commit, push the branch, and open a pull request: `gh pr create --fill`.
6. Merge when all CI jobs pass. The jobs are listed in [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).

## Recover

* A failed migration leaves `_sqlx_migrations` with `success = false` for that version; fix the SQL and run `qm migrate` again.
* To start the development database over: `psql -h 127.0.0.1 -p 5433 -d postgres -c "drop database qrtrmastr"` then step 3 of option B. Never do this to the Compose volume without a backup.
* A container that will not start: `docker compose logs qm-postgres`, then `docker compose up -d` again. The volume is untouched by a restart.
