---
type: Technical Reference
title: Design review of the redactor and the Gmail adapter
description: A review after the fact of the two components that handle personal data and credentials, against least privilege, fail-safe defaults, complete mediation and defense in depth, with 17 findings.
tags: [security, privacy, review, redaction, gmail]
generated: { by: "process:claude-code", at: "2026-11-22T02:25:00Z" }
updated: 2026-11-21
sources:
  - id: redactor
    resource: ../../src/adapters/mail/redact.rs
    title: The redactor
  - id: gmail
    resource: ../../src/adapters/mail/gmail.rs
    title: The Gmail adapter
  - id: reas-sec-001
    resource: ../../standards/reas/standard/security.md
    title: REAS-SEC-001, a design review for a component that handles personal data or credentials
---

# Design review of the redactor and the Gmail adapter

A review agent of a Claude Code session wrote this record on 2026-11-21, at commit `2f5ea23`. A line number in it is a line of that commit. The session read five of the findings that the record calls new against the code before it committed the record: F1, F3, F9, F10 and F15. No finding is corrected in the change that adds this page; each finding that is new is open, and the [self-audit](/self-audit.md) names them in the row of REAS-SEC-001. A finding that is corrected later gets a note here with its date, and the text of the finding stays.

## What this record is

REAS-SEC-001 asks for a design review before a component that handles personal data or credentials is written (`standards/reas/standard/security.md:50`).
Both components exist already.
The redactor is at redaction version 8 (`src/adapters/mail/redact.rs:36`).
The Gmail adapter was committed on 2026-11-20 (`docs/log.md:26`).
This record is therefore a review after the fact.
It does not say that a review came before the code.
The self-audit names the missing review as a gap of REAS-SEC-001 (`docs/self-audit.md:80`).

The review reads the code at commit `2f5ea23` on the branch `records-for-step-9`.
The review changes no file of the repository.

The exposure is the answer to question 5 of ADR 0001 (`docs/decisions/0001-ports-and-adapters-for-a-private-tool.md:22-27`).
Any party that can send mail to the mailbox can supply outside data.
A From address can be forged, and a mail without a DKIM pass is stored and parsed.
Google supplies the JSON of each answer.
The maintainer supplies the configuration and the command line.

### What was read

* `src/adapters/mail/redact.rs`, `src/adapters/mail/mod.rs`, `src/adapters/mail/mime.rs`, `src/adapters/mail/classify.rs`, `src/adapters/mail/gmail.rs`.
* `src/adapters/config.rs`, `src/adapters/cli/mod.rs`, `src/main.rs`, `src/application/import.rs`, `src/adapters/fixtures.rs`, `src/application/fixture.rs`.
* The queries `body_by_key`, `insert`, `pending` and `newest_received_at` of `src/adapters/postgres/mod.rs`, and `migrations/0001_raw_mail.sql`.
* `compose.yaml`, `Cargo.toml`, `fuzz/fuzz_targets/redact.rs`.
* `docs/concepts/privacy.md`, ADR 0001, 0004, 0013, 0015 and 0017, `docs/self-audit.md`, `docs/log.md`, `docs/runbooks/gmail-fetch.md`.
* Of the HTTP library `ureq` 3.5.2: its feature list, its default for a proxy, and its log calls.

### What ran

Three commands ran on 2026-11-21 at commit `2f5ea23`.
The working tree had changes from other work in `.github/workflows/ci.yml`, `scripts/check.sh` and `scripts/check.test.sh`, and two files that git does not track; none is below `src/`.

| Command | Outcome |
| --- | --- |
| `cargo test --locked --lib adapters::mail` | Passed: 74 tests. |
| `cargo test --locked --lib adapters::cli::tests::no_setting` | Passed: 1 test. |
| `cargo test --locked --lib adapters::config` | Passed: 5 tests. |

Not run: every group of `scripts/check.sh`, the fuzz targets, the integration tests, a sign-in, and a request to Google.

### Limits of this record

One assistant made this review in one reading.
A statement with the words "by reading" has no test behind it.
This review writes one file only, so it adds no test for a gap that it found.
The review did not read the mail library, the HTML library or the TLS library.
The review did not read the Takeout reader, which is a third path of mail bytes.

## Component 1: the redactor

The redactor is `Redactor` in `src/adapters/mail/redact.rs`.
`PlatformMail::prepare` calls it (`src/adapters/mail/mod.rs:41-131`).
`import_from` builds it from the configuration (`src/main.rs:104-107`).

### Least privilege

What the design does:

* The redactor opens no file, no socket and no database connection. It takes text and returns text or an error (`redact.rs:470`, `redact.rs:540`).
* The terms enter through one constructor, which takes them by value (`redact.rs:326`).
* `Redactor` does not implement `Debug` (`redact.rs:243-249`). No error of the redactor holds the text of a term: an error names the kind and the position (`redact.rs:61-103`). The test `short_or_malformed_terms_are_refused_without_echoing_them` holds this (`redact.rs:968`).
* A pattern that does not compile gives `None`, so the text of the pattern error, which quotes the pattern, is dropped (`redact.rs:292-306`).
* `FileConfig` prints `[REDACTED]` in place of the terms in its `Debug` form (`src/adapters/config.rs:225-233`). The test `the_file_config_has_working_defaults_and_hides_the_terms_in_debug` holds this (`config.rs:350`).
* A configuration error names the key and the kind of the problem and never the value (`config.rs:72-94`). The test `a_configuration_error_names_the_key_and_never_the_value` holds this (`config.rs:447`).
* Compose mounts the configuration directory read-only (`compose.yaml:62`).
* The reader takes only the first HTML part and the first text part of a mail. It does not read attachments or the To header (`src/adapters/mail/mime.rs:41-63`).
* Mail from an address that is not a configured sender is counted and not stored (`mod.rs:58-63`, `src/application/import.rs:96`).

