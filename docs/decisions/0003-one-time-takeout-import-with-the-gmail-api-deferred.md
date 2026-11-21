---
type: Architecture Decision
title: One-time Takeout import, with the Gmail API deferred
description: Why the MVP reads a Google Takeout export instead of the Gmail API, how the two paths will share a key, and what the mail source port costs.
tags: [decision, import, mail]
generated: { by: "process:claude-code", at: "2026-11-13T21:00:00Z" }
updated: 2026-11-20
---

# ADR 0003 — One-time Takeout import, with the Gmail API deferred

## Context

The receipts live in Gmail. Three ways to read them were compared: the Gmail API with the read-only scope, IMAP, and a Google Takeout export. The API is the only one where read-only is enforced by Google rather than promised, and it supports incremental sync; it needs a Google Cloud project to hold an OAuth client. IMAP needs an app password or the full-mail scope, both full access. Takeout is a one-time file with no credentials at all. The maintainer asked for one-time access for the MVP.

## Decision

* The MVP reads a Takeout export of the `FreshDirect` label through the `TakeoutExport` adapter of the `MailSource` port. No Google Cloud project, no OAuth, no credential on disk.
* The message key is the `Message-ID` header, else the SHA-256 of the raw bytes. Both Takeout and the Gmail API carry the header, so a later API adapter dedupes against what Takeout imported.
* The source hash is stored and unique as well, so a message that arrives under a different header is still stored once.
* A later export is imported the same way; known messages are skipped. That is the MVP's incremental sync.
* The Gmail API adapter is deferred: same port, same key, a Google Cloud project when the maintainer wants it.

* Amended 2026-11-18: the adapter reads the archives as Google delivers them. The path is a folder of `.zip` archives, one archive or one mailbox file; an archive is read in place with the `zip` crate (inflate only, through `flate2` and the Rust `zlib-rs`; no encryption, no other compression method), and the `MailSource` port is a bounded stream of messages, not a list. Reasons: the maintainer's first export was the whole mailbox, 8.4 GB in a 3 GB archive; the `qm` container has a read-only root and a 16 MB `/tmp`, so it cannot unzip; and an import must not need memory in proportion to the export. Alternatives rejected: unzip in the runbook (the maintainer keeps a second, larger copy of all mail on disk); shelling out to `unzip` (a second tool in the image and a writable directory); reading a whole mailbox into memory per file (worked for a label, not for a mailbox).

* Amended 2026-11-19: the Gmail API adapter exists ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)). A Takeout export stays the way to load the history.

## Consequences

* Nothing runs in the cloud and nothing authenticates. The export file carries personal data and lives outside the repository ([Export your FreshDirect mail](/runbooks/takeout-export.md)).
* New receipts arrive by hand, when the maintainer requests a new export.
* The `MailSource` port exists for one adapter today. It is the boundary the deferred adapter needs, and it lets the import use case be tested with an in-memory source.

## Alternatives

* Gmail API now: rejected for the MVP by the maintainer; the setup cost and the consent-screen expiry trap were not worth it for a one-time load.
* IMAP: rejected; read-only would be by discipline only.
