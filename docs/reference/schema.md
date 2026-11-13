---
type: Technical Reference
title: Database schema
description: Every table of the store, its purpose, and the migration that made it.
tags: [database, schema, postgresql]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: m0001
    resource: ../../migrations/0001_raw_mail.sql
    title: Migration 0001, raw mail
  - id: m0002
    resource: ../../migrations/0002_grocery.sql
    title: Migration 0002, grocery orders and products
---

# Database schema

Migrations live in `migrations/` and only add. `qm migrate` applies them; SQLx records each in `_sqlx_migrations`. A check constraint that lists values is named `<table>_<column>_check` and matches a `text_enum!` in `src/domain/states.rs`. Every index has a comment in the migration that names the query it serves.

| Table or view | Purpose | Key | Migration |
| --- | --- | --- | --- |
| `raw_messages` | One imported mail after redaction, with its parse state and version. | `message_key` unique; `source_sha256` unique | 0001 |
| `parse_attempts` | One row per parser run over a message: parser, version, outcome, error, anomaly count. | identity | 0001 |
| `grocery_orders` | One order as the receipt states it, with its totals and whether it reconciled. | `order_number` | 0002 |
| `products` | One product identity: display name, size, sold by each or weight, department. | identity | 0002 |
| `product_aliases` | One normalized name and size pointing at a product. | identity; `(name_norm, size_norm)` unique | 0002 |
| `grocery_order_lines` | One line of one order, with the quantity cell verbatim, weight, prices, flags, and the alias it matched. | identity; `(order_number, line_no)` unique | 0002 |
| `order_line_anomalies` | A receipt row no rule classified, with its raw cells. | identity | 0002 |
| `product_preferences` | Per product: muted, note. | `product_id` | 0002 |
| `purchases` (view) | Lines joined through alias and product to their order's delivery date. What the recommendation reads. | — | 0002 |

## Column notes

* `raw_messages.parse_state` with `version`: a transition is a conditional update on both, and succeeds only when exactly one row changes (REAS-STATE-003).
* `grocery_order_lines.qty_first` and `qty_second`: the two numbers of the receipt's quantity cell, stored verbatim. `billed_qty` is the weight when present, else `qty_second`; the meaning of the pair is confirmed against a real receipt in milestone 1.
* Money columns are `numeric(10,2)`; quantities and weights `numeric(10,3)`. No floating point.
* `pg_trgm` is created by migration 0002 for `qm alias suggest`. It is a trusted extension, so the database owner can create it.
