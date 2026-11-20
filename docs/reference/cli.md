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
| `qm import --takeout <path>` | Reads Takeout exports: a folder of `.zip` archives and `.mbox` files, one archive, or one mailbox file. Archives are read in place and nothing is extracted; one message is in memory at a time. `--mbox` is an alias. One archive is open at a time. Stores each new message of FreshDirect and of the restaurant platforms redacted, then parses every stored FreshDirect receipt that waits ([Import](/concepts/import.md)). Idempotent. Needs the `[redaction]` section of the configuration. A path that cannot be opened, a folder or archive without mail, a broken archive, or a missing `[redaction]` section, is an error with status 1 whose text says what the file is, how to request it, or which section to add. | Three lines of counts (the second, new messages by platform, only when there are new ones): messages read, new, already known, from other senders; receipts parsed, partial, failed, unsupported, left for later, anomalies. A `REFUSED` line per message in which personal data survived, and one `REFUSED <count>: <cause>` line per cause; above twenty refusals only the lines per cause. A cause names the platform, the message kind, the part and the check, never the text. The command then exits with status 1. |
| `qm gmail login` | Signs in to Gmail with read-only access for `qm fetch` ([Fetch new receipts from Gmail](/runbooks/gmail-fetch.md)). Prints an address to open in a browser and reads the address the browser ends on. Needs a terminal, `QM_GMAIL_CLIENT_ID` and `QM_GMAIL_CLIENT_SECRET`. | `signed in; qm fetch reads your new receipts`. |
| `qm fetch` | Reads the mail of the configured senders from Gmail, received since a week before the newest stored message, then stores and parses it as `qm import` does ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)). `--since YYYY-MM-DD` reads from another day; `--max N` is the most messages of one run, 500 when omitted. Idempotent. | The lines of `qm import`. |
| `qm add ITEM` | Adds an item to your next order by hand, with the reason ([ADR 0011](/decisions/0011-essentials-extras-reminders-and-additions-with-a-reason.md)). `--reason` is `out-of-stock` (none is left at home), `running-low` (almost used up), `recipe`, `new-food` or `repeat-favorite`; without it the command asks, and when it cannot ask (no terminal) it stores nothing and exits with status 1. Through Compose the question needs a terminal: `docker compose run --rm qm add miso paste`. | One line: the number of the addition, the item, the reason, and the product of your history that the text names when it names exactly one. |
| `qm drop NUMBER` | Removes one addition. Status 1 when no addition has the number. | `dropped NUMBER`. |
| `qm due` | What your next order most likely holds ([Recommendation](/concepts/recommendation.md)): the essentials and the extras of the habit rule, a shuffled "remember this?" list, and your additions with their reasons. One row per product of the habit rule: how many of the latest 20 and of the latest 50 orders hold it, the last purchase, the typical quantity. Sizes of one product count as one, and a product bought in bulk is left out and named below the list until the stock is expected to run out. With `--gap`: what you are about to run out of, by the gap rule. | The day and the order count, then the essentials, the extras, the reminders, your additions and a line for each stocked product. With `--html`: the same as one web page ([ADR 0012](/decisions/0012-the-next-order-as-a-page-that-is-a-file.md)); `scripts/due_page.sh` writes it to `next-order.html` in the exports directory and prints the path. With `--gap`: a header with the day, the order count and the order cadence, then one row per item: product, times bought, usual gap, last bought, days since, typical quantity, status. |
| `qm fixture add <key> [--dir DIR]` | Writes the stored, redacted HTML of the message with that key under `DIR/<platform>/<year>/`. The default `tests/fixtures/local` is resolved against the repository root, not the working directory, so it lands in the ignored path from any subdirectory. | The path written. |

### `qm due` options

| Option | Effect |
| --- | --- |
| `--top N` | How many extras follow the essentials; 10 when omitted. |
| `--remember N` | How many "remember this?" products; 5 when omitted. |
| `--html` | Print a web page instead of a table. Not with `--json`, `--gap` or `--simple`. |
| `--seed NUMBER` | The seed of the "remember this?" shuffle; the day when omitted. |
| `--as-of YYYY-MM-DD` | The day to judge from; today when omitted. For the habit rule it is the day a stock is judged from. |
| `--gap` | The gap rule instead. The three options below apply to it. |
| `--include-tentative` | Also items bought only twice. |
| `--include-lapsed` | Also items not bought for more than three usual gaps. |
| `--include-not-due` | Also items that are not due yet. |
| `--simple` | The simple rule instead: in at least half your orders and missing from the last. |
| `--json` | The same report as JSON: the product label, `product_id`, `times_bought`, `usual_gap_days`, `last_bought`, `days_since`, `typical_quantity`, `last_price`, and `status` with its `status` tag and its number. |

A name on a receipt can hold a character that a terminal obeys: an escape sequence, a line break, a direction mark. The tables, the report of `qm import`, the question of `qm add` and the error line show such a character as text, `\u{1b}` for the escape character. `--json` writes it as a JSON escape, `\u001b`, and the value that a program reads is the same. `--html` has the encoding of HTML; see [where a text leaves the application](/concepts/application.md#where-a-text-leaves-the-application).

## The first run

`qm --help` and `qm import --help` say what the tool needs: a Google Takeout export of your FreshDirect label, and your redaction terms in `config.toml` ([Export your FreshDirect mail and import it](/runbooks/takeout-export.md)). The two errors a first run meets name the fix: an export that cannot be opened prints the path it tried, the file to look for inside the unzipped archive and the runbook; a missing `[redaction]` section prints the section to add with placeholder values and where the file lives (`/config` in the container is `QM_CONFIG_DIR` on the host).

Commands of later milestones: `qm review`, `qm reparse --stale`, `qm alias suggest|merge|split`, `qm product rename|mute`, `qm cart`. Each is documented here when it exists.