Where the design meets the principle only in part:

* `RedactionTerms` derives `Debug` and `Clone` (`redact.rs:41`). A `{:?}` of the terms themselves prints every term, because only `FileConfig` hides them. No code prints them today, by a search of `src/`.
* A copy of the terms stays in memory for the whole run. `Settings::redaction_terms` gives the redactor a clone (`config.rs:337-342`), and `Settings` keeps the first copy. The reason on `Redactor::new` says that no copy stays with the caller (`redact.rs:309-312`). That is true for the clone and not for `Settings`.
* More is stored than a reader of the store uses. The redacted text body goes into `raw_messages.body_text` (`src/adapters/postgres/mod.rs:237-250`). No query of the program reads that column, by a search for the name in `src/`. ADR 0007 keeps restaurant mail for a later parser.
* A mail of a configured sender is stored when its kind is `Unknown`, for example a promotion (`src/adapters/mail/classify.rs:35-37`).
* The store keeps the SHA-256 of the bytes before redaction (`mod.rs:42`, `migrations/0001_raw_mail.sql:16`). The hash is one-way. A party that holds the store and a copy of a mail can confirm that the mail is the stored one.

### Fail-safe defaults

What the design does:

* A command that redacts refuses to run when the configuration has no `[redaction]` section (`config.rs:337-342`, `src/main.rs:105`). The section has no default on purpose (`config.rs:211-218`).
* A name, mail address or address line of fewer than three characters refuses the construction. An empty term and a term of spaces are such terms (`redact.rs:334-337`).
* A phone of fewer than seven digits, and a card term that is not exactly four digits, refuse the construction (`redact.rs:378-380`, `redact.rs:399-401`).
* A term whose pattern does not compile within the size limit refuses the construction (`redact.rs:338-346`). The test `a_term_whose_pattern_passes_the_size_limit_is_refused_by_its_position` holds this (`redact.rs:1131`).
* When the construction fails, `import_from` returns before it connects to the database or reads a mail (`main.rs:106-108`).
* `redact_text` returns its result only after `verify` passes (`redact.rs:574-575`).
* When `verify` fails for the subject, the HTML body or the text body, `prepare` returns an error and builds no message (`mod.rs:85-100`). The import stores nothing of that mail (`import.rs:115-118`).
* The report of a refused mail holds the message key, the kind of the term and a fixed text for the check. It holds no part of the mail body (`mod.rs:76-84`, `redact.rs:172-177`).
* The command ends with a failure status when a mail was refused (`main.rs:115-122`).
* A `Message-ID` that carries a configured term is not the key. The hash of the mail is the key (`mod.rs:51-56`). The test `a_message_id_that_carries_a_term_is_replaced_by_the_hash` holds this (`mod.rs:205`).
* A link whose address carries a configured term loses the attribute (`redact.rs:514-524`).

Where the design does not meet the principle:

* An import runs with no term when the `[redaction]` section is there and holds no list. Each list has `#[serde(default)]` (`redact.rs:44-58`), and `Redactor::new` accepts five empty lists (`redact.rs:326-414`). The patterns for the shapes still run. `verify` then has no term to look for and passes every text. This is finding F1.
* A key with a spelling error is ignored without a message. `RedactionTerms` has no `deny_unknown_fields` (`redact.rs:41-42`), so `address_line` in place of `address_lines` gives an empty list for that kind. This is finding F1 too.
* A card ending in a form that the card pattern does not know is stored and not refused. `verify` looks for a card with the same pattern that the pass uses (`redact.rs:402-404`, `redact.rs:624-631`). This is finding F5.
* A refused mail leaves the other mails of the run stored. This is the design (ADR 0004, `docs/decisions/0004-redaction-before-storage.md:19`), and the failure status says that one was refused.

### Complete mediation

The table lists each value that `prepare` takes from the bytes of a mail and where the value goes.

| Value | Where it goes | What it passes | Evidence |
| --- | --- | --- | --- |
| Subject | `raw_messages.subject` | `redact_text` and the assertion | `mod.rs:85-88` |
| Subject, before redaction | The kind of the message, an enum | Nothing; no text leaves | `mod.rs:68` |
| First HTML part | `raw_messages.body_html` | `redact_html` and the assertion | `mod.rs:89-94` |
| First text part | `raw_messages.body_text` | `redact_text` and the assertion; no structural pass and no link pass | `mod.rs:95-100`, `redact.rs:540-576` |
| `Message-ID` | `raw_messages.message_key`, the import report, an error text, the name of a fixture file | `carries_configured_term` only; no pattern for a shape | `mod.rs:51-56`, `redact.rs:416-417` |
| All bytes | `raw_messages.source_sha256`, and the key when the header is no key | SHA-256 | `mod.rs:42-43` |
| From address | The platform, an enum | Nothing; the address is not stored | `mod.rs:58`, `import.rs:96` |
| `Date` | `raw_messages.received_at` | Nothing; a time | `mod.rs:64-67` |
| `Authentication-Results` | `dkim_pass`, a Boolean | Nothing; no text leaves | `mod.rs:70-74` |
| Other headers, other parts, attachments | Nowhere | Not read | `mime.rs:41-63` |

