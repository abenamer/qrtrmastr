---
type: Architecture Decision
title: qm due ranks by recent habit
description: The backtest that compared the gap rule, frequency rules and a fitted model on the maintainer's history, and why the default of qm due is now the count of a product in the latest orders.
tags: [decision, recommendation]
generated: { by: "process:claude-code", at: "2026-11-18T19:00:00Z" }
---

# ADR 0009 — qm due ranks by recent habit

## Context

[ADR 0006](/decisions/0006-recommendation-by-usual-gap-with-a-simple-rule-beside-it.md) made the gap rule the default of `qm due` before any real history was loaded, and said that a replacement must beat it on held-out orders. The history is now loaded: 510 orders from 2017-03 to 2026-11, one about every 6 days, 11.8 distinct products in an order, 1,741 products. The first `qm due` listed products bought three or four times years ago at the top. The maintainer asked for predictive analytics.

## Decision

* The default of `qm due` is the habit rule: the products most often in the latest 20 orders, ties broken by the latest 50 orders, then by the later last purchase. `--top N` sets the length of the list; the default is 15.
* The gap rule stays under `--gap` and the simple rule under `--simple`. Neither changed.
* No fitted model ships. On this history it does not beat the habit rule.
* Product variants are not merged by similar names. Such a merge changed the result by one point and joined products that differ (large and extra large eggs). [ADR 0010](/decisions/0010-sizes-of-one-product-count-as-one-and-a-bulk-purchase-is-a-stock.md) later joined products with exactly the same name.

## Consequences

* The list is the staples. It does not say when a product runs out and it cannot name a product that is bought every few months; `--gap` is the rule for that question.
* About half of the first five products listed are in the next order, and a list of fifteen holds about a third of what the next order holds. Most of an order is not habit.
* A dropped product leaves the list after 20 orders, about four months.
* The latest orders are taken by delivery date, with no upper limit. A receipt with a date far ahead, which a forged mail can carry ([ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md)), keeps one of the 20 places until 20 later orders exist. Nothing leaves out an order of a later date or a non-verified order yet.

## Alternatives

* A logistic model over frequency in the latest 5, 10, 20, 50 and 100 orders, the time since the last purchase against the usual gap in orders and in days, the regularity of the gaps and the season: measured, not better than the habit rule, and its numbers would be harder to explain.
* The gap rule with other thresholds: its ranking by days overdue is the defect, not a threshold.

## Runs

2026-11-18. Walk-forward over the latest 60 orders of the 510: for each, the rule sees only the orders before it, and its list is compared with the order. "Right" is the share of listed products that are in the order; "caught" is the share of the order that the list holds. The prototype is a script outside the repository over an export of the `purchases` view.

| Rule | Top 5: right, caught | Top 10: right, caught | Top 15: right, caught |
| --- | --- | --- | --- |
| Count in the latest 20 orders, then 50 (the habit rule) | 0.43, 0.18 | 0.30, 0.26 | 0.25, 0.31 |
| Count in the latest 50 orders | 0.45, 0.19 | 0.30, 0.26 | 0.24, 0.31 |
| Fitted logistic model, 9 features | 0.43, 0.18 | 0.30, 0.26 | 0.25, 0.32 |
| Fitted logistic model, 16 features | 0.43, 0.18 | 0.32, 0.27 | 0.25, 0.32 |
| Closeness to the usual gap | 0.21, 0.09 | 0.11, 0.10 | 0.09, 0.12 |

The gap rule as `qm due` applied it (every product whose gap has passed, at least four purchases, not lapsed) listed 69 products for an order, of which 0.05 were in it, and caught 0.27. With product variants merged by name (302 of 1,741 joined) the fitted model reached 0.45, 0.19 at five. The model was fitted on orders 150 to 449 and tested on the latest 60; its probabilities were calibrated in the low range (0.14 predicted, 0.14 observed) and too high above 0.7 (0.57 observed). The habit rule in `src/domain/recommend.rs` has a unit test; it was not run against the prototype's numbers.
