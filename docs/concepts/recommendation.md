---
type: Concept
title: Recommendation
description: The two rules behind qm due, what each number means, the thresholds, and what the rules cannot know.
tags: [recommendation, rules]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
sources:
  - id: recommend
    resource: ../../src/domain/recommend.rs
    title: The rules
  - id: usecase
    resource: ../../src/application/recommend.rs
    title: The Due use case
---

# Recommendation

`qm due` lists what you are about to run out of. It uses counts and observed gaps only. It fits no model, draws no samples, and reads no weather. The maintainer chose that for the MVP; probabilities and seasons are deferred.

## The habit rule (default)

The products most often in the latest orders ([ADR 0009](/decisions/0009-qm-due-ranks-by-recent-habit.md)). For each product in at least one of the latest 20 orders, the rule counts the orders that hold it among the latest 20 and among the latest 50. The list is ranked by the first count, then the second, then the later last purchase. The next section says how `qm due` cuts it into lists. The typical quantity is the median billed quantity over those 50 orders. Muted products are left out. The rule uses no dates: on the maintainer's history it names what the next order holds about twice as often as the gap rule.

Two things are applied before the count ([ADR 0010](/decisions/0010-sizes-of-one-product-count-as-one-and-a-bulk-purchase-is-a-stock.md)):

* Products with exactly the same name are one product, the one bought last. A case of 36 and a pack of 3 share one history, and a purchase counts as its quantity times the amount of the pack when the size texts compare.
* A product is a stock when its last purchase is at least 3 times its usual amount. It is expected to last that many usual gaps. Until that day, judged from `--as-of`, it is left out of the list and named in a line below it.

## What `qm due` prints

Four lists ([ADR 0011](/decisions/0011-essentials-extras-reminders-and-additions-with-a-reason.md)):

* **Essentials**: the habit rule's products that are in at least a quarter of the latest 20 orders. All of them.
* **Extras**: the next products of the habit rule, `--top` of them, 10 by default.
* **Remember this?**: `--remember` products, 5 by default, that are in at least 3 orders of the history and in none of the latest 20. A shuffle picks them; its seed is the day, or `--seed`. It is a question, not a prediction.
* **Your additions**: what you added with `qm add`, each with its reason: ran out, almost running out, making a recipe, new food to try, repeat favorite. An addition is listed until an order is delivered after the day it was added.

The history gives about a third of an order. The additions are how the rest of the list is made, and their reasons are kept as data for a later rule.

## The gap rule (`--gap`)

For each product, from the `purchases` view:

* The purchase dates are the distinct delivery dates it appears on.
* The usual gap is the median of the gaps between consecutive purchase dates, in days.
* The order cadence is the median gap between all distinct delivery dates: how often you order at all.
* Days since is the day of the question minus the last purchase date. The day of the question is `--as-of`, or today.

| Purchases | Status |
| --- | --- |
| 1 | bought once; never listed by default |
| 2, below `min_purchases` | tentative; listed with `--include-tentative` |
| at least `min_purchases`, days since more than `lapsed_after_gaps` times the usual gap | lapsed; listed with `--include-lapsed` |
| days since more than the usual gap | overdue, by the difference |
| days since plus the order cadence more than the usual gap | due soon, in the difference |
| otherwise | not due; listed with `--include-not-due` |

The list is ranked: overdue by days overdue, then due soon by how soon, then the rest. The typical quantity is the median billed quantity; for an item sold by weight it is in the receipt's unit, pounds. The last price is the last line price paid. Muted products (`product_preferences.muted`) are left out.

## The simple rule (`--simple`)

Products present in at least half of all orders and absent from the most recent order, ranked by the share of orders they appear in. No dates are involved. It exists for the day the gap rule feels like too much.

## Thresholds

`min_purchases` (3) and `lapsed_after_gaps` (3) live in the `[recommend]` section of the configuration file ([Configuration](/reference/configuration.md)).

## What the rules cannot know

A gap is observed on orders, not consumption: an item bought on a sale and frozen looks like a long gap. The median absorbs the occasional case. Seasonal items show one long gap a year; with two years of history the rule reports them as lapsed in their off-season and overdue at its end. Nothing here is a forecast; the deferred design records what one would look like.
