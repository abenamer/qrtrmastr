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
| `RUST_LOG` | no | `info` | Log filter, in `tracing` syntax, for example `info,sqlx=warn`. |

The Compose file reads `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` and `QM_DB_PORT` from the same `.env` to create the database service; see [Develop](/runbooks/develop.md).

Redaction terms and the recommendation thresholds arrive with milestone 1 in a TOML file under the user's configuration directory; this page lists them when they exist.
