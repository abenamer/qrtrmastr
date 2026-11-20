---
type: Architecture Decision
title: Restaurant mail is stored at import and parsed later
description: Why the import stores the order mail of six restaurant platforms beside FreshDirect before a parser exists for any of them, and which rules decide what is stored.
tags: [decision, import, mail]
generated: { by: "process:claude-code", at: "2026-11-18T16:00:00Z" }
---

# ADR 0007 — Restaurant mail is stored at import and parsed later

## Context

The export that arrived is the whole mailbox: six archives, 32 GB compressed, about 100 GB of mail. One pass over it takes a long time. The maintainer asked that the import read one archive at a time and keep the FreshDirect, Seamless, Grubhub and other restaurant receipts. Restaurant orders were in the deferred design; no parser, table or fixture exists for them, and the assistant must not open real mail to write one.

## Decision

* The import stores the order mail of six restaurant platforms beside FreshDirect: `seamless`, `grubhub`, `toast`, `square`, `clover`, `doordash`. Each is a value of `Platform`; migration 0003 widens `raw_messages_platform_check` to the same list.
* A message belongs to a platform by its exact From address. The addresses are configuration (`import.senders` for FreshDirect, `import.restaurant_senders` for the others); the defaults come from the inbox inventory of 2026-11-13, senders and subjects only. A marketing address of the same company is another address and is not stored.
* One reader, `PlatformMail`, serves every platform. The steps are the same; only the sender list and the subject rules differ, and both are data.
* The kind comes from the subject, by platform. Seamless and Grubhub: "Thanks for your ... order" is a receipt. Square: "Receipt from ..." is a receipt. DoorDash: "Order Confirmation for" is placed, "Final receipt" is a receipt, "Adjustment" is updated. Toast and Clover: a subject with the word "receipt" is a receipt. Anything else is `unknown`, and unknown is stored, as for FreshDirect.
* The DKIM pass must be for the domain of the message's own sender.
* Redaction is the same and comes first ([ADR 0004](/decisions/0004-redaction-before-storage.md)): a message in which a configured term survives is refused.
* No restaurant parser ships. The messages stay in state `fetched`. The parse step of `qm import` reads FreshDirect only.
* The archives are read in place, one at a time, in name order ([ADR 0003](/decisions/0003-one-time-takeout-import-with-the-gmail-api-deferred.md)). Nothing is extracted to disk: the exports directory is mounted read-only, and 100 GB of unzipped mail has no use after the pass.

## Consequences

* One pass over the mailbox is enough. A later parser version reads the stored, redacted rows; the export is not needed again.
* DoorDash and Square send promotions and non-food receipts from the same address. Those are stored as `unknown` or as receipts of a non-food merchant; sorting them is the work of the parser and of merchant classification.
* A platform that is not in the list, or an order address that is not in the configuration, is not stored. The count "from other senders" does not say which senders; a second pass is needed after the configuration changes.
* The stored bodies of restaurant mail carry more free text than a grocery receipt (delivery instructions, a courier's first name). Redaction removes the configured terms only.

## Alternatives

* Wait for the parsers and import restaurant mail then: rejected, it costs a second pass over 100 GB for each platform.
* Store every message with the word "receipt" in the subject, from any sender: rejected, it stores mail from senders nobody examined, and redaction was written for known layouts.
* Extract each archive to disk, import, delete: rejected, it needs a writable exports directory and up to 50 GB of temporary space, and gives nothing the stream does not.

## Runs

* 2026-11-18: the subject rules are fixture-tested with synthetic subjects (`classify::tests::subjects_classify`), the reader with a synthetic Seamless message, and the store with `a_restaurant_receipt_is_stored_under_its_platform_and_waits_for_a_parser` against PostgreSQL. No rule was checked against real mail. The Toast and Clover rule and the address `orders@eat.grubhub.com` are assumptions; the first import shows whether rows arrive for them.