What the design does beyond the table:

* The receipt parser reads the stored HTML only (`postgres/mod.rs:264-266`). The order tables, the anomaly rows and the output of `qm due` therefore come from text that passed the redactor.
* `qm fixture add` writes the stored HTML only (`src/application/fixture.rs:26-28`, `src/adapters/fixtures.rs:39-69`).
* The program has five log calls. None holds a value from a mail (`gmail.rs:718`, and four calls of `takeout.rs` that name a path).
* The HTML library prints tokens of a mail before redaction at `debug` and `trace`. `log_line_is_safe` holds its targets at `warn` at most (`src/adapters/cli/mod.rs:211-219`). The test `no_setting_lets_a_library_print_mail_or_tokens` holds the rule (`cli/mod.rs:590`).
* A message key and an error text go through `terminal_text` before they reach the terminal (`cli/mod.rs:242-256`).

Where the design does not meet the principle:

* The link pass reads three attributes: `href`, `src` and `action` (`redact.rs:515`). A query string in another attribute stays, for example in `background`, `srcset`, `poster` or a `style` with a `url()`. A link that is the text of an element keeps its query string too. This is finding F3.
* The text body has no link pass at all. `redact_text` has no step that reads a link (`redact.rs:540-576`). Each link of a text part keeps its query string and its fragment. `docs/concepts/privacy.md:36` says that the redactor removes "the query string of every link". This is finding F3.
* A link keeps its path in every part (`redact.rs:505-506`). A token for the recipient that the sender writes into the path is stored. The parser needs the path of a product link, so this is a trade and not a slip. It is part of finding F3.
* A term in an encoded form passes outside the three attributes. Only `carries_configured_term` decodes percent pairs (`redact.rs:432`), and only the link pass and the key check call it (`redact.rs:517`, `mod.rs:55`). A mail address written with `%40` in place of `@` in a text body is not matched by the term, because the term needs `@`. The sequence pass does not match it, because `4` and `0` are a letter or digit between the characters of the term (`redact.rs:262`). The pattern for the shape of an address needs `@` (`redact.rs:149-153`). `verify` uses the same forms. By reading. This is finding F2.
* The key check looks for configured terms and for no shape (`redact.rs:416-417`). A `Message-ID` that holds an address or a number that is not configured is stored and printed. The code comment and `docs/concepts/privacy.md:38` say so. This is part of finding F4.
* A stored row is not redacted again when the redactor changes. A known key is skipped (`import.rs:98-101`). `qm fixture add` does not read `redaction_version` (`postgres/mod.rs:187-191`). This is finding F8.
* `qm fixture add --dir` takes any directory (`cli/mod.rs:166-168`, `fixtures.rs:21-29`). This is finding F9.
* The text of a panic is not examined here. The self-audit records that it is not encoded (`docs/self-audit.md:85`).

### Defense in depth

What the design does, as layers in the order in which a mail meets them:

1. The sender rule: only mail of a configured From address is stored (`mod.rs:58-63`).
2. The structural pass: the smallest block with an address, payment or contact heading is removed, with every image and every comment (`redact.rs:472-503`).
3. The link pass for three attributes (`redact.rs:507-526`).
4. The term passes, which run twice around the phone pass (`redact.rs:545-553`, `redact.rs:579-596`).
5. The patterns for the shape of a phone number, a mail address and a card ending (`redact.rs:561-573`).
6. The assertion `verify` (`redact.rs:601-633`). It reads the markup with the pattern of the pass, the text without tags with a second pattern, and the letters and digits of the text for a term of eight or more. The test `the_assertion_is_independent_of_the_passes` holds three cases (`redact.rs:936`).
7. The fuzz target `redact`, which asserts that a result holds no configured term as it is (`fuzz/fuzz_targets/redact.rs:15-28`).
8. For the repository, not for the store: real receipts stay out of git, `tests/fixtures/local/` and `backups/` are ignored, and the canary fails when a tracked file holds a shape or a term (`docs/concepts/privacy.md:87-90`).
9. For a Claude session: ten `Read` deny rules, and the guard hook as a best effort (`docs/concepts/privacy.md:44-59`).

Where the design meets the principle only in part:

* Layers 4, 5 and 6 share one list of terms and one normalization (`redact.rs:181-200`). What the list lacks, no layer finds. ADR 0004 records this for a missing term (`docs/decisions/0004-redaction-before-storage.md:32`). This is finding F4.
* For a phone and a card, layer 6 uses the pattern of layer 4 over a second view of the text (`redact.rs:616-631`). It adds no second form. `docs/concepts/privacy.md:36` says that the assertion "does not reuse the passes' patterns". That holds for the text pattern and the sequence check of a name, mail address or address line. It does not hold for a phone or a card. This is finding F5.
* Layer 5 has no assertion. A phone number or an address that is not configured is held by one pattern only.
* Layer 6 reads the text and the sequence without tags (`redact.rs:602-603`). Inside a tag only the markup check with the pattern of the pass applies. This is recorded (finding F7).
* No layer reads the store after the write. No command of `qm` compares the stored rows with the terms of today. The log shows that a query by hand over the stored rows found card digits three times (`docs/log.md:48-50`). This is part of finding F8.
* The canary reads tracked files. It does not read the database, a dump of it, or a file under `tests/fixtures/local/`.
* In a body, the digits of a configured phone followed by one more digit stay. This is recorded and kept (finding F6).

