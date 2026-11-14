---
type: Concept
title: Product identity
description: How a printed product name becomes a product, why aliases sit between lines and products, and how a mistake is corrected.
tags: [identity, aliases, normalization]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
sources:
  - id: identity
    resource: ../../src/domain/identity.rs
    title: Normalization
  - id: catalog
    resource: ../../src/adapters/postgres/mod.rs
    title: Catalog on PostgresStore
---

# Product identity

A receipt names a product with text and a size in parentheses, and nothing else. The same product can be printed in more than one way over the years. Identity therefore has two layers: an alias, which is one normalized spelling, and a product, which is what you buy.

## Normalization

The printed name is split into a name and a size. The size is the trailing parenthetical, with or without a dash before it: `Lactaid 100% Lactose-Free Whole Milk, Carton - (1/2 gallon)` has the size `1/2 gallon`.

The name becomes `name_norm`: Unicode NFKC, lowercase, one space between words, trailing punctuation removed, every dash variant unified, `approx.` removed. The size becomes `size_norm`: units in one spelling (`8 oz`, `0.5 gal`, `12 ct`, `6 pk, 12 fl oz`), `approx.` removed. Both are pure functions of the printed text; the same text always gives the same pair.

## Aliases and products

`product_aliases` is unique on `(name_norm, size_norm)` and points at a product. When a line's pair is unseen, the import creates a product with the printed name and size as its display name, and an alias with source `auto`. The import never blocks on identity.

An order line stores its alias, not its product. The `purchases` view joins through the alias to the product at query time. Merging two products is therefore one update on the alias rows, and it is reversible; order data never changes.

If the receipt links each product, the link path is stored on the alias as `product_url_path`. When real receipts show those paths are stable identifiers, they become the first key and the normalized pair the fallback.

## Correcting a mistake

Two spellings of one product each got a product: `qm alias suggest` (milestone 2) lists likely pairs by trigram similarity of their normalized names, same sold-by, with purchase counts; `qm alias merge` points the alias at the other product; `qm alias split` undoes it. Until those commands exist, the same is one `update product_aliases set product_id = ... where id = ...` in `psql`.

## What is not done

No guessing. Two products with different normalized names stay two products until you merge them. A language model could propose merges; it would do so as a client of the tool, with you confirming, and that is outside the MVP.
