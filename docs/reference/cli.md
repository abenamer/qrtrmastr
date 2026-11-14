---
type: Technical Reference
title: Command line
description: The qm commands, their arguments, and their exit status.
tags: [cli, reference]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: cli
    resource: ../../src/adapters/cli/mod.rs
    title: Command definitions
  - id: main
    resource: ../../src/main.rs
    title: Dispatch
---

# Command line

`qm` reads its settings from the environment and an optional `.env` file ([Configuration](/reference/configuration.md)). Every command exits with status 0 on success and 1 on failure, with the error on standard error (REAS-RUST-005). Logs go to standard error at the level `RUST_LOG` sets.

The command runs as a container: `docker compose run --rm qm <command>` ([Develop and run](/runbooks/develop.md)). Inside it, your exports directory is `/data`, your configuration directory is `/config`, and the local fixtures directory is `/fixtures`; `qm fixture add` needs `--dir /fixtures` there, because the container has no repository root to anchor to.

| Command | What it does | Output |
| --- | --- | --- |
| `qm migrate` | Applies the pending migrations. A second run applies nothing. | One line per applied migration, then `N applied now, M already applied`. |
| `qm import --mbox <path>` | Reads a Takeout export, stores each new message redacted, then parses every stored receipt that waits ([Import](/concepts/import.md)). Idempotent. Needs the `[redaction]` section of the configuration. An export that cannot be opened, or a missing `[redaction]` section, is an error with status 1 whose text says what the file is, how to request it, or which section to add. | Two lines of counts: messages read, new, already known, from other senders; receipts parsed, partial, failed, unsupported, left for later, anomalies. A `REFUSED` line per message in which personal data survived; the command then exits with status 1. |
| `qm due` | What you are about to run out of, by the gap rule ([Recommendation](/concepts/recommendation.md)). | A header with the day, the order count and the order cadence, then one row per item: product, times bought, usual gap, last bought, days since, typical quantity, status. |
| `qm fixture add <key> [--dir DIR]` | Writes the stored, redacted HTML of the message with that key under `DIR/<platform>/<year>/`. The default `tests/fixtures/local` is resolved against the repository root, not the working directory, so it lands in the ignored path from any subdirectory. | The path written. |

### `qm due` options

| Option | Effect |
| --- | --- |
| `--as-of YYYY-MM-DD` | The day to judge from; today when omitted. |
| `--include-tentative` | Also items bought only twice. |
| `--include-lapsed` | Also items not bought for more than three usual gaps. |
| `--include-not-due` | Also items that are not due yet. |
| `--simple` | The simple rule instead: in at least half your orders and missing from the last. |
| `--json` | The same report as JSON: the product label, `product_id`, `times_bought`, `usual_gap_days`, `last_bought`, `days_since`, `typical_quantity`, `last_price`, and `status` with its `status` tag and its number. |

## The first run

`qm --help` and `qm import --help` say what the tool needs: a Google Takeout export of your FreshDirect label, and your redaction terms in `config.toml` ([Export your FreshDirect mail and import it](/runbooks/takeout-export.md)). The two errors a first run meets name the fix: an export that cannot be opened prints the path it tried, the file to look for inside the unzipped archive and the runbook; a missing `[redaction]` section prints the section to add with placeholder values and where the file lives (`/config` in the container is `QM_CONFIG_DIR` on the host).

Commands of later milestones: `qm review`, `qm reparse --stale`, `qm alias suggest|merge|split`, `qm product rename|mute`, `qm cart`. Each is documented here when it exists.