## Component 2: the Gmail adapter

The adapter is `src/adapters/mail/gmail.rs`.
`gmail_login` and the `Fetch` arm of `run` wire it (`src/main.rs:126-154`, `src/main.rs:192-236`).

### Least privilege

What the design does:

* The scope is `gmail.readonly` (`gmail.rs:33`). The test `the_authorization_address_asks_for_read_only_offline_access_with_pkce` holds the scope of the consent address (`gmail.rs:864`). The tool cannot send, change or delete mail with this token.
* The adapter makes three kinds of call: the token endpoint, the message list, and one message in raw form (`gmail.rs:399`, `gmail.rs:442`, `gmail.rs:710-729`).
* The search names the configured senders and a moment, and nothing else. A sender address is cut to the characters of an address (`gmail.rs:540-558`). The test `the_search_names_the_senders_and_the_moment_and_nothing_else` holds this (`gmail.rs:936`).
* The three addresses of the adapter are constants of `Endpoints::default` (`gmail.rs:146-154`). `main.rs` uses the default in both places (`main.rs:134`, `main.rs:218`). No setting changes a host.
* The token file is created with mode 0600 in one step, with `create_new`, and moved into place (`gmail.rs:515-535`). The test `the_token_file_is_private_and_round_trips` holds the mode (`gmail.rs:953`).
* The token is not in the database, so a dump of the database does not hold it (ADR 0013, `docs/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md:22`).
* The access token is in memory only (`gmail.rs:565`). The client secret, the code, the verifier and the two tokens are `SecretString` values (`gmail.rs:165`, `gmail.rs:270`, `gmail.rs:391`, `gmail.rs:435`).
* The tool opens no port. Nothing listens at the redirect address (`gmail.rs:34-36`).
* The container runs as a user that is not root, with a read-only file system, no capability and `no-new-privileges` (`compose.yaml:70-77`).
* The gzip feature of the HTTP library is off (`Cargo.toml:44`), so the adapter decodes no compressed answer.

Where the design meets the principle only in part:

* The token gives read access to all mail of the account. The tool needs the mail of a few senders. Google has no scope that returns a body for a part of a mailbox, so the scope cannot be narrower; the search is the only limit, and the tool applies it itself. ADR 0013 records this (`0013:27`). This is part of finding F11.
* The token is plain text in a file. A process that runs as the same user reads it. The mode keeps other users out, not other programs of the maintainer.
* The token does not end. The runbook says to set the project to "In production", because a token of a project in "Testing" ends after seven days (`docs/runbooks/gmail-fetch.md:25`). ADR 0013 records the choice (`0013:30`).
* The tool has no command that ends the access. The runbook gives two steps by hand (`gmail-fetch.md:52`).
* Compose gives every command the state directory with write access and the client secret in its environment, also `qm due` and `qm add` (`compose.yaml:53`, `compose.yaml:65`). Only `qm gmail login` and `qm fetch` need them. This is finding F16.
* The HTTP library takes a proxy from the environment by default (`ureq` 3.5.2, `src/config.rs:1073`, `src/proxy.rs:232-241`). `agent` does not switch that off (`gmail.rs:169-178`). This is finding F14.

### Fail-safe defaults

What the design does:

* Without both client values the two commands stop with `NoCredentials` (`main.rs:129`, `main.rs:193`). Without a token file `qm fetch` stops with `NoToken` (`gmail.rs:496-498`).
* An empty `QM_STATE` counts as not set, so the token file does not land in the working directory (`config.rs:311-319`).
* `qm gmail login` refuses to run when its input is no terminal (`main.rs:131-133`).
* The client refuses plain HTTP (`gmail.rs:171`, `main.rs:148`, `main.rs:219`). The last part of the test `a_refusal_and_a_bad_answer_arrive_as_one_error_without_the_token` holds this (`gmail.rs:1298-1309`).
* No redirect is followed (`gmail.rs:173`).
* Certificate verification is the default of the library, with the root certificates that are compiled in. No line of `src/` names the setting that turns it off, by a search.
* A call has 60 seconds and a connection 10 (`gmail.rs:42-43`). A token answer and a page of the list have 1 MiB, a message 64 MiB (`gmail.rs:45-48`). The body is read with its limit before the JSON is read (`gmail.rs:238-246`).
* One run reads at most `--max` messages, 500 by default (`gmail.rs:717-723`, `cli/mod.rs:70-72`). The channel holds 8 messages (`gmail.rs:40`).
* A page with an id that is not 1 to 64 ASCII letters and digits is refused whole, before a message of that page is read (`gmail.rs:613-637`). The test `a_page_of_the_list_is_read_and_an_id_that_is_no_id_is_refused` holds this (`gmail.rs:1088`).
* A token with a line break sends no request (`gmail.rs:1060`).
* The pasted address is accepted only with the scheme, host and port of the redirect and the state of this attempt, and with a code that is not empty (`gmail.rs:339-374`). The test `the_pasted_address_must_be_the_redirect_of_this_attempt` holds five refusals (`gmail.rs:900`).
* A failed write of the token leaves the old file (`gmail.rs:507-508`, `gmail.rs:534`).
* A panic of the reader task gives a fixed text (`gmail.rs:756-766`).
* A fetched mail takes the same `prepare` as a mail of an export, so every statement of component 1 applies (`main.rs:104-109`).

