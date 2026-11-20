---
type: Technical Reference
title: Configuration
description: Every environment variable the application reads, with its default.
tags: [configuration, reference]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: config
    resource: ../../src/adapters/config.rs
    title: Settings reader
  - id: env-example
    resource: ../../.env.example
    title: Example environment file
---

# Configuration

`qm` reads these variables from the environment. If a `.env` file exists in the working directory, it is loaded first. `.env.example` lists them with placeholders; `.env` is ignored by git and denied to the assistant.

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `DATABASE_URL` | yes | none | PostgreSQL connection URL. A missing value is an error, never a default. Held as a secret in memory and never logged. |
| `DB_MAX_CONNECTIONS` | no | `4` | Pool size. Must be a positive whole number. |
| `QM_GMAIL_CLIENT_ID` | for `qm fetch` | none | The client ID of your own OAuth client, type "Desktop app" ([Fetch new receipts from Gmail](/runbooks/gmail-fetch.md)). |
| `QM_GMAIL_CLIENT_SECRET` | for `qm fetch` | none | Its client secret. Held as a secret in memory and never logged. |
| `QM_STATE` | no | the user's state directory, `~/.local/state/qrtrmastr` on Linux | Where the Gmail refresh token is kept, in `gmail-token.json` with mode 0600. `/state` in the container, which is `QM_STATE_DIR` on the host. |
| `RUST_LOG` | no | `info` | Log filter, in `tracing` syntax, for example `info,sqlx=warn`. |

The Compose file reads `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` and `QM_DB_PORT` from the same `.env` to create the database service, and `QM_DATA_DIR` (default `~/qm-data`) and `QM_CONFIG_DIR` (default `~/.config/qrtrmastr`) to mount your exports and your configuration into the `qm` container as `/data` and `/config`, both read-only; see [Develop and run](/runbooks/develop.md). Inside the container `DATABASE_URL` and `QM_CONFIG` are set by the Compose file.

## The configuration file

`qm` reads `config.toml` from the user's configuration directory, `~/.config/qrtrmastr/` on Linux, or from the path in `QM_CONFIG`. A missing file is fine until a command needs a section. Any value can be overridden by an environment variable `QM_<SECTION>__<KEY>`, for example `QM_RECOMMEND__MIN_PURCHASES=4`.

| Section | Key | Default | Meaning |
| --- | --- | --- | --- |
| `[redaction]` | `names` | none | Your name as the mail greets you, and your full name. Each at least three characters. |
| `[redaction]` | `emails` | none | Your email addresses. |
| `[redaction]` | `phones` | none | Your phone numbers; any formatting, at least seven digits. |
| `[redaction]` | `address_lines` | none | Each line of the delivery address as the receipt prints it. |
| `[redaction]` | `card_last4` | none | The last four digits of each payment card; exactly four. |
| `[recommend]` | `min_purchases` | `3` | Purchases needed before an item is recommended. |
| `[recommend]` | `lapsed_after_gaps` | `3` | An item not bought for more than this many usual gaps is lapsed. |
| `[import]` | `senders` | `["service@freshdirect.com"]` | The exact From addresses that are FreshDirect's. |
| `[import.restaurant_senders]` | `seamless` | `["orders@eat.seamless.com"]` | The exact From addresses of Seamless order mail. |
| `[import.restaurant_senders]` | `grubhub` | `["orders@eat.grubhub.com", "no-reply@grubhub.com"]` | The same for Grubhub. |
| `[import.restaurant_senders]` | `toast` | `["no-reply@toasttab.com"]` | The same for Toast. |
| `[import.restaurant_senders]` | `square` | `["messenger@messaging.squareup.com"]` | The same for Square. |
| `[import.restaurant_senders]` | `clover` | `["app@clover.com"]` | The same for Clover. |
| `[import.restaurant_senders]` | `doordash` | `["no-reply@doordash.com"]` | The same for DoorDash. |

A `[import.restaurant_senders]` table in the file replaces the whole default: list every platform you want stored. A key that is not one of the six platforms is an error. Mail from an address that is in neither list is counted and not stored ([ADR 0007](/decisions/0007-restaurant-mail-is-stored-at-import-and-parsed-later.md)).

`qm import` refuses to run without a `[redaction]` section; its error prints the section with placeholder values and names the file it looked for. The section is personal data: the file is outside the repository and denied to the assistant. The same terms, one per line, belong in `~/.config/qrtrmastr/canary-terms.txt` for the local privacy canary.
