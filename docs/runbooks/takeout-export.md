---
type: Runbook
title: Export your FreshDirect mail and import it
description: The one-time Google Takeout export of the receipts, the configuration file with your redaction terms, and the import.
tags: [runbook, import, takeout]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
sources:
  - id: reader
    resource: ../../src/adapters/mail/mod.rs
    title: The FreshDirect mail reader
  - id: config
    resource: ../../src/adapters/config.rs
    title: Settings
---

# Export your FreshDirect mail and import it

## Prerequisites

* `qm migrate` has run against your database ([Develop](/runbooks/develop.md)).
* About ten minutes for the Gmail and Takeout steps, then hours to a day for Google to build the export.

## Procedure

1. In Gmail, create a filter for `from:service@freshdirect.com` that applies a new label, `FreshDirect`. Tick "Also apply filter to matching conversations" so the old mail gets the label too.
2. Open <https://takeout.google.com>. Deselect everything, select Mail, choose "All Mail data included", deselect everything there and select the label `FreshDirect`. Request one export as a .zip.
3. When the mail from Google arrives, download the archive and unzip it into a directory outside the repository, for example `~/qm-data/takeout-2026-11/`. The export is `Takeout/Mail/FreshDirect.mbox` inside it. It carries your name, address, phone and card digits: never copy it into the repository, never print it in a Claude Code session. The guard hook refuses the common forms of that.
4. Write your redaction terms to `~/.config/qrtrmastr/config.toml`. Every line of your delivery address as the receipt prints it, your phone digits, your name as the greeting prints it and alone, your email, the last four digits of each card:

       [redaction]
       names = ["Your Name", "Your"]
       emails = ["you@example.com"]
       phones = ["2125550100"]
       address_lines = ["12 Example Street Apt 3", "Brooklyn, NY 11201"]
       card_last4 = ["1234"]

   The same terms, one per line, go into `~/.config/qrtrmastr/canary-terms.txt` for the local privacy canary.
5. Import:

       qm import --mbox ~/qm-data/takeout-2026-11/Takeout/Mail/FreshDirect.mbox

   Expected result: two lines of counts, for example

       messages read 312, new 312, already known 0, from other senders 0
       receipts parsed 101, partial 2, failed 0, unsupported 3, left for later 206, anomalies 9

   "Left for later" are the placed and updated mails, which no parser reads yet. "Unsupported" receipts are in a layout the parser does not know; they are stored and wait for a parser version. A "REFUSED" line means personal data survived redaction in a message; that message was not stored, the command exits with status 1, and you add the surviving term to the configuration and import again.
6. Check: `qm due --as-of <today>` prints a plausible list. `qm due --json` gives the same as data.

## Repeat later

Request a new Takeout export and import it again. Messages already stored are counted as known and skipped; only the new ones are added. That is the MVP's incremental sync.

## Recover

* A failed import is re-run as is; the import is idempotent.
* To make a stored receipt a parser fixture, `qm fixture add <message key>` writes its redacted HTML under `tests/fixtures/local/`, which git ignores. The key is printed by `qm review` in a later milestone; until then, read it from the database: `select message_key, subject from raw_messages where parse_state = 'unsupported'`.
* To start over, truncate the tables in a `psql` session and run `qm migrate`, or drop and recreate the development database as [Develop](/runbooks/develop.md) describes. Never do this to the Compose volume without a backup.
