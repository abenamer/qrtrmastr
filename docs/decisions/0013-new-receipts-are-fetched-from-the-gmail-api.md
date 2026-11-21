---
type: Architecture Decision
title: New receipts are fetched from the Gmail API
description: Why qm fetch reads new receipts from the Gmail API with the read-only scope, how the sign-in works without an open port, where the token lives, and the limits of the adapter.
tags: [decision, import, mail, security]
generated: { by: "process:claude-code", at: "2026-11-19T20:00:00Z" }
updated: 2026-11-20
---

# ADR 0013 — New receipts are fetched from the Gmail API

## Context

[ADR 0003](/decisions/0003-one-time-takeout-import-with-the-gmail-api-deferred.md) deferred the Gmail API: a Takeout export was enough for the first load. The lists of `qm due` now come from the history, and the maintainer's direction is that buying on FreshDirect is the input: a new order must reach the tool without an export by hand.

## Decision

* `qm fetch` reads new mail through the Gmail API with the scope `gmail.readonly` (`GmailApi` in `src/adapters/mail/gmail.rs`, an adapter of the `MailSource` port). A message is read raw: the same bytes a Takeout export holds. The sender rule, the redaction with its assertion, the key and the parse are the ones of `qm import`; a message that both paths deliver is stored once.
* The search is the configured sender addresses and the mail received since a week before the newest stored message (`RawMessageStore::newest_received_at`), or since `--since`. One run reads at most `--max` messages, 500 by default.
* The OAuth client belongs to the user: a Google Cloud project of their own, client type "Desktop app", given to the tool as `QM_GMAIL_CLIENT_ID` and `QM_GMAIL_CLIENT_SECRET`. The repository holds no client.
* `qm gmail login` is the installed-application flow with PKCE (S256) and a random state. The tool opens no port: Google sends the browser to `http://127.0.0.1:8765`, where nothing listens, and the user pastes that address back. The address is accepted only with the host, the port and the state of this attempt.
* The refresh token is kept in `gmail-token.json` in the state directory (`QM_STATE`, `/state` in the container, `~/.local/state/qrtrmastr` on the host), written with mode 0600 to a new file and moved into place. It is not in the database, so a database backup does not hold it.
* The HTTP client is `ureq`, blocking, with the `rustls` and `ring` crates the project already has through SQLx. Each call runs on the blocking pool.

## Consequences

* A credential is on disk for the first time. The token gives read access to all mail of the account, not only to the receipts: Google has no narrower scope that returns a body. Whoever can read the file can read the mail until the access is removed at https://myaccount.google.com/permissions.
* The limits of REAS-SEC-001 for the adapter: HTTPS only; no redirect is followed; 60 seconds for a call and 10 to connect; 1 MB for a token answer or a page of the list and 64 MB for a message; a limit on the messages of a run; a message id is letters and digits before it becomes part of a path; a sender address is cut to the characters of an address before it becomes part of the search.
* No token, code, client secret or message body is part of an error or a log line. An error from Google is reduced to the status and a short code. The HTTP library itself prints every byte it sends and receives at the `trace` level, tokens and raw messages among them; the log filter holds its targets at `debug` at most, whatever `RUST_LOG` asks (`log_line_is_safe` in `src/adapters/cli/mod.rs`).
* A Google Cloud project in "Testing" gives refresh tokens that end after seven days; the runbook says to publish the app. The tool then says `run qm gmail login again`.
* Nothing runs on a timer yet: a fetch happens when `qm fetch` runs.
* 13 crates are new in the lockfile; `ureq`, `ureq-proto`, `utf8-zero`, `http` and `httparse` are compiled, with `base64`, `url` and `getrandom`, which were already there through other crates.

## Alternatives

* A listener on the loopback address for the redirect: needs a published port of the container for one sign-in.
* The token in PostgreSQL: rejected; a backup would carry it.
* `reqwest`: an asynchronous client with a larger tree (hyper, h2, tower) and, in its current version, another TLS provider than the one the project has.
* IMAP: rejected in ADR 0003; read-only would be by discipline only.
* Gmail push notifications: need a public endpoint.

## Runs

2026-11-19. Unit tests of the adapter against a local HTTP server on the loopback address (plain HTTP, tests only): the sign-in and the refresh, the paging of the list, the raw messages with and without padding, the limit of a run, a 401, a revoked sign-in, an id that would leave the path, and the refusal of plain HTTP by the real client; the PKCE challenge against the vector of RFC 7636; the pasted address; the search text; the token file and its mode. `cargo deny check`: bans, licenses and sources pass; advisories fail on `chrono` and `chrono-tz` marked unmaintained, as they do without this change. Not run: a sign-in and a fetch against Google; they need the maintainer's Google Cloud project.