Where the design does not meet the principle:

* `TokenFile::load` reads a file of any mode and any owner, and follows a link (`gmail.rs:493-505`). A token file that a copy or a restore made readable for others is used without a message. The log records this (`docs/log.md:26`). This is part of finding F11.
* The tool does not create the state directory and does not check its mode. The runbook asks for mode 700 by hand (`gmail-fetch.md:36-40`). This is part of finding F11.
* The start of the search comes from a `Date` header, which the sender wrote. A stored mail with a date in the future sets the next search to seven days before now (`main.rs:196-210`, `postgres/mod.rs:213-220`). This is finding F10.
* An empty list of senders gives the search `from:()`. What Gmail returns for it is not known. The log records this (`docs/log.md:26`).
* The 64 MiB limit is for one answer. No limit holds for the size of a stored body or for the time that one mail takes in the redactor. This is finding F12.

### Complete mediation

What the design does:

* Each request goes through one agent with the limits above (`gmail.rs:169-178`). `GmailApi::get` is the one function that sends the access token, and it sends it to the constant API host (`gmail.rs:682-698`).
* Each answer goes through `read_body` (`gmail.rs:217-247`). An answer without a success status becomes a status and a short code. The code is cut to letters, digits, underscores and spaces, 48 at most (`gmail.rs:200-210`). The free text of Google is not kept.
* No variant of `GmailError` holds a token, a code, a message body or free text of an answer (`gmail.rs:52-130`). The test `the_sign_in_and_the_refresh_send_the_right_form_and_keep_secrets_out_of_errors` holds this for a revoked token (`gmail.rs:989`).
* The error of the HTTP library is reduced to a fixed text, because its own text can name an address (`gmail.rs:180-196`).
* A message id is checked before it becomes part of a path (`gmail.rs:622-630`). The page token goes into a query parameter, which the library encodes (`gmail.rs:706-709`).
* The sign-in uses PKCE with S256. The verifier is 32 random bytes of the operating system, and the state 16 (`gmail.rs:261-291`). The test `the_challenge_is_the_one_of_rfc_7636` holds the challenge (`gmail.rs:855`).
* A failed parse of the token file gives a fixed text and never a part of the file (`gmail.rs:501-503`).
* The HTTP library prints every byte at `trace`. `log_line_is_safe` holds its targets at `debug` at most, whatever `RUST_LOG` asks (`cli/mod.rs:211-219`, `main.rs:60-65`).
* The bytes of each fetched mail go to `PlatformMail::prepare` and nowhere else (`gmail.rs:724-733`, `import.rs:92-95`).

Where the design meets the principle only in part:

* The `error` parameter of a pasted address is read before the state is compared (`gmail.rs:360-365`). An address of another attempt with an `error` parameter therefore gives the text "Google did not give access" with a code of that address. The code is cut to 48 identifier characters, and no token follows from it.
* The path of the token file is part of an error text (`gmail.rs:66-76`). A path below a home directory names the account of the user on the terminal. Nothing stores it.
* `agent` takes `https_only` as an argument, and `Endpoints` has public fields (`gmail.rs:134-144`, `gmail.rs:169`). The two call sites of `main.rs` pass `true` and the default. No test reads the two call sites.
* The test of the log filter holds the predicate. No test runs the subscriber of `main.rs` with a `RUST_LOG` value. A reviewer ran six settings by hand on 2026-11-20 (`docs/log.md:26`). This is finding F13.

Not applicable:

* Authorization of each protected operation: the tool has no endpoint and no second user. ADR 0001 says that the application has no listener.
* Trust in a caller on the loopback address: nothing listens. The browser goes to `http://127.0.0.1:8765` and gets no answer from the tool. Another local program that listens on that port receives the code and the state. It cannot trade the code, because the verifier stays in the `qm` process (`gmail.rs:402`).
* A constant-time comparison of the state: the comparison is `!=` on two texts (`gmail.rs:365`). It runs once for each process, on one line that the user pastes, and each attempt has a new state (`gmail.rs:286-291`). No party can repeat the comparison and measure it, so the time of the comparison gives nothing away.

### Defense in depth

What the design does, as pairs of a first barrier and the barrier behind it:

| First barrier | Barrier behind it | Evidence |
| --- | --- | --- |
| Mode 0600 of the token file | The directory with mode 700 by the runbook; `gmail-token.json*` in `.gitignore`; the `Read` deny rule for the state directory in a Claude session | `gmail.rs:525-530`, `gmail-fetch.md:39`, `.gitignore:26`, `.claude/settings.json:34` |
| The state of the sign-in | PKCE: a code of another attempt has no verifier | `gmail.rs:365`, `gmail.rs:402` |
| No token in an error of this code | The log filter for the library that prints bytes | `gmail.rs:52-130`, `cli/mod.rs:211-219` |
| HTTPS only | No redirect; constant hosts; certificate verification | `gmail.rs:171-173`, `gmail.rs:146-154` |
| The read-only scope | Redaction with its assertion before a row is written | `gmail.rs:33`, `main.rs:104-109` |
| The size limit of an answer | The limit of a run and the bound of the channel | `gmail.rs:45-48`, `gmail.rs:40`, `gmail.rs:717` |
| The check of a message id | The path is appended to a constant host | `gmail.rs:622-630`, `gmail.rs:691` |
| The client secret outside the repository, in `.env` | `.env` in `.gitignore`; the `Read` deny rule; the secret scan of CI | `.gitignore:4`, `.claude/settings.json:27` |

