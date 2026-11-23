---
type: Technical Reference
title: Project terminology
description: Project-specific technical terms used in this bundle and in the code.
tags: [terminology, ste]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-22
sources:
  - id: reas-terms
    resource: https://github.com/abenamer/reas/blob/main/standard/terminology.md
    title: REAS terminology
---

# Project terminology

These are project-specific technical terms. They are not a reproduction of the ASD-STE100 dictionary. The [REAS terminology](https://github.com/abenamer/reas/blob/main/standard/terminology.md) defines the general Rust and architecture terms (port, adapter, newtype, typestate).

| Term | Meaning |
| --- | --- |
| Addition | An item the user put on the next order by hand with `qm add`, stored with its reason: `out-of-stock` (none is left at home), `running-low` (almost used up), `recipe`, `new-food` or `repeat-favorite`. Open until an order is delivered after the day it was added. |
| Alias | One normalized name and size text as a receipt prints a product, pointing at one product. Stored in `product_aliases`; a line stores its alias, so a merge of two products is one update on the alias. |
| Anomaly | A row of a receipt's item table that no parser rule classified, kept with its raw cells in `order_line_anomalies`; also a check of the receipt's amounts that did not hold, kept there with no cells. The order is still stored. Nothing is dropped. |
| Billed quantity | The quantity the receipt charged for: the weight when the receipt gives the line one, else the second number of the quantity cell. Stored as `billed_qty`. |
| Composition root | `src/main.rs`. The one place that creates the concrete adapters and wires them to the use cases. |
| Consequential component | A part of the program in which one defect can expose sensitive data, misuse a credential that can change or spend, lose money, or cause physical, legal or safety harm, through a failure path that the maintainer can name (REAS-FOUND-004). The maintainer records which parts are consequential; [ADR 0020](/decisions/0020-reas-1-9-0-draft-and-what-it-changes-in-the-open-gaps.md) holds a proposal. |
| DKIM pass | The mail's `Authentication-Results` header says the sender's signature verified. Stored as `dkim_pass`. An order from a receipt without it is tagged `non-verified`. |
| Essential | A product in at least a quarter of the latest 20 orders. The other products of the habit rule are extras. |
| Failure path | One sentence for a consequential component: the defect, what it travels through, and the party that it reaches. |
| Fuzz target | A small program in `fuzz/fuzz_targets/` that calls one reader of outside bytes with bytes that a fuzzer makes, and that fails on a stop and on a property that does not hold. [ADR 0017](/decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md). |
| Gap rule | `qm due --gap`: an item is overdue when more days than its usual gap have passed since its last purchase, and due soon when the next order would pass the gap. |
| Habit rule | The default rule of `qm due`: products ranked by how many of the latest 20 orders hold them, then of the latest 50. |
| Kind (message) | Which of a platform's mails a message is: `placed`, `updated`, `receipt`, or `unknown`. Classified from the subject; `unknown` is stored, never skipped. |
| Local fixture | A real receipt, redacted and written to `tests/fixtures/local/`, which git ignores. Tests use it when present. |
| Message key | What a raw message is stored under and deduplicated on: the `Message-ID` header, or the SHA-256 of the raw bytes when the header is missing, is empty or carries a configured name, mail address, address line or phone number. A key has a kind, `message_id` or `sha256`, and a text; the store keeps them in two columns, and the text is unique whatever its kind. Written with its kind, a key is `message_id:<text>` or `sha256:<text>`. `domain::ids::MessageKey`. |
| Order number | FreshDirect's identifier of an order: 11 digits until August 2026, 13 after. Text, never a number. The primary key of `grocery_orders`. |
| Order cadence | The median gap, in days, between all distinct delivery dates: how often you order at all. The horizon of "due soon". |
| Parse state | The lifecycle of a raw message: `fetched`, then one of `parsed`, `partial`, `failed`, `unsupported` after an attempt. `domain::states::ParseState`, with a transition function per legal change. |
| Platform | Where a message came from: `freshdirect`, or a restaurant platform (`seamless`, `grubhub`, `toast`, `square`, `clover`, `doordash`). Found from the exact sender address. Another source adds a value, not a table. |
| Purchase | One line of one order, read through its alias to a product: product, delivery date, billed quantity and price. The `purchases` view. |
| Raw message | One imported mail after redaction: its key, kind, redacted HTML and text, and parse state. Stored in `raw_messages`. |
| Reconciled | The receipt of an order agrees with itself within one cent in three checks: the line prices sum to the subtotal; the subtotal, tax, bottle deposit, delivery fee, service fee and tip sum to the total; each line with a unit price is that price times its billed quantity, except a line with an options price or the special-price flag, which is not checked. Stored as `reconciled` on the order. |
| Redaction term | A text the redactor removes before storage: an address line, a phone number, a card's last four digits, a name, an email address. Kept in configuration outside the repository. |
| Reminder | A product in at least 3 orders of the history and in none of the latest 20, shown under "remember this?". Picked by a shuffle with a seed. |
| Restore drill | A run of `scripts/restore_drill.sh`: a new backup is restored into a PostgreSQL container that is thrown away, and each table is compared with the database. Its record goes into the log. |
| Seed | A committed input of a fuzz target, in `fuzz/seeds/`, written by `scripts/fuzz_seeds.py` from the synthetic fixtures. A fuzz run starts from the seeds. |
| Simple rule | `qm due --simple`: products present in at least half of all orders and absent from the most recent one. No dates. |
| Stock | A last purchase of at least 3 usual amounts. The product is left out of the lists until the stock is expected to run out. |
| Synthetic fixture | A receipt built from the real layout with invented products, prices that reconcile, and fake personal data chosen to trip the privacy canary. Committed under `tests/fixtures/synthetic/`. |
| Takeout export | What Google Takeout delivers for Gmail: one or more `.zip` archives, each with the mail in mbox form under `Takeout/Mail/`. The MVP's mail source, read in place. It carries personal data and lives outside the repository. |
| Typical quantity | The median billed quantity of a product over its purchases; what `qm due` suggests. |
| Unsafe baseline | `unsafe-baseline.tsv`: the count of unsafe items for each package that the counter names, written by `scripts/check.sh unsafe write`. The `unsafe` group fails when a count is not the one of the file. |
| Usual gap | The median gap, in days, between a product's distinct purchase dates. |

## Retired terms

A retired term is a word that this bundle used for a concept and does not use now. Use the replacement. The checker fails when a document uses a retired term in its text outside code; this table, the log, the changelog and the language review can name one. Write a retired term in the table as plain text.

| Retired term | Replacement | Reason |
| --- | --- | --- |
| staple | essential, or extra | [ADR 0011](/decisions/0011-essentials-extras-reminders-and-additions-with-a-reason.md) gave the products of the habit rule two names. The older word did not say which of the two it meant. |
| unverified | non-verified | The tag of an order from a receipt without a DKIM pass is `non-verified` ([ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md)). One word for the tag in the text and in the database. |

