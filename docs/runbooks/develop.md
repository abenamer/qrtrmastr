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

* `examples/lint_canary.rs` holds one planted violation of each lint of the table, compiled only with the flag `lint_canary`. `scripts/check.sh planted` runs Clippy on it with that flag and fails unless each lint fires there at the level of `Cargo.toml`, and unless the table and the list `LINT_LEVELS` of the script name the same lints at the same levels. It also fails on a line of the lint table in another form than `name = "level"`, on a second lint table, on a setting of `clippy.toml` beside the three allowances for test code, and on the flag in `src/` or `tests/`. It proves the levels of the table; that a warning fails the build is the `-D warnings` of the Clippy command in the `rust` group. When you add a lint to `Cargo.toml`, add its violation to that file and its name to that list in the same change. An `#[expect]` on a planted violation proves nothing: it switches the lint on for its own scope.

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

## Develop: the checks

`scripts/check.sh` is the one list of the checks (REAS-DOC-006). CI and the Stop hook call its groups. To add a check, add it to a group of the script.

| Group | What it runs | CI job |
| --- | --- | --- |
| `toolchain` | `rust-toolchain.toml`, `rustc` and, in CI, the toolchain that the workflow installed name one version | `rust` |
| `rust` | The `toolchain` group, `cargo fmt --check`, `cargo check`, `cargo clippy` with warnings denied, the `planted` group, the unit tests, the doc tests, `cargo doc` with warnings denied | `rust` |
| `planted` | Clippy on `examples/lint_canary.rs` with its flag: each of the 14 lints of `Cargo.toml` fires on its planted violation at its level | `rust`, as a part of the `rust` group |
| `compile` | `cargo check` only | none; the Stop hook |
| `sqlx` | The migrations on the database of `DATABASE_URL`, then `cargo sqlx prepare --check` | `rust`, on an empty database |
| `database` | `cargo test` for every test target, with the integration tests; needs `DATABASE_URL` | `rust` |
| `deny` | `cargo deny check` | `deny`, through a pinned action with its own version of the tool |
| `audit` | `cargo audit` | `audit` |
| `docs` | The documentation checker and its tests | `docs` |
| `privacy` | The privacy canary: shapes, and your terms when the terms file exists | `privacy`, shapes only |
| `hooks` | The test of the guard hook and the test of the script | `privacy` |
| `lists` | The gate below | `privacy` |

`scripts/check.sh all` runs every group except `sqlx`. Its last line names what it did not run: `sqlx`, the `database` group without `DATABASE_URL`, and the canary with your terms on a machine without the terms file.

Two jobs are actions with no command in the script: `secrets` (gitleaks over the history) and `dependency-review` (the dependencies that a pull request changes). A second workflow, `scan.yml`, runs the audit and the deny check once a day on `main`, because an advisory arrives without a change to the code. Its jobs are named `daily-audit` and `daily-deny`; a failed run sends a notification and blocks nothing.

`scripts/check.sh lists` is a gate against drift. It fails when:

* a line of a workflow or of the Stop hook runs a command of the script without the script, such as `cargo test`;
* the workflow of pull requests does not run each of its nine groups in exactly one step, or does not run on a pull request and on a push;
* a workflow has an `if`, a `needs`, a `continue-on-error` or a `shell` line in its plain spelling, other than the one condition of the dependency review. Branch protection takes a job that is skipped as passed, so a condition on a required job can switch it off;
* the deny action or its `check` command is not in a workflow, or the daily workflow has no schedule, no audit, or a job with the name of a required check;
* `.github/workflows/` holds a workflow that the script does not know;
* the Stop hook does not call one of its five groups in the form `scripts/check.sh <group> 1>&2 || failed=1`;
* `CLAUDE.md`, the README or this page does not name the script.

`scripts/check.test.sh` plants each of these, except the last, in a copy of the files and asserts the failure. The gate knows the commands of the script only: it does not find a tool that the script does not use, a command in another spelling, or a change inside a group. It is not a control against a change that is meant to switch a check off; the review of the pull request is.

Which job reads a path that a change can touch:

| Path | Read by |
| --- | --- |
| `src/`, `tests/`, `examples/`, `migrations/`, `Cargo.toml`, `Cargo.lock`, `clippy.toml`, `rust-toolchain.toml`, `.sqlx/` when it exists | `rust` |
| `Cargo.toml`, `Cargo.lock`, `deny.toml`, `.cargo/audit.toml` | `deny`, `audit`, `dependency-review` |
| Each `.md` file, `scripts/check_docs.py`, `tests/docs/`, `requirements-docs.txt` | `docs` |
| `.claude/hooks/guard-bash.sh` and its test, `scripts/check.sh` and its test | `privacy`, the `hooks` group runs the two tests; each job that calls the script runs a part of it |
| `.github/workflows/`, `.claude/hooks/stop-check.sh`, and the name of the script in `CLAUDE.md`, `README.md` and this page | `privacy`, the `lists` group, which reads them as text. No job runs the Stop hook or tests its patterns. |
| Each tracked file | `secrets` |
| `tests/`, `docs/`, `src/`, `migrations/`, `README.md`, `CHANGELOG.md`, without the synthetic inputs, images and lockfiles | `privacy`, the canary. It reads no other tracked file: not `scripts/`, `.claude/`, `.github/`, `standards/`, `CLAUDE.md`, `AGENTS.md`, or the files at the top such as `Cargo.toml` and `compose.yaml`. |
| `standards/` | `docs` for the links of its Markdown files; `dependency-review` for its lockfiles. No job compiles or reads its Rust files, manifests and corpus files. |
| `Dockerfile`, `compose.yaml`, `.dockerignore` | No job. Nothing in CI builds the image. |
| `scripts/due_page.sh`, `scripts/pii_canary.sh`, `.claude/hooks/format.sh` | No job runs a test of these scripts. The canary runs, and its exclusions have no test. |
| `.claude/settings.json`, `.github/dependabot.yml`, `.gitleaks.toml`, `.gitignore`, `.env.example`, `LICENSE`, `backups/.gitkeep` | No job, except `secrets`. |

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
       DATABASE_URL=... scripts/check.sh all

   Expected result: the last line starts with `PASS: all` and names what did not run: always the `sqlx` group, and the integration tests when `DATABASE_URL` is not set.
4. Run the `/security-review` skill in Claude Code over the pending changes. Record the result in the log entry of the day.
5. Commit, push the branch, and open a pull request: `gh pr create --fill`.
6. Merge when all CI jobs pass. The jobs are listed in [ADR 0002](/decisions/0002-pull-requests-and-the-security-review-on-every-commit.md).

## Recover

* A failed migration leaves `_sqlx_migrations` with `success = false` for that version; fix the SQL and run `qm migrate` again.
* To start the development database over: `psql -h 127.0.0.1 -p 5433 -d postgres -c "drop database qrtrmastr"` then step 3 of option B. Never do this to the Compose volume without a backup.
* A container that will not start: `docker compose logs qm-postgres`, then `docker compose up -d` again. The volume is untouched by a restart.