Where the design meets the principle only in part:

* Behind the mode of the token file, no barrier is in the program. The directory mode is a step of the runbook, and the deny rule is for a Claude session only (`docs/concepts/privacy.md:78`). The token is not encrypted.
* The log filter names two libraries (`cli/mod.rs:212-216`). A new library that prints bytes needs a new line. The log records this (`docs/log.md:26`).
* Certificate verification is confirmed from the configuration, not from a connection. The log records this (`docs/log.md:26`). No target reaches the TLS path (ADR 0017, `docs/decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md:84`).
* The answers of the token endpoint have no fuzz target (`0017:83`).
* A sign-in and a fetch against Google were not run by the project's checks (`0013:44`).

## Findings

The findings are in the order of what they can let through, the largest first.
"Recorded" means that one of `docs/self-audit.md`, ADR 0017 or `docs/log.md` names the gap as open; ADR 0004 and ADR 0013 are cited where they name it too.
"New" means that this review did not find it there.

### F1. A `[redaction]` section with no term, or with a key that is spelled wrongly, runs the import (new)

* Who and what: the maintainer writes `[redaction]` and leaves it empty, or writes one key wrongly. No outside sender is needed. Every mail is then stored with the names or address lines of that kind in it. The assertion passes, because it has no term for that kind.
* Evidence: `src/adapters/config.rs:211-218` (the section is an `Option`, so only a missing section is an error); `src/adapters/mail/redact.rs:41-58` (each list has a default, and the type has no `deny_unknown_fields`); `redact.rs:326-414` (the constructor has no check for an empty set); `src/main.rs:105-106`. By reading; the test at `config.rs:350` shows that a section with one key of five is accepted.
* Record: new. The self-audit says only that the `Deserialize` derives of `config.rs` and `redact.rs` "were not examined for invariants or for `deny_unknown_fields`" (`docs/self-audit.md:81`). The doc comment at `redact.rs:38-40` states the empty list as the design.
* A fix must guarantee that a command that redacts runs only with the terms that the maintainer wrote, each under a key that the program knows, and that an empty kind is a statement of the maintainer and not a silence.

### F2. A term in percent-encoded form is stored outside the three link attributes (new)

* Who and what: a platform, or an outside sender with a forged From address, writes the mail address of the maintainer with `%40` in place of `@`, for example in a link of the text part. The address is stored in `body_text`. The same holds for a text node of the HTML and for an attribute other than `href`, `src` and `action`.
* Evidence: `redact.rs:432` and `redact.rs:517` (only the link pass and the key check decode); `redact.rs:149-153` (the shape needs `@`); `redact.rs:262` (the sequence pass admits no letter or digit between two characters of a term); `redact.rs:601-615` (the assertion uses the same forms). By reading; no test was run for it.
* Record: new for a body. The log records the related forms for a short text only: an encoding applied twice, a numeric entity, a full-width digit (`docs/log.md:8`, "Left open", point 1).
* A fix must guarantee that a term which a reader of the stored text can get back by decoding it once is replaced or refuses the message, in every part and every attribute.

### F3. A link keeps its query string in the text body and outside three attributes, and keeps its path everywhere (new)

* Who and what: a platform writes a token for the recipient into the links of the text part, or into the path of a link. The token is stored. A token can open a page of the sender that shows the mail before redaction, or an account setting. An outside sender can cause only what it already knows.
* Evidence: `redact.rs:505-526` (the link pass is in `redact_html` and reads `href`, `src` and `action`); `redact.rs:540-576` (`redact_text` has no link step); `mod.rs:95-100` (the text part is stored); `postgres/mod.rs:237-250` against a search for `body_text` (the column is written and never read). The test at `redact.rs:706` holds the query token of the HTML fixture only.
* Record: new. `docs/concepts/privacy.md:36` says "the query string of every link", which the text body does not meet.
* A fix must guarantee that no stored part holds a part of a link that the parser does not read, or that the part which is kept is recorded as kept with its reason.

### F4. The redactor knows the configured terms and three shapes; data in another form is stored (recorded, with forms that are new)

* Who and what: an outside sender with a forged From address writes data about the maintainer in a form that no term and no shape covers. A platform does the same with a term that the configuration lacks. The text is stored, and the assertion passes.
* Evidence: `redact.rs:601-633` (the assertion reads the configured terms); `redact.rs:181-190` (six invisible characters are removed; a direction mark, for example, is not); `redact.rs:347-352` (a term of fewer than eight letters and digits has no sequence pass and no sequence assertion); `redact.rs:216-221` (the sequence reads ASCII letters and digits only); `redact.rs:416-417` (the key check reads no shape).
* Record: recorded. ADR 0004 says "the assertion checks the configured terms only, so a missing term is not found" (`docs/decisions/0004-redaction-before-storage.md:32`). The log lists the short term without a sequence pass, the number glued to the front of the configured address, and the first name that hides a longer name (`docs/log.md:32`, "Noted and left"). New in this review, by reading: an invisible character outside the six inside a short term; a name in another Unicode composition than the configured one; an address or a number that is not configured inside a `Message-ID`.
* A fix must guarantee a stated list of the forms that the redactor covers, so that the rest is a known limit and not a surprise, and a second check that does not depend on the same list where one can exist.

