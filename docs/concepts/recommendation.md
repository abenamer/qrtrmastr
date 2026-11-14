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

## The gap rule (default)

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
