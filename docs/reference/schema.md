---
type: Technical Reference
title: Database schema
description: Every table of the store, its purpose, and the migration that made it.
tags: [database, schema, postgresql]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-21
sources:
  - id: m0001
    resource: ../../migrations/0001_raw_mail.sql
    title: Migration 0001, raw mail
  - id: m0002
    resource: ../../migrations/0002_grocery.sql
    title: Migration 0002, grocery orders and products
  - id: m0003
    resource: ../../migrations/0003_restaurant_platforms.sql
    title: Migration 0003, restaurant platforms
  - id: m0004
    resource: ../../migrations/0004_order_sender_check.sql
    title: Migration 0004, the sender check of an order
  - id: m0005
    resource: ../../migrations/0005_list_additions.sql
    title: Migration 0005, the additions to the next order
  - id: m0006
    resource: ../../migrations/0006_addition_reason_running_low.sql
    title: Migration 0006, the reason running-low
---

# Database schema

Migrations live in `migrations/` and only add. `qm migrate` applies them; SQLx records each in `_sqlx_migrations`. A check constraint that lists values is named `<table>_<column>_check` and matches a `text_enum!` in `src/domain/states.rs`. Every index has a comment in the migration that names the query it serves. Migration 0003 adds no table: it replaces `raw_messages_platform_check` with a constraint that admits the six restaurant platforms beside `freshdirect`. Migration 0004 adds `grocery_orders.sender_check`, `verified` or `non-verified`.

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
| `list_additions` | What the user added to the next order by hand: text, reason (`AddReason`), day, and the product when the text names one. | `id` | 0005, 0006 |
| `purchases` (view) | Lines joined through alias and product to their order's delivery date. What the recommendation reads. | — | 0002 |

## Column notes

* `raw_messages.parse_state` with `version`: a transition is a conditional update on both, and succeeds only when exactly one row changes (REAS-STATE-003).
* `grocery_order_lines.qty_first` and `qty_second`: the two numbers of the receipt's quantity cell, stored verbatim. `billed_qty` is the weight when present, else `qty_second`; the meaning of the pair is confirmed against a real receipt in milestone 1.
* Money columns are `numeric(10,2)`; quantities and weights `numeric(10,3)`. No floating point. No table has a column for the currency: each stored amount is from a receipt in US dollars, and the store adapter gives each amount that it reads back the currency `USD` (`Money::from_decimal`). A second currency needs a column first. `Money` and `Quantity` refuse a number that these columns cannot hold: above 99,999,999.99 on either side of zero, and 10,000,000 or more.
* `raw_messages.message_key` and the pair `product_aliases.name_norm`, `size_norm` have a unique index, and a B-tree index of PostgreSQL holds no entry above about 2,700 bytes. The columns have no length limit in the schema; the program has one before it writes: 998 bytes for a `Message-ID` key (`domain::ids::MESSAGE_ID_MAX_BYTES`) and 1,024 bytes for the name and size together (`domain::identity::ALIAS_KEY_MAX_BYTES`). No text column holds a NUL character; the reader of a mail refuses a message that has one in a stored part.
* `pg_trgm` is created by migration 0002 for `qm alias suggest`. It is a trusted extension, so the database owner can create it.