### F5. A card ending after a mask in an unknown form is stored, and the assertion does not refuse it (recorded in part)

* Who and what: a platform prints the card as a mask, then a hyphen, then the four digits, or uses a mask character other than `x`, `X`, `*` and the bullet. The four digits are stored, also when they are a configured card.
* Evidence: `redact.rs:154-158` and `redact.rs:402-404` (after a mask the pattern admits spaces only; the hyphen is admitted after a brand, `redact.rs:140`); `redact.rs:624-631` (the assertion uses the pattern of the pass). By reading.
* Record: recorded for the family: "card digits after a brand are missed with an em dash, parentheses, `x`, dots or the word `ending` alone" (`docs/log.md:32`). New: the form with a mask and a hyphen, and the fact that for a phone and a card the assertion is the pattern of the pass, where `docs/concepts/privacy.md:36` says that it is not.
* A fix must guarantee that a configured card ending which stands in payment context in a form the pass does not know refuses the message.

### F6. In a body, the digits of the phone followed by one more digit are left alone (recorded; kept by decision)

* Who and what: a sender writes the configured phone number with a digit glued after it. The run stays in the stored body.
* Evidence: still true. `redact.rs:391` (the pattern needs a character that is no digit, or the end, after the number); the last assertion of `a_phone_number_inside_a_longer_run_of_digits_is_found_in_a_short_text` (`redact.rs:1029-1033`); `the_assertion_is_independent_of_the_passes` (`redact.rs:960-964`). Both tests passed in the run above. The key check and the link check do find such a run (`redact.rs:446-454`).
* Record: recorded in ADR 0004 (`0004:36`), in the log (`docs/log.md:8`, "Left open", point 2, and `docs/log.md:32`, "a digit glued after a phone number hides it") and as a fact in `docs/concepts/privacy.md:38`. ADR 0004 and the log entry of 2026-11-21 still call it an open question for the maintainer. The request for this review says that the maintainer decided on 2026-11-21 to keep it; this review did not find that decision in those files.
* A fix is not asked for. The record must say that the behaviour is a decision, with its date.

### F7. The assertion reads the text and the sequence without tags (recorded)

* Who and what: a term inside a tag, such as a name in an attribute, in a form that only the sequence pass removes. With that pass wrong, the assertion does not refuse the message.
* Evidence: still true. `redact.rs:602-603` (the view has no tags); `redact.rs:606` (the markup check uses the pattern of the pass, which does read inside a tag).
* Record: recorded in ADR 0017 (`0017:88`) and the log (`docs/log.md:8`, "Left open", point 3). The fuzz target `redact` holds the pass.
* A fix must guarantee that the assertion reads every character that is stored, in a form that does not depend on the pass.

### F8. A stored row keeps the redaction of the day it was stored, and nothing reads the store again (recorded in part)

* Who and what: a correction of the redactor does not reach a row that is stored. A term that an older version let through stays in the store and goes into a fixture file.
* Evidence: `import.rs:98-101` (a known key is skipped); `postgres/mod.rs:187-191` (`body_by_key` does not read `redaction_version`); `fixtures.rs:39-69` (the stored HTML is written as it is); the list of commands (`cli/mod.rs:47-143`) has none that checks stored rows.
* Record: recorded for the first sentence: "A message stored at an earlier version is not redacted again by a later import" (`docs/log.md:32`). New: `qm fixture add` writes a row of any version, and no command compares the store with the terms of today.
* A fix must guarantee that a row of an older redaction version is found and is redacted again or removed before it is read or written to a file.

### F9. `qm fixture add --dir` writes a redacted real receipt to any directory (new)

* Who and what: the maintainer names a directory that git tracks. A redacted real receipt is then in the working tree. Below `tests/fixtures/synthetic/input/` both modes of the canary skip the file (`scripts/pii_canary.sh:58`). No outside sender can cause this. Through Compose the container can write to `/fixtures` and `/state` only (`compose.yaml:65-71`), so the case needs a `qm` without a container.
* Evidence: `cli/mod.rs:166-168`; `fixtures.rs:21-29` (an absolute directory is taken as it is); `fixtures.rs:67` (the file is written with the default mode).
* Record: new. `docs/concepts/privacy.md:96` describes the default directory only.
* A fix must guarantee that the command writes only below a directory that git ignores, or that a file it writes elsewhere cannot pass the canary.

### F10. A stored mail with a date in the future moves the next search to seven days before now (new)

* Who and what: an outside sender forges the From address of a platform and writes a `Date` in the future. After the run that stores this mail, each run of `qm fetch` without `--since` searches from seven days before now. A receipt that arrived earlier and was not yet fetched is then never read, without a message. This needs more than seven days between two runs. A platform with a wrong clock causes the same.
* Evidence: `main.rs:196-210` (the newest stored time, at most now, less seven days); `postgres/mod.rs:213-220` (the newest `received_at` of all rows); `mod.rs:64-67` (`received_at` is the `Date` header). By reading.
* Record: new. The log records the first half as corrected: a time in the future "no longer moves the search past today" (`docs/log.md:26`).
* A fix must guarantee that the start of a search comes from a value that no sender of mail can set.

