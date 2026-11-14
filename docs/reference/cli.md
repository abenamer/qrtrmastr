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

| Command | What it does | Output |
| --- | --- | --- |
| `qm migrate` | Applies the pending migrations. A second run applies nothing. | One line per applied migration, then `N applied now, M already applied`. |
| `qm import --mbox <path>` | Reads a Takeout export, stores each new message redacted, then parses every stored receipt that waits ([Import](/concepts/import.md)). Idempotent. Needs the `[redaction]` section of the configuration. | Two lines of counts: messages read, new, already known, from other senders; receipts parsed, partial, failed, unsupported, left for later, anomalies. A `REFUSED` line per message in which personal data survived; the command then exits with status 1. |
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

Commands of later milestones: `qm review`, `qm reparse --stale`, `qm alias suggest|merge|split`, `qm product rename|mute`, `qm cart`. Each is documented here when it exists.
