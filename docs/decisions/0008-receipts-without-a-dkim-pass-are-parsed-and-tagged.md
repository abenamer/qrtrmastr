---
type: Architecture Decision
title: Receipts without a DKIM pass are parsed and tagged
description: Why the parse step no longer refuses a receipt without a DKIM pass, what the tag on the order means, and the backtest of the FreshDirect parser version 2 over the stored receipts.
tags: [decision, parsing, import]
generated: { by: "process:claude-code", at: "2026-11-18T18:00:00Z" }
---

# ADR 0008 — Receipts without a DKIM pass are parsed and tagged

## Context

The first import of the maintainer's mailbox stored 510 FreshDirect receipts and parsed none. 429 were refused because Gmail recorded no DKIM pass for `freshdirect.com`: every FreshDirect mail before 2025-06-19 lacks one. The other 81 failed with `no delivery date found`, because parser version 1 was written against an invented layout. The maintainer's decision: the mailbox is the maintainer's own, a receipt is read-only data, and a forged receipt could do no more than add a wrong order to a private history; read them all and tag the unverified ones.

## Decision

* The parse step does not refuse a receipt without a DKIM pass. `raw_messages.dkim_pass` still records the result for each message.
* An order carries `sender_check`: `verified` when the receipt had the pass, `non-verified` when it did not (`SenderCheck` in `src/domain/states.rs`, migration 0004). A query can leave the non-verified orders out.
* The exact sender address is still the rule for what the import stores ([ADR 0007](/decisions/0007-restaurant-mail-is-stored-at-import-and-parsed-later.md)).
* The FreshDirect parser is version 2. It reads the delivery from the sentence the real receipt prints, sums the subtotals of an order from two stores, takes `Total Tax` for the tax rows, counts a fuel surcharge as a fee, and knows the title, order, summary and legend rows of the real layout. The layout was read from stored, redacted rows; the fixture `tests/fixtures/synthetic/input/freshdirect/2024/receipt-two-stores.html` is invented and follows that structure.

## Consequences

* The order history starts in 2017 and not in the middle of 2025.
* A forged receipt from the exact sender address would be stored and parsed. The tag says which orders had no pass; nothing checks them further.
* The security review of 2026-11-20 traced what such a forged receipt can do; it is more than "add a wrong order", and none of it is prevented yet:
    * It can replace a genuine order. The order write deletes the row with the same order number first, whatever its tag. The sender must know the order number; the replaced row then reads `non-verified`, and the genuine message stays stored.
    * Its lines count in `qm due`. The `purchases` view does not carry `sender_check`, so no query leaves the non-verified orders out today. A forged line with the name of a real product, dated far ahead, changes when that product is due.
    * It can stop the parse step. An amount larger than the column holds, or one that overflows the subtraction in the reconciliation, ends the run with an error or a panic; the message stays `fetched` and stops every later run until it is removed by hand.
* What it cannot do: reach SQL text, a file path, a shell or a network request; change the tag of another message; or obtain `verified` without the header that Gmail adds.
* An order that does not reconcile is stored as `partial` with its lines and its anomalies. Its lines count in `qm due`.

## Alternatives

* Accept an SPF pass in place of DKIM: not done; it needs the headers of old mail examined, and the maintainer chose to read everything.
* Leave out everything before 2025-06-19: rejected; it removes eight years of history.

## Runs

* 2026-11-18, parser version 2 over the 510 stored, redacted FreshDirect receipts of 2017 to 2026 (`backtest_over_stored_receipts`, a local export outside the repository): 510 parsed, 0 failed, 0 unsupported, 7,986 lines. 208 reconcile. 302 are partial: in 226 a line's printed price is not its unit price times the billed quantity, in 92 the subtotal and its additions do not sum to the total, in 34 the lines do not sum to the subtotal (an order can have more than one cause). Rows the parser could not classify: 108 with no rule, 45 with a quantity cell that does not hold two positive numbers, 4 with a price of `$ FREE`. The causes of the partial orders are not examined yet.