### F11. The token file is trusted as it is found, and the token is wide and lasting (recorded)

* Who and what: a local process that runs as the maintainer, or another user after a copy or a restore that changed the mode, reads the file. It then reads all mail of the account until the access is removed at Google.
* Evidence: `gmail.rs:493-505` (no check of mode or owner; a link is followed); `gmail-fetch.md:36-40` (the directory mode is a step by hand); `gmail.rs:33` (the scope); `gmail-fetch.md:25` (a token that does not end); no command ends the access (`cli/mod.rs:145-152`).
* Record: recorded. "`TokenFile::load` does not check the mode of a file that exists" and "a `QM_STATE` of spaces or a relative path is accepted" (`docs/log.md:26`); the reach and the life of the token in ADR 0013 (`0013:27`, `0013:30`). New detail: the tool does not check the mode of the state directory.
* A fix must guarantee that the tool uses a token only from a file and a directory that no other user can read, and says so when it finds another state.

### F12. No limit for the size of a stored body or for the time of one mail in the redactor (recorded)

* Who and what: an outside sender with a forged From address sends mails of the largest size that Gmail accepts. Each run of `qm fetch` reads them until they are stored or a week has passed, holds about ten of them in memory at one time, runs every pattern over each, and stores each body.
* Evidence: `gmail.rs:48` (64 MiB for an answer); `gmail.rs:40` (8 messages in the channel, one more in the reader and one in the use case); `mod.rs:85-100` (no limit before redaction); `migrations/0001_raw_mail.sql:14-15` (two text columns without a limit).
* Record: recorded. "a text that the database stores without an index ... has no limit of its own" (`docs/self-audit.md:80`); "no target asserts a limit on memory or time" (`docs/self-audit.md:82`, `0017:87`).
* A fix must guarantee a limit for the size of a mail before it is redacted, applied before the work that it bounds.

### F13. The log filter is a list of two libraries, and a test holds the rule and not the wiring (recorded in part)

* Who and what: a new library that prints bytes of a mail or a token at `debug` or `trace` reaches the terminal when the maintainer sets `RUST_LOG` to find a fault.
* Evidence: `cli/mod.rs:211-219`; `main.rs:54-65`; the test at `cli/mod.rs:590` calls the function and builds no subscriber.
* Record: recorded for the list: "the rule for the log names two libraries, so another HTTP library needs a new line" (`docs/log.md:26`). New: no committed test runs the subscriber; the six settings of 2026-11-20 were run by hand.
* A fix must guarantee that a library is silent below `warn` unless the rule names it, and that a test runs the subscriber as `main` builds it.

### F14. The HTTP client takes a proxy from the environment (new)

* Who and what: a process that sets `ALL_PROXY` or `HTTPS_PROXY` in the environment of `qm` sends the calls through a host of its choice. That host sees the names of the Google hosts, the sizes and the times. It does not see a token or a mail, because TLS runs from `qm` to Google inside the tunnel. Compose passes no such variable to the container.
* Evidence: `ureq` 3.5.2 `src/config.rs:1073` and `src/proxy.rs:232-241`; `gmail.rs:169-178` sets no proxy. By reading; no call was made through a proxy.
* Record: new.
* A fix must guarantee that the path of a call is set by the code or by a setting that the documents name.

### F15. The terms can be printed with `{:?}`, and one copy stays in the settings (new)

* Who and what: a later change that formats `RedactionTerms` for a log line or an error prints every term. No outside party can cause this today.
* Evidence: `redact.rs:41` (the derive); `config.rs:337-342` (the clone); `redact.rs:309-312` (the reason that says no copy stays).
* Record: new.
* A fix must guarantee that no formatting of a type that holds a term can print it.

### F16. Compose gives every command the state directory and the client secret (new)

* Who and what: a defect in a command that needs neither, such as the page of `qm due --html`, runs with write access to the directory of the token and with the secret in its environment.
* Evidence: `compose.yaml:53`, `compose.yaml:65`. The service has one definition for all commands.
* Record: new.
* A fix must guarantee that only the two commands of the Gmail path can reach the token and the secret.

### F17. No exception to a security rule is recorded (recorded)

* Evidence: still true. "The project records no exception to a security rule in this decision" (`docs/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md:39`). The self-audit says that the exception for a private tool does not apply (`docs/self-audit.md:76`).
* Consequence for this record: each finding above is a gap and not an exception. F6 is kept by a decision of the maintainer; that decision is about one behaviour of the redactor and is not an exception to a rule.

## Statements of the documents that this review does not confirm

* `docs/concepts/privacy.md:36`: "the query string of every link". True for `href`, `src` and `action` in the HTML body. Not true for the text body, a text node or another attribute (F3).
* `docs/concepts/privacy.md:36`: "The assertion does not reuse the passes' patterns". True for the text pattern and the sequence check of a name, mail address or address line. Not true for a phone or a card, and the markup check uses the pattern of the pass (F5, F7).
* `src/adapters/mail/redact.rs:309-312`: "no copy stays there after the redactor is built". True for the value that the caller hands over. `Settings` keeps its own copy (F15).
* ADR 0004 (`0004:36`) and the log entry of 2026-11-21 (`docs/log.md:8`) call F6 an open question. The decision to keep it is not in those files.
