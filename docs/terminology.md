---
type: Technical Reference
title: Project terminology
description: Project-specific technical terms used in this bundle and in the code.
tags: [terminology, ste]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: reas-terms
    resource: https://github.com/abenamer/reas/blob/main/standard/terminology.md
    title: REAS terminology
---

# Project terminology

These are project-specific technical terms. They are not a reproduction of the ASD-STE100 dictionary. The [REAS terminology](https://github.com/abenamer/reas/blob/main/standard/terminology.md) defines the general Rust and architecture terms (port, adapter, newtype, typestate).

| Term | Meaning |
| --- | --- |
| Alias | One normalized name and size text as a receipt prints a product, pointing at one product. Stored in `product_aliases`; a line stores its alias, so a merge of two products is one update on the alias. |
| Anomaly | A row of a receipt's item table that no parser rule classified, kept with its raw cells in `order_line_anomalies`. The order is still stored. Nothing is dropped. |
| Billed quantity | The quantity the receipt charged for: the weight when the item is sold by weight, else the second number of the quantity cell. Stored as `billed_qty`. |
| Composition root | `src/main.rs`. The one place that creates the concrete adapters and wires them to the use cases. |
| DKIM pass | The mail's `Authentication-Results` header says the sender's signature verified. Stored as `dkim_pass`; a parser refuses a message without it. |
| Kind (message) | Which of a platform's mails a message is: `placed`, `updated`, `receipt`, or `unknown`. Classified from the subject; `unknown` is stored, never skipped. |
| Local fixture | A real receipt, redacted and written to `tests/fixtures/local/`, which git ignores. Tests use it when present. |
| Message key | What a raw message is stored under and deduplicated on: the `Message-ID` header when present, else the SHA-256 of the raw bytes. `domain::ids::MessageKey`. |
| Order number | FreshDirect's identifier of an order: 11 digits until August 2026, 13 after. Text, never a number. The primary key of `grocery_orders`. |
| Parse state | The lifecycle of a raw message: `fetched`, then one of `parsed`, `partial`, `failed`, `unsupported` after an attempt. `domain::states::ParseState`, with a transition function per legal change. |
| Platform | Where a message came from. `freshdirect` today; the column exists so another source adds a value, not a table. |
| Purchase | One line of one order, read through its alias to a product: product, delivery date, billed quantity and price. The `purchases` view. |
| Raw message | One imported mail after redaction: its key, kind, redacted HTML and text, and parse state. Stored in `raw_messages`. |
| Reconciled | The sum of an order's line prices matches its subtotal within one cent. Stored as `reconciled` on the order. |
| Redaction term | A text the redactor removes before storage: an address line, a phone number, a card's last four digits, a name, an email address. Kept in configuration outside the repository. |
| Synthetic fixture | A receipt built from the real layout with invented products, prices that reconcile, and fake personal data chosen to trip the privacy canary. Committed under `tests/fixtures/synthetic/`. |
