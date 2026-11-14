---
type: Concept
title: Import
description: How a mail export becomes stored messages and parsed orders, what each message state means, and why nothing is dropped.
tags: [import, parsing, states]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
sources:
  - id: import
    resource: ../../src/application/import.rs
    title: ImportMail use case
  - id: parse
    resource: ../../src/application/parse.rs
    title: ParsePending use case
  - id: parser
    resource: ../../src/adapters/receipts/freshdirect_2026.rs
    title: The 2026 receipt parser
  - id: states
    resource: ../../src/domain/states.rs
    title: ParseState
---

# Import

`qm import` has two halves. The first reads the export and stores each message once. The second parses every stored receipt that waits.

## Storing messages

For each message of the export, in order:

1. The MIME parts and a few headers are read: `Message-ID`, `Date`, `Subject`, `From`, `Authentication-Results`, the HTML body, the text body.
2. The message key is the `Message-ID` header, or the SHA-256 of the raw bytes when the header is missing. A key already stored is skipped and counted as known. The source hash is stored too and is unique, so a message that arrives under a new header is still stored once.
3. A message not from the platform's sender (`import.senders` in the configuration) is counted and not stored.
4. The kind comes from the subject: `placed`, `updated`, `receipt`, or `unknown`. Unknown is stored.
5. `dkim_pass` records whether an `Authentication-Results` header that Gmail itself added (authserv-id `mx.google.com`) says a DKIM signature verified for the sender's domain or one of its subdomains. A pass from another signing domain, or a header another server added, does not count; a forged sender with its own valid signature would otherwise pass.
6. The redactor runs over the subject and both bodies ([Privacy](/concepts/privacy.md)). A message in which a configured term survives is refused: nothing of it is stored, the import continues, and the command exits with a failure status at the end.
7. One row of `raw_messages` is written, in state `fetched`.

## Parsing receipts

The second half reads every message of the platform in state `fetched`, oldest first:

* A placed, updated or unknown message is left for later; no parser reads those kinds yet.
* A receipt without a DKIM pass is refused: outcome `failed`, reason recorded, nothing parsed. A receipt without an HTML body is refused the same way.
* The parser registry asks each parser whether it recognizes the layout. None: outcome `unsupported`. The most confident one parses; a parse error is outcome `failed` with the error recorded.
* A parsed receipt is reconciled: the lines must sum to the subtotal, the subtotal and its additions to the total, and each priced line must be unit price times billed quantity, within one cent. A line with an options price or a special-price flag is exempt from the last check. Every mismatch becomes an anomaly, and the order is written with `reconciled = false`. The outcome is `ok` when everything reconciles and `partial` otherwise.
* Each line's name is normalized and resolved to an alias ([Identity](/concepts/identity.md)); an unseen alias creates a product.
* Everything one attempt changes is written in one transaction: the attempt row, the order with its lines and anomalies, and the message's state. The state change is a conditional update on the state and version the message had when it was read; if it changed meanwhile, nothing is written and the message is counted as a conflict (REAS-STATE-003).

## What a row of the item table can be

The parser classifies every row of the item table by its content, never by a class name: a header row, a department name, an item, a totals line, or unknown. An unknown row becomes a row of `order_line_anomalies` with its raw cells and the order is still stored. The compiler enforces the rule: the row is an enum and every variant is matched.

## States

| State | Meaning |
| --- | --- |
| `fetched` | Stored; no parser has run, or none applies yet. |
| `parsed` | An order exists and reconciles. Anomalies may exist. |
| `partial` | An order exists and does not reconcile; the anomalies say why. |
| `failed` | A parser recognized the layout and could not read it, or the message was refused. |
| `unsupported` | No parser recognizes the layout. A fixture and a parser version are the way forward. |

A message moves from `fetched` to one of the four outcomes on its first parse, and between outcomes on a reparse. Every attempt is a row of `parse_attempts`.

## What is not read yet

Placed and updated mails are stored and left in `fetched`. Their use, the diff against the receipt that shows substitutions and shortages, is a later milestone. Two receipts for one order number: the later one replaces the order rows; the earlier message stays `parsed`.
