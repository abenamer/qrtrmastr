---
type: Architecture Decision
title: The store keeps the items of a receipt and not the mail
description: Why the import parses a receipt while it reads it and stores no subject and no body of a mail, what follows for a parser that gets better, and what happens to the mail that is stored.
tags: [decision, privacy, import]
generated: { by: "process:claude-code", at: "2026-11-24T23:30:00Z" }
updated: 2026-11-24
sources:
  - id: import
    resource: ../../src/application/import.rs
    title: The import use case, which returns the pages of the receipts and stores none
  - id: parse
    resource: ../../src/application/parse.rs
    title: The parse use case, which reads a page of the same run
  - id: adr-0004
    resource: 0004-redaction-before-storage.md
    title: ADR 0004, which this decision replaces
---

# ADR 0021: The store keeps the items of a receipt and not the mail

## Status

Accepted by the maintainer on 2026-11-24. It replaces [ADR 0004](/decisions/0004-redaction-before-storage.md), redaction before storage, for each mail that is imported from that day on. Steps 1 and 2 of three are done.

## Context

The purpose of the program is to read the items of a grocery receipt and to say what to order next. Until now it stored each mail, redacted, so that a better parser could read old mail again without the mailbox. A mail carries the name, the address, the phone number and the card of the maintainer, so the stored copy needed a redactor, and the redactor needed its terms in the configuration, its own tests, its fuzz target, a design review and a version for each change.

On 2026-11-24 a change of the redactor grew for a day around inputs that nobody had seen in a real receipt. The maintainer asked why the program does these things when it only has to extract food items, and decided: store the extracted items only, stay in that lane, and say what the limits are. The redaction before storage is one of the decisions that [ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md) names as costly to reverse; this is that reversal, made on purpose.

## Decision

* The system shall store, of a mail, a record with its key, its platform, its kind, the moment of its `Date` header, whether it had a DKIM pass, the hash of its bytes, and the state of its parse.
* The system shall not store the subject, the HTML body or the text body of a mail.
* When an import reads a receipt, the system shall keep the page of the mail in memory until the parse run of the same command has read it.
* When a receipt is parsed, the system shall store the order and its lines, as before. For a row of the receipt that no rule of the parser read, the system shall store the place of the row and the reason, and no cell of the row.
* If a cell that stands alone in a row holds four digits one after another, then the parser shall not read it as a department.
* While a message has no order that reconciles, the system shall parse it again in each run that reads its mail again.
* If a message waits for a parser and the run did not read its mail, then the system shall write nothing for it and shall say how many such messages there are.
* The debug form of the pages of a run shall print their count and no text of a page.

The mail stays in Gmail and in the Takeout export. A parser that gets better reads a receipt again when the maintainer imports the mail again.

Three steps, each its own pull request:

1. The import parses while it reads and stores no text of a mail. This record. Nothing that is stored is removed.
2. What is no longer used goes: the redactor, the `[redaction]` section of the configuration, `qm fixture add`, their tests, their fuzz target and their documents.
3. A command that the maintainer runs one time sets the stored subject and bodies of each older row to nothing and keeps the orders. The maintainer chose this on 2026-11-24, over an empty database and over rows that stay.

## Consequences

* An import of today writes no subject and no body of a mail. What it writes of a receipt is what the parser extracted: the order number, the dates, the totals, and for each line the name of the product as the receipt prints it, its department, its quantities and prices, and the path of its link.
* The parser now reads the page as the sender wrote it, where it read a redacted page before. Two things follow from the review of this change. A row that no rule read was stored cell by cell, and in the real layout one such row holds the line of the payment card: the cells are no longer stored. A cell that stands alone was a department when it was short and had no dollar sign: a cell with four digits in a row is none now. Limits: a department, a product name and the path of a product link are stored as the receipt has them, and a sender that puts a token of the recipient into the path of a link puts it into the store.
* Since step 2 (2026-11-27) the program has no redactor, no `[redaction]` section and no `qm fixture add`. A section of that name in a file of before is not read.
* A row of before 2026-11-24 keeps its redacted subject and bodies until step 3. A row that was never parsed is parsed one time from its stored page.
* A message that was stored under the hash of its bytes, because its `Message-ID` held a redaction term, does not get a new attempt from a later import: the import of today files its page under the `Message-ID`. A new attempt that fails for a message whose earlier attempt stored an order leaves that order; this needs a parser that changed.
* A receipt that a parser could not read, or whose order does not reconcile, gets a new attempt only when its mail is imported again. `qm fetch` asks Gmail for the mail since the newest stored one, so it does not bring an old receipt back; a Takeout import does.
* Each import of a whole export parses again each receipt that has no order that reconciles, and writes one attempt row for each.
* No fixture can be made from a real receipt after step 2. The tests use the synthetic receipts, as CI does.
* The mail of a restaurant platform gets a record and no content. A parser for such a platform reads the mail when it is imported again.
* The `Message-ID` of a mail is its key and is stored as the sender wrote it. Until 2026-11-24 a header that held a configured term was replaced by the hash of the mail. A sender that writes the address of the recipient into that header puts it into the key; no real mail of the platforms is known to do so. This is a limit.
* The plan of [ADR 0019](/decisions/0019-the-gaps-that-remain-after-the-ten-steps.md) is paused. Lines 1 to 3 and most findings of the design review are about the stored mail and the redactor. After step 3 the maintainer gets a short list of what is left for a program that extracts items, and chooses from it.
* The self-audit describes the program of before this decision in its rows on the redactor and the stored mail. It is read again after step 3.

## Alternatives

* Keep the redacted mail and make the redactor better. That was redaction version 9, written on 2026-11-24 and not merged: it kept less of a mail and still stored one.
* Store the table of the items of each mail and not the whole mail. Not chosen: it still stores text of a mail and still needs a part of the redactor.
* An empty database and a new import. Not chosen by the maintainer: the additions that were made by hand would be lost.

## Runs

No run on real mail. The tests of 2026-11-24 are in [the log](/log.md) of that day.

| Check | Result on 2026-11-24 |
| --- | --- |
| After an import and a parse of the synthetic receipts, the rows of `raw_messages` with a subject, a body or a redaction version other than 0, and the anomalies with a stored cell | None (`importing_the_same_export_twice_stores_everything_once`). |
| A cell that stands alone with the line of a payment card, and four department names | No department, and four departments (`a_lone_cell_with_four_digits_in_a_row_is_no_department`). |
| A receipt that failed, in a run without its mail and in a run with it | No attempt, then one new attempt (`a_receipt_whose_amounts_cannot_be_added_is_a_failed_attempt_and_no_order`). |
| A row of before with a stored page | Parsed one time from the page (`a_row_of_before_with_a_stored_page_is_parsed_from_it_once`). |
