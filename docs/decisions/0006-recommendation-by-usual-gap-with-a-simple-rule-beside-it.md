---
type: Architecture Decision
title: Recommendation by usual gap, with a simple rule beside it
description: The two rules behind qm due, why neither is a forecast, the thresholds, and what would justify more.
tags: [decision, recommendation]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
---

# ADR 0006 — Recommendation by usual gap, with a simple rule beside it

## Context

The maintainer asked for a recommendation without forecasting: no model, no simulation, no seasons or weather in the MVP. The history is about a hundred orders over two years; most items appear in a handful of them.

## Decision

* The default rule: for each product, the median gap between its purchase dates is its usual gap; an item is overdue when more days than that have passed since the last purchase, and due soon when the next order, at the usual cadence between orders, would pass the gap ([Recommendation](/concepts/recommendation.md)). The typical quantity is the median billed quantity.
* The simple rule, `--simple`: items in at least half of all orders and absent from the last one. No dates.
* Thresholds: three purchases before an item is recommended, and an item not bought for three usual gaps is lapsed. Both in configuration.
* The day of the question is passed in; the domain never reads the clock.

## Consequences

* `qm due` is explainable in one sentence per row, and every number in a row is a count or a median of the history.
* It cannot say how likely a run-out is, and it treats a sale stock-up like any other purchase. The deferred design records the probabilistic model, the seasonality measure and the backtest that would justify replacing this rule; none of it ships until it beats this rule on held-out orders.

## Alternatives

* Croston-type intermittent-demand forecasting, a discrete-time hazard model with pooling across items, bootstrap probabilities: deferred, not rejected, pending a prediction log and a backtest.
* A language model: rejected for the rules; arithmetic over a few thousand rows is deterministic and testable.

## Runs

None yet. The first measurement is the held-out backtest the deferred model needs, and it will be recorded here against this rule as the baseline.
