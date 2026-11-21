---
type: Runbook
title: Develop and run
description: Running qm through Docker Compose, the toolchain for development, the database options, the checks, and the pull-request flow.
tags: [runbook, development, docker]
generated: { by: "process:claude-code", at: "2026-11-13T23:30:00Z" }
updated: 2026-11-21
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
       docker compose run --rm qm import --takeout /data/<folder inside your exports directory>
       docker compose run --rm qm due
       docker compose run --rm qm fixture add <key> --dir /fixtures

   Expected result: the same output the native binary gives ([Command line](/reference/cli.md)). `--rm` removes the container when the command ends; the data stays in the database volume. Before the first `fixture add`, run `mkdir -p tests/fixtures/local` on the host: a bind mount Docker creates itself belongs to root and the container cannot write to it.

Inside the container, your exports directory is `/data` (read-only), your configuration directory is `/config` (read-only), and `tests/fixtures/local` of the repository is `/fixtures`. The container has a read-only root file system, no capabilities, and runs as an unprivileged user.

Never run `docker compose down -v`: it deletes the volume. The guard hook refuses its common forms as a backstop; the rule is yours to keep.

## Develop: toolchain

* Rust, in the version that `rust-toolchain.toml` names, with `rustfmt` and `clippy`. rustup reads the file and installs that version when you run `cargo` in the repository. CI uses the same version and fails when the workflow and the file differ. The image in the `Dockerfile` is the same minor version. To move to a newer Rust, change the file, the workflow and the image in one pull request; a newer Clippy can report findings that the old one did not.
* `cargo install sqlx-cli --no-default-features --features postgres,rustls --locked`, `cargo install cargo-audit cargo-deny --locked`: toolchain utilities, not project binaries.
* Python 3 with `pip install -r requirements-docs.txt` for the docs checker.
* The project's own binary is never installed on the host: `cargo build` puts it under `target/`, and `docker compose run --rm qm` runs it.

## Develop: the lints

`Cargo.toml` declares the lint levels under `[lints.rust]` and `[lints.clippy]`, with the REAS rule that names each one. CI runs Clippy with `-D warnings`, so a level of `warn` fails the `rust` job as `deny` does.

* Unsafe code does not compile, in any target of the package.
* An `unwrap` outside test code does not pass. An `expect` outside test code passes only below `#[expect(clippy::expect_used, reason = "...")]`; the reason says why the failure cannot occur. Put the attribute on the one statement or the one small function that holds the call, so that it does not cover a later call.
* A pattern that is a constant of the source is compiled with `domain::pattern::fixed`, which sets `SIZE_LIMIT`, 1 MiB, as the most memory of the compiled pattern. A pattern built from data sets the same limit and returns an error when it does not compile.
* Arithmetic outside test code is written with a `checked_` or a `saturating_` method, or in a form that cannot overflow; `+`, `-` and `*` on a number that is not a constant do not pass (`clippy::arithmetic_side_effects`, REAS-SEC-003). Use `checked_` and return an error or `None` when a wrong number would be read later. Use `saturating_` only when a comment can say why the bound of the type gives the right answer. A counter of a run report goes up with `application::count`.
* A slice, a vector or a text is read with `get`, `first`, a slice pattern or an iterator; `v[i]`, `v[a..b]` and `&text[a..b]` do not pass outside test code (`clippy::indexing_slicing`, `clippy::string_slice`). `clippy.toml` leaves an index free in test code. The index of a capture group, `caps[1]`, is not covered: use it only for a group that takes part in every match of the pattern.
* To suppress a lint, write `#[expect(lint, reason = "...")]`. `#[allow]` does not pass. When the suppressed finding goes away, the compiler reports the attribute, and you remove it.
* `clippy.toml` leaves `unwrap` and `expect` free inside a test function and a test module. A helper function in `tests/` is outside both; `tests/database.rs` has one attribute for the file.

The release profile sets `overflow-checks = true`: an arithmetic overflow stops the program with an error and a failure exit status; the number does not wrap.

## Develop: the documentation checker

`python3 scripts/check_docs.py` reads each file whose name ends in `.md`, in the working tree. It does not read the directories `.git`, `target`, `.venv` and `node_modules` at the top of the repository. [ADR 0016](/decisions/0016-the-documentation-checker-follows-the-checker-of-reas.md) says where the checker comes from. It fails on:

* a concept without a `type`, without an `updated` date, or with an `updated` value that is not a day as `YYYY-MM-DD` or is after today. A concept is each file of `docs/` except `docs/log.md` and a file named `index.md`;
* a concept that `docs/index.md` does not reach through links in the bundle;
* a local link to a file that does not exist, and a fragment (`#heading`) that the target does not have;
* a `generated` or `verified` event without an actor or without a time that has a UTC offset;
* a heading of `docs/log.md` at level two that is not a date;
* a retired term of [the terminology](/terminology.md) in the text outside code, in the title of a link, in HTML, or in the `title` or the `description` of the front matter. The table itself, the log, the changelog, the language review and `standards/` can name one;
* a file with the extension `.MD` or `.markdown`, and a Markdown file that is a symbolic link;
* a gate that looked at nothing: no concept, no local link, no fragment, no retired term, or no rule declaration (the vendored standard holds them).

It prints a warning for a concept whose `updated` date is older than 90 days (an audit report), 180 days (a technical reference or a review procedure) or 365 days (any other type). Read the page against the code, and then move the date or remove the page. `--strict-freshness` makes the warning an error; CI does not use it.

`tests/docs/test_docs.py` plants a violation for each of these checks in a small bundle and asserts that the checker names it. Five tests call the entry point of the command and read its exit status and what it prints.

The checker has limits:

* It does not fetch an external link, and it does not check the language.
* It searches a retired term as plain text. It does not find a term with an invisible character in it, or with a letter of another alphabet that looks the same. In a block of HTML it does not find a term with a tag or an entity inside the word. A term of two words is also found when a line or a paragraph ends between the two words.
* It searches the `title` and the `description` of the front matter, and no other key.
* A term in the table of retired terms is plain text with single spaces. The checker refuses a term with markup; a term with an entity or two spaces in it finds nothing.
* It reads the table of retired terms from the first line that is the heading `## Retired terms`.
* A file named `index.md` below `docs/` is a page of links and not a concept: the checker asks it for no type and no date.
* A link that the reader cannot see (a link with no text, a link in an HTML template) counts for reachability.
* It reads the HTML attributes `href`, `src`, `id` and `name`. It does not read `srcset`, `data` or `poster`, and it does not read a tag that follows an inline `<script>` or `<style>` or an attribute that is not closed.
* It takes a link with a colon in its first part, such as `x:page.md`, as an external link.
* It reads the extensions `.md` and `.markdown`. It does not read a file with another extension that a site can show as Markdown, such as `.mkd`.
* It reads a directory named `target`, `.venv` or `node_modules` below the top of the repository, also when git ignores it. It does not follow a symbolic link to a directory.
* It reads a footnote as CommonMark does, as a link definition.

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
