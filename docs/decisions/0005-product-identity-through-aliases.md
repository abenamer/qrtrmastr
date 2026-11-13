---
type: Architecture Decision
title: Product identity through aliases
description: Why an order line points at an alias and not a product, how a name is normalized, and what the product link path may become.
tags: [decision, identity]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
---

# ADR 0005 — Product identity through aliases

## Context

The receipt has no product identifier in its text; it has a name and a size. The same product is printed differently over time, and the recommendation must see it as one product.

## Decision

* The printed name is split into a normalized name and a normalized size, by pure functions ([Product identity](/concepts/identity.md)). The pair is an alias, unique in `product_aliases`, pointing at a product.
* An order line stores its alias. Product identity is resolved at query time through the alias. Merging two products is one reversible update of alias rows; order data never changes.
* An unseen pair creates a product and an `auto` alias during import. Import never blocks on identity.
* If the receipt links products, the link path is stored on the alias. When real receipts show it stable, it becomes the first key.

## Consequences

* The first import creates one product per distinct printed spelling. Duplicates are expected and are fixed by merging, with tooling in milestone 2.
* No guessing: similar names are not merged automatically.

## Alternatives

* Store the product on the line and rewrite lines on merge: rejected; order data would change, and a merge would not be reversible.
* Fuzzy matching at import: rejected; a wrong automatic merge is worse than a duplicate.
