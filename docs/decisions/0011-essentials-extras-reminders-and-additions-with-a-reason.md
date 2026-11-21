---
type: Architecture Decision
title: Essentials, extras, reminders, and additions with a reason
description: Why qm due prints the habit list in two parts with a shuffled list of products that left the history, and why the user finishes the list by hand with a reason for each addition.
tags: [decision, recommendation]
generated: { by: "process:claude-code", at: "2026-11-19T15:00:00Z" }
updated: 2026-11-20
---

# ADR 0011 — Essentials, extras, reminders, and additions with a reason

## Context

The tests of [ADR 0009](/decisions/0009-qm-due-ranks-by-recent-habit.md) and [ADR 0010](/decisions/0010-sizes-of-one-product-count-as-one-and-a-bulk-purchase-is-a-stock.md) found a limit: a list of 15 from the history holds about a third of the next order, and no rule that was measured does better. Most of an order is not habit. The maintainer's direction: name the core of the order "essentials" and the rest "extras", show a random "remember this?" list, and let the user finish the list by hand, with a question about the reason for each addition: out of stock, making a recipe, new food to try, repeat favorite.

## Decision

* `qm due` prints the habit list in two parts. An **essential** is in at least a quarter of the latest 20 orders (`ESSENTIAL_ONE_IN`, `HabitRecommendation::is_essential`); all essentials are listed. The **extras** are the next products by the same ranking, cut to `--top`, 10 by default.
* **Remember this?** lists `--remember` products, 5 by default, in at least 3 orders of the whole history and in none of the latest 20 (`remember` in `src/domain/recommend.rs`). Which ones is a shuffle fixed by a seed. The seed is the day unless `--seed` gives one, so the list changes each day and a run can be repeated. The domain takes the seed as a value; it reads no clock and no random source.
* `qm add ITEM` stores an **addition** (`list_additions`, migration 0005): the text, the reason, the day, and the product when the text is inside the name of exactly one product. The reason is one of `out-of-stock`, `running-low`, `recipe`, `new-food`, `repeat-favorite` (`AddReason`). `out-of-stock` is the stock of the user: none is left at home and the product must be replaced. `running-low` is "almost running out": some is left, not for long; the maintainer added it on 2026-11-19 (migration 0006), with ketchup as the usual case. Without `--reason` the command asks on the terminal; when it cannot ask, it stores nothing and says which option to give. `qm drop NUMBER` removes an addition.
* `qm due` lists the additions that are open: added on or after the latest delivery date. An order delivered later closes them. Nothing is deleted; the rows stay with their reasons.
* The reminders are not a prediction and are not held to the backtest of a prediction.

## Consequences

* Each addition is a labelled example of why a product outside the habit was wanted. With enough of them, a later rule can be tested: out-of-stock and running-low additions against the gap of the product, repeat favorites against the reminders.
* An addition made on the day of a delivery stays open until the next order.
* The latest delivery date has no upper limit. An order with a date far ahead, which a forged receipt can carry ([ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md)), closes every addition until that day: `qm add` stores it and `qm due` does not list it, and nothing says why.
* A text that matches several products, such as several sizes of one name, has no product. The text is kept and nothing is guessed.
* The reminders can name a product that is no longer sold; the tool knows no catalog but the history.

## Alternatives

* Extras as every other product of the latest 20 orders: about 112 products, of which 5 in 100 are in the next order. Cut to a length instead.
* A random source in the rule: rejected; a run could not be repeated or tested.
* A free-text reason: rejected; fixed reasons can be counted and compared.

## Runs

2026-11-19, the walk-forward of ADR 0009 over the latest 60 of 510 orders, prototype script outside the repository, same names joined.

| Essential when in at least | Products listed | Right | Caught |
| --- | --- | --- | --- |
| 3 of the latest 20 | 23.9 | 0.19 | 0.38 |
| 4 of 20 | 14.0 | 0.25 | 0.30 |
| 5 of 20 (chosen) | 8.6 | 0.32 | 0.23 |
| 6 of 20 | 5.7 | 0.41 | 0.20 |
| 8 of 20 | 3.4 | 0.52 | 0.15 |

Reminders: with at least 3 orders in the history and none in the latest 20, 444 products are candidates for an order. One of them is in the next order 4 times in 1,000 and in one of the next five orders 20 times in 1,000; a list of five has a product bought within five orders about 1 time in 10. On the stored history as of 2026-11-19 the rule lists 8 essentials. Tests: `essentials_are_in_a_quarter_of_the_latest_orders_and_reminders_left_them`, `an_item_names_a_product_only_when_exactly_one_matches`, `the_reason_prompt_takes_a_number_or_the_text_and_asks_again_otherwise`, and the database test `an_addition_keeps_its_reason_and_is_listed_until_a_later_order`.
