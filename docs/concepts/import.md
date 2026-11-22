---
type: Concept
title: Import
description: How a mail export becomes stored messages and parsed orders, what each message state means, and why nothing is dropped.
tags: [import, parsing, states]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
updated: 2026-11-21
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

`qm import` has two halves. The first reads the export and stores each message of a platform once: FreshDirect and six restaurant platforms ([ADR 0007](/decisions/0007-restaurant-mail-is-stored-at-import-and-parsed-later.md)). The second parses every stored FreshDirect receipt that waits. No parser reads restaurant mail yet.

New mail arrives in one of two ways, and both end in the same steps. `qm import` reads a Takeout export: the way to load the history. `qm fetch` reads the Gmail API with read-only access: the way new receipts arrive ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)). Both hand the use case the raw bytes of a message; the sender rule, the redaction, the key and the parse do not know which source it came from, and a message that both deliver is stored once.

## Storing messages

The mail source is the `TakeoutExport` adapter. Its path is a folder, one Takeout archive or one mailbox file. In a folder it reads every `.zip` and every `.mbox` file, to a depth of six folders, in path order, and it does not follow a symbolic link. One archive is open at a time; the log has a line when one is opened and when it is done. An archive is read in place: each entry whose name ends in `.mbox` is inflated as a stream and nothing is written to disk. The adapter sends one message at a time through a channel that holds at most 32, so memory does not grow with the export (REAS-RUST-007); when the use case stops, the reader stops at its next message.

A path that cannot be opened, a folder with no archive and no mailbox file, archives with no mailbox entry, a file that is not a readable archive, or a read that fails ends the run with an error and a failure exit status, and the parse step does not run (REAS-RUST-005). Messages read before the error are stored; the next run counts them as known. For each message, in order:

1. The MIME parts and a few headers are read: `Message-ID`, `Date`, `Subject`, `From`, `Authentication-Results`, the HTML body, the text body.
2. The message key is the `Message-ID` header, or the SHA-256 of the raw bytes when the header is missing or is no key: empty, longer than 998 bytes, or with a NUL character. The limit of 998 bytes is the longest line of a mail; the database has a unique index on the key, and an index holds no entry above about 2,700 bytes. A key that a version before 2026-11-21 stored can be longer, and is read as it is. A key already stored is skipped and counted as known. The import reads the stored keys once when it starts; a stored key whose text does not fit its kind (a `message_id` key that is empty, has whitespace at its start or its end or holds a NUL character, or a `sha256` key that is not 64 lowercase hexadecimal digits) stops the run with an error that names the row and not the text. The source hash is stored too and is unique, so a message that arrives under a new header is still stored once.
3. The platform comes from the exact From address: `import.senders` in the configuration are FreshDirect's, `import.restaurant_senders` lists the addresses of each restaurant platform. A message from any other address is counted and not stored.
4. The kind comes from the subject, by the rules of the platform: `placed`, `updated`, `receipt`, or `unknown`. Unknown is stored.
5. `dkim_pass` records whether an `Authentication-Results` header that Gmail itself added (authserv-id `mx.google.com`) says a DKIM signature verified for the domain of this message's sender or one of its subdomains. A pass from another signing domain, or a header another server added, does not count; a forged sender with its own valid signature would otherwise pass.
6. The redactor runs over the subject and both bodies ([Privacy](/concepts/privacy.md)). A message in which a configured term survives is refused: nothing of it is stored, the import continues, and the command exits with a failure status at the end.
7. A message with a NUL character in its subject or in a body after redaction is refused as a message that cannot be read: PostgreSQL holds no such character in a text. The import reports it as `malformed` with its key and continues. The redacted HTML never holds the character: the page reader removes it or puts U+FFFD in its place. The text body can hold it. When a mail has no text part, its text body is made from the HTML, and there a NUL character or the reference `&#0;` gives the character; such a mail is refused. A mail with a text part that has no NUL beside such an HTML part is stored.
8. One row of `raw_messages` is written, in state `fetched`.

## Parsing receipts

The second half reads every FreshDirect message in state `fetched`, oldest first. Restaurant messages stay in `fetched` until a parser exists for their platform.

* A placed, updated or unknown message is left for later; no parser reads those kinds yet.
* A receipt without a DKIM pass is parsed like any other, and its order is tagged `non-verified` in `grocery_orders.sender_check` ([ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md)). A receipt with no stored HTML body is refused (the mail library makes an HTML body from a text part, so that is a mail with neither part): outcome `failed`, reason recorded, nothing parsed.
* The parser registry asks each parser whether it recognizes the layout. None: outcome `unsupported`. The most confident one parses; a parse error is outcome `failed` with the error recorded.
* A parsed receipt is reconciled: the lines must sum to the subtotal, the subtotal and its additions to the total, and each priced line must be unit price times billed quantity, within one cent. A line with an options price or a special-price flag is exempt from the last check. Every mismatch becomes an anomaly, and the order is written with `reconciled = false`. The outcome is `ok` when everything reconciles and `partial` otherwise. A receipt whose amounts cannot be added at all, because a sum, a product or a gap of them is above 99,999,999.99, has no reconciliation: its outcome is `failed` and no order is written. A number above that is no amount for the parser: a line whose final price is such a number is a row that the parser cannot read, an anomaly; a unit price or an options price of that size is left out of its line; a totals row of that size is no totals row.
* Each line's name is normalized and resolved to an alias ([Identity](/concepts/identity.md)); an unseen alias creates a product. The normalized name and size are together at most 1,024 bytes, because the database has a unique index on the two. The parser does not read a row with a longer name: the row is an anomaly, and its reason gives the limit and not the name. If a parser gives such a line all the same, the attempt is outcome `failed` and no order is stored.
* Everything one attempt changes is written in one transaction: the attempt row, the order with its lines and anomalies, and the message's state. The state change is a conditional update on the state and version the message had when it was read; if it changed meanwhile, nothing is written and the message is counted as a conflict (REAS-STATE-003).

## What a row of the item table can be

The parser classifies every row of the item table by its content, never by a class name: a header row, a department name, an item, a totals line, or unknown. An unknown row becomes a row of `order_line_anomalies` with its raw cells and the order is still stored. The compiler enforces the rule: the row is an enum and every variant is matched.

## States

| State | Meaning |
| --- | --- |
| `fetched` | Stored; no parser has run, or none applies yet. |
| `parsed` | An order exists and reconciles. Anomalies may exist. |
| `partial` | An order exists and does not reconcile; the anomalies say why. |
| `failed` | A parser recognized the layout and could not read it, the amounts of the receipt cannot be added, or the message was refused. |
| `unsupported` | No parser recognizes the layout. A fixture and a parser version are the way forward. |

A message moves from `fetched` to one of the four outcomes on its first parse, and between outcomes on a reparse. Every attempt is a row of `parse_attempts`.

## What is not read yet

Placed and updated mails are stored and left in `fetched`. Their use, the diff against the receipt that shows substitutions and shortages, is a later milestone. Two receipts for one order number: the later one replaces the order rows; the earlier message stays `parsed`.
