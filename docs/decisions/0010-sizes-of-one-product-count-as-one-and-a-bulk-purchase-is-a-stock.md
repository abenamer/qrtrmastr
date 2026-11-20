---
type: Architecture Decision
title: Sizes of one product count as one, and a bulk purchase is a stock
description: Why the habit rule joins products with the same name and leaves out a product whose last purchase was a multiple of the usual amount, with the backtest that also rejected a general not-due-yet filter.
tags: [decision, recommendation]
generated: { by: "process:claude-code", at: "2026-11-18T21:00:00Z" }
---

# ADR 0010 — Sizes of one product count as one, and a bulk purchase is a stock

## Context

The habit rule of [ADR 0009](/decisions/0009-qm-due-ranks-by-recent-habit.md) listed a seaweed snack as a staple last bought 48 days before. The maintainer had bought a case of 36 on 2026-11-01. The case is another product in the catalog: the same name, the size text `36ct, 0.17oz ea` and not `3ct, 0.17oz ea`. 139 names in the history have more than one product, 307 products in all. ADR 0009 rejected a merge by similar names; a merge by the same name was not measured there.

## Decision

* For the habit rule, products with exactly the same name are one product: the one of them bought last (`merge_variants` in `src/domain/recommend.rs`). The catalog is not changed.
* The amount of a pack is read from the size text (`pack_amount` in `src/domain/pack.rs`): pieces, ounces or fluid ounces. When every size of a name is understood in one dimension, a purchase counts as its quantity times that amount; otherwise as its quantity. A product sold by weight counts as its quantity.
* A product is a stock when its last purchase is at least 3 times its usual amount (`STOCKED_RATIO`). It is expected to last that many usual gaps from the purchase. Until that day `qm due` leaves it out of the list and names it in a line below the list. The usual amount and the usual gap are medians over the purchase days in the latest 50 orders; a product with fewer than three purchase days is never a stock.
* `--as-of` applies to the habit rule: it is the day a stock is judged from.
* No general "not due yet" filter. It was measured and makes the list worse.
* The gap rule and the simple rule do not change: they still see each size as its own product.

## Consequences

* A product bought in more than usual quantity is also a stock, without a change of size: three quarts where one is usual.
* A stock that perishes before it is used is still left out until its day.
* 66 of the 139 names have sizes that do not compare (a size text without an amount, or two dimensions); they are joined, and a purchase counts as its quantity.

## Alternatives

* Leave out every product whose usual gap has not passed, with the gap scaled by the amount bought: measured, worse at every threshold (see Runs). Staples are bought again before their usual gap has passed.
* A threshold of 2 usual amounts: measured, it left out products that were bought anyway.
* A merge in the catalog (`qm product merge`): it stays a manual step; a case and a small pack are different things to order.

## Runs

2026-11-18. The walk-forward test of ADR 0009 over the latest 60 of 510 orders, a prototype script outside the repository over an export of the purchases with their size texts. "Right" and "caught" as in ADR 0009, for a list of 15.

| Rule | Right | Caught |
| --- | --- | --- |
| Habit rule, each size its own product | 0.25 | 0.31 |
| Habit rule, same name joined | 0.25 | 0.32 |
| Joined, and stocks left out (ratio 3) | 0.251 | 0.320 |
| Joined, and every product left out before half its scaled gap | 0.23 | 0.29 |
| Joined, and every product left out before its scaled gap | 0.17 | 0.22 |

With the ratio 3, a stock was left out of a list 11 times in the 60 orders, and the product was in the order 0 times. With the ratio 2: 88 times, in the order 15 times. With the usual amount over the latest 5 or 8 purchase days and not all of them: the same to two decimals. The Rust rule has unit tests (`a_case_and_a_small_pack_are_one_product_and_the_case_is_a_stock`, `a_size_text_gives_its_amount`); it was run on the stored history and left out two products as of 2026-11-18. It was not run against the prototype's numbers.
