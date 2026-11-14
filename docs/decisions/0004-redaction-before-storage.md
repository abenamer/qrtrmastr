---
type: Architecture Decision
title: Redaction before storage
description: What the redactor removes, in what order, why a surviving term refuses the message, and why the export stays the durable original.
tags: [decision, privacy, redaction]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
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

## Alternatives

* Store the full MIME and redact on read: rejected; the database would hold personal data.
* Redact with a language model: rejected; nondeterministic, and the data would leave the machine.
