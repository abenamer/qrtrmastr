---
type: Concept
title: Application overview
description: What Quartermaster does, how its layers fit together, and the code path of a command.
tags: [architecture, overview]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: lib
    resource: ../../src/lib.rs
    title: Crate root and layer map
  - id: main
    resource: ../../src/main.rs
    title: Composition root
---

# Application overview

## Purpose

Quartermaster turns FreshDirect receipt emails into a PostgreSQL order history and recommends the next cart. It is a personal tool for one household. It reads a one-time Google Takeout export of the receipts; it does not talk to FreshDirect, and it places no orders.

The MVP does these things:

1. Imports the mail export, redacts personal data, and stores each message once.
2. Parses each receipt into an order with its lines, and records what it could not classify.
3. Resolves each line to a product through an alias, so renamed products stay one product.
4. Recommends items whose usual gap between purchases has elapsed, or, with `--simple`, items bought in most orders and missing from the last.

Recipes, restaurant receipts, probabilistic forecasting, weather, and the MCP server are not in the MVP. The plan that holds them is outside this repository.

## Layers

The code has three layers and one composition root. Dependencies point inward (REAS-ARCH-002).

| Layer | Path | Holds | Must not hold |
| --- | --- | --- | --- |
| Domain | `src/domain` | Identifier newtypes, money and quantities, stored labels and the `ParseState` lifecycle, and the rules that act on them. | I/O, the clock, SQLx, file or network types. |
| Application | `src/application` | Use cases, and the ports (traits) they need from the outside. | Any adapter type. |
| Adapters | `src/adapters` | `PostgresStore`, the settings reader, the command line; later the mbox reader and the receipt parsers. | Business rules. |
| Composition root | `src/main.rs` | Builds the adapters and the use cases and runs one command. | Anything else. |

A use case takes its adapter as a type parameter bound by the port (REAS-ARCH-006): `Migrate<S: SchemaStore>`. The concrete adapter is visible in `main.rs` and nowhere else.

## The code path of a command

`qm migrate` follows the REAS-ARCH-001 order:

1. The command-line adapter (`adapters::cli`) parses the arguments into `Command::Migrate`.
2. The composition root reads the settings, connects `PostgresStore`, and builds `Migrate`.
3. The use case calls the `SchemaStore::migrate` port.
4. `PostgresStore` applies the embedded migrations and reports which were new.
5. The composition root prints the report. A failure prints the error and exits with status 1 (REAS-RUST-005).

Every later command (`import`, `due`, `cart`) follows the same five steps with its own use case and ports.

## Where it runs

On the maintainer's machine, as two containers: PostgreSQL with a named volume, and the `qm` command, run with `docker compose run --rm qm ...` ([Develop and run](/runbooks/develop.md)). Nothing of the project is installed on the host. Nothing runs in the cloud except GitHub Actions, which runs the checks.
