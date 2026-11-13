---
type: Concept
title: Privacy
description: What personal data the mail carries, what the store keeps, and the two canaries that guard the repository.
tags: [privacy, security, redaction]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: canary
    resource: ../../scripts/pii_canary.sh
    title: The privacy canary
  - id: guard
    resource: ../../.claude/hooks/guard-bash.sh
    title: The guard hook
  - id: schema
    resource: ../../migrations/0001_raw_mail.sql
    title: raw_messages
---

# Privacy

## What the mail carries

Every FreshDirect mail carries the maintainer's name, email address, delivery address, phone number, and the last four digits of the payment card. None of these is stored. The repository may become public, so the rule is stricter than "not in the database": nothing that came from a real receipt is committed, even redacted.

## What the store keeps

`raw_messages` holds each message after redaction: its key, kind, date, subject, the redacted HTML and text, and a hash of the unredacted bytes for change detection. The hash is one-way. The order tables hold what the receipt says about products, quantities, prices and dates, and nothing about the person.

The redactor runs before any row is written ([ADR 0004](/decisions/0004-redaction-before-storage.md)). It removes the smallest HTML block that carries an address, payment or contact heading, every image and every HTML comment, the query string of every link, and the whole address of a link whose path carries a configured term; normalizes the serialized markup so that entities for spaces and invisible characters cannot split a term; replaces each configured redaction term, longest term first, with any whitespace or tags between its words; applies patterns for phone numbers, masked card numbers and email addresses; and then asserts, over the text a reader would see with tags and markup gone, that no configured term survives in any spacing or case and that no long term's letters and digits appear in sequence. The assertion does not reuse the passes' patterns, so a form the passes did not model is refused rather than stored. A message that fails the assertion is not stored, the import continues, and the command exits with a failure status at the end. A `Message-ID` that carries a configured term is replaced by the hash of the message, so it is neither stored nor printed.

The redaction terms live in the maintainer's configuration directory, outside the repository. The Takeout export lives under `~/qm-data/`, outside the repository. Both paths are denied to the assistant's file reads in `.claude/settings.json`, and every shell command outside a short allow list asks the maintainer first. The guard hook is a best-effort backstop behind those two controls: it refuses the common forms of a command that names an export file outside the import tooling. A pattern match over command text is not a security boundary, and the documents do not treat it as one.

## The two canaries

`scripts/pii_canary.sh` runs in two modes.

* Shape mode, in CI on every pull request and push: fails when a file under `tests/fixtures/local/` or `backups/` is tracked, or when a tracked fixture, document, migration or source file matches the shape of a phone number, an email address, a masked card number, or a street address. The canary fails closed: a file it cannot read is a failure, not a pass, and file names reach `grep` as arguments so a space in a name changes nothing.
* Term mode, in the Stop hook on the maintainer's machine: fails when any line of `~/.config/qrtrmastr/canary-terms.txt`, the real address, phone, name and email, appears in a tracked file. CI cannot run this mode because it does not know the terms, which is the point.

The synthetic fixtures under `tests/fixtures/synthetic/input/` carry fake personal data of the right shapes on purpose, in reserved forms that cannot be real: a 555-01xx phone number, an `example.com` address, an invented street. The canary skips that directory and scans what the redactor produces from it: the parser snapshot under `src/`, and the tests assert the fake data is gone. A redactor regression therefore fails the tests, and anything it let through into a committed snapshot fails the canary.

`qm fixture add <key>` writes a stored, redacted real receipt under `tests/fixtures/local/`, which git ignores and the canary refuses to see tracked. The tests use those files when present, so a parser can be built against real layouts without a real receipt ever entering git.

## Residual risk

A redacted receipt still names what one household buys. That is why real receipts stay out of git even after redaction. The database itself is local; a backup of it is treated like the database: `backups/` is the Compose mount point for dumps, its contents are ignored by git, and the canary fails if anything but its `.gitkeep` is tracked.
