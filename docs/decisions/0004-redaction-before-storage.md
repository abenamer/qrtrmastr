---
type: Architecture Decision
title: Redaction before storage
description: What the redactor removes, in what order, why a surviving term refuses the message, and why the export stays the durable original.
tags: [decision, privacy, redaction]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
updated: 2026-11-20
---

# ADR 0004 — Redaction before storage

## Context

Every FreshDirect mail carries the maintainer's name, email, delivery address, phone number and card digits. The store must keep the receipt HTML so a better parser can be run over old mail, and the repository may become public. Nothing personal may be written to the database or a log.

## Decision

* Redaction runs in memory before any row is written, in four passes ([Privacy](/concepts/privacy.md)): structural removal of the smallest HTML block that carries an address, payment or contact heading, of images and of comments, of link query strings and of any link whose path carries a term; normalization of the serialized markup (entities for spaces become spaces, invisible characters go) and literal replacement of every configured term, longest term first, with whitespace or tags allowed between its words; patterns for phone numbers, email addresses and masked card numbers; and an assertion independent of the passes, over the text with markup removed and spacing and case normalized, that no configured term survives and no long term's letters and digits appear in sequence.
* A message in which a term survives is refused: it is not stored, the import continues, and the command exits with a failure status.
* The configured terms live in the maintainer's configuration file outside the repository; the local privacy canary reads the same terms.
* A `redaction_version` is stored per message, so a stored body can be re-imported and re-redacted when a pass improves. The Takeout export and Gmail hold the originals; the store keeps a hash of the unredacted bytes for change detection only.
* The last four card digits are redacted only in payment context (`ending in`, masked digits), never as a bare four-digit number, which would eat prices and quantities.

## Consequences

* Redaction is lossy by design. A better parser may want text the redactor removed; the answer is a re-import with a better redactor, never a looser one.
* A term too short to redact safely (fewer than three characters) is refused at configuration time.
* The `regex` crate has no look-around, so word boundaries are captured and put back; the tests cover adjacent terms and names that are substrings of words.
* The first security review of this code found that the assertion reused the passes' own patterns and so could not catch what they missed: a term split by an inline tag, or a phone number with a non-breaking space inside it, was stored. Redaction version 2 normalizes the markup, lets tags sit between the words of a term, and asserts over the text view independently; the three cases are regression tests. An assertion that cannot be stronger than its passes is not an assertion.
* Construction errors name the kind and position of a bad term, never its value, and configuration errors name the key, never the value.

* 2026-11-18, redaction version 3: a sequence pass was added after the first import of real mail refused 1,979 messages for one cause, a long term without a word boundary or with other punctuation in a text body. The pass replaces what the assertion refuses; the assertion is unchanged. The same import showed a term the configuration lacked (a family name beside a middle initial) in a stored row: the assertion checks the configured terms only, so a missing term is not found.

* 2026-11-20, redaction version 7: two security reviews of versions 3 to 6 ran the old and the new redactor side by side and found that the order of the passes is part of the rule. The sequence pass replaced a name inside an email address that is not a configured term, in the local part or in the domain; the email pattern then did not match, and the rest of the address was stored, where version 2 removed it whole. The email pattern now admits a replacement on both sides of the `@` and runs until nothing changes. The optional country code of the phone patterns took the last digit of a ZIP code before a phone number, so the address term did not match and the rest of the ZIP code was stored. A first fix, a country code only after a character that is not a digit, was wrong: a `1` left in the text is glued to what stands before it, takes its boundary away, and a configured phone or a short term was then stored. The rule that holds: the terms are replaced before the phone numbers, so an address term claims its own ZIP code, and again after them, so a term gets back the boundary that a country code had covered. Every input of both reviews is a regression test. A pass that inserts a replacement must be tested against every pass after it, and a fix of the redactor is reviewed like the change it fixes.

## Alternatives

* Store the full MIME and redact on read: rejected; the database would hold personal data.
* Redact with a language model: rejected; nondeterministic, and the data would leave the machine.
