---
type: Concept
title: Application overview
description: What Quartermaster does, how its layers fit together, and the code path of a command.
tags: [architecture, overview]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-21
sources:
  - id: lib
    resource: ../../src/lib.rs
    title: Crate root and layer map
  - id: main
    resource: ../../src/main.rs
    title: Composition root
  - id: cli
    resource: ../../src/adapters/cli/mod.rs
    title: The command-line adapter, with the encoding for the terminal
---

# Application overview

## Purpose

Quartermaster turns FreshDirect receipt emails into a PostgreSQL order history and recommends the next cart. It is a personal tool for one household. It reads a one-time Google Takeout export of the receipts; it does not talk to FreshDirect, and it places no orders.

The MVP does these things:

1. Imports the mail export, redacts personal data, and stores each message once.
2. Parses each receipt into an order with its lines, and records what it could not classify.
3. Resolves each line to a product through an alias, so renamed products stay one product.
4. Recommends items whose usual gap between purchases has elapsed, or, with `--simple`, items bought in most orders and missing from the last.

Recipes, restaurant receipts, probabilistic forecasting, weather, and the MCP server are not in the MVP. The plan that holds them is outside this repository.

## Layers

The code has three layers and one composition root. Dependencies point inward (REAS-ARCH-002).

| Layer | Path | Holds | Must not hold |
| --- | --- | --- | --- |
| Domain | `src/domain` | Identifier newtypes, money and quantities, stored labels and the `ParseState` lifecycle, and the rules that act on them. | I/O, the clock, SQLx, file or network types. |
| Application | `src/application` | Use cases, and the ports (traits) they need from the outside. | Any adapter type. |
| Adapters | `src/adapters` | `PostgresStore`, the settings reader, the command line; later the mbox reader and the receipt parsers. | Business rules. |
| Composition root | `src/main.rs` | Builds the adapters and the use cases and runs one command. | Anything else. |

A use case takes its adapter as a type parameter bound by the port (REAS-ARCH-006): `Migrate<S: SchemaStore>`. The concrete adapter is visible in `main.rs` and nowhere else.

## The code path of a command

`qm migrate` follows the REAS-ARCH-001 order:

1. The command-line adapter (`adapters::cli`) parses the arguments into `Command::Migrate`.
2. The composition root reads the settings, connects `PostgresStore`, and builds `Migrate`.
3. The use case calls the `SchemaStore::migrate` port.
4. `PostgresStore` applies the embedded migrations and reports which were new.
5. The composition root prints the report. A failure prints the error and exits with status 1 (REAS-RUST-005).

Every later command (`import`, `due`, `cart`) follows the same five steps with its own use case and ports.

## Where a text leaves the application

A text from a receipt is outside data, also after the application stored it. The application validates it when it comes in and encodes it when it goes out, in the adapter that writes to the output context (REAS-SEC-006). Each context has its own encoding. No encoding changes the stored text, and no context uses the encoding of another.

| Output context | Outside data that reaches it | Encoding, and where | Test |
| --- | --- | --- | --- |
| The terminal: the tables of `qm due`, the report of `qm import` and `qm fetch`, the line and the question of `qm add`, the error line, the lines of `qm migrate`, the path of `qm fixture add` | The name and the size of a product from a receipt; a message key, which is the Message-ID of a mail and is read before the sender check; the text of an addition; a stored value that an error quotes; a path | `terminal_text` in `src/adapters/cli/mod.rs` writes as `\u{..}` each control character, each direction mark, the two Unicode line separators and each tag character. Nothing is removed. An error is one line. The exception is a configuration error, whose text is the program's own and holds configuration only: `terminal_lines` keeps its line breaks and encodes each line. | `a_text_for_the_terminal_keeps_its_letters_and_shows_its_controls_as_text`, `a_text_of_several_lines_keeps_its_line_breaks_and_nothing_else`, `one_name_from_a_receipt_is_encoded_for_each_sink_and_stays_whole_in_the_report`, `the_import_report_shows_a_control_character_of_a_message_key_as_text`, `the_line_of_an_added_item_shows_a_direction_mark_as_text`, `the_reason_prompt_shows_a_control_character_of_the_item_as_text`, and in `src/main.rs` `a_configuration_error_keeps_its_lines_and_any_other_error_is_one_line` |
| A log line | The path of an export file or an archive. The other fields are counts and constant texts; a message body, a term and a database URL never go to a log. | The path is written in its debug form, in `src/adapters/mail/takeout.rs`: a line break or a control character of a file name is an escape, so one event is one line. The log library does not do that for a field written with `%`. A filter holds the lines of the HTML parser and the HTTP library ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)). | `a_file_name_with_control_characters_is_one_log_line_without_them`, `an_archive_name_with_control_characters_adds_no_log_line_either`, `no_setting_lets_a_library_print_mail_or_tokens` |
| JSON, `qm due --json` | Every text of a report | The wire types in `src/adapters/cli/wire.rs` say which fields the JSON holds; the domain and the application layer have no serialization. `json_text` in `src/adapters/cli/mod.rs`: `serde_json` writes each string, and a pass over its output writes as `\uXXXX` each other character that a terminal obeys or hides, because standard output is often a terminal. The JSON value is the same. | `a_report_as_json_has_no_character_that_a_terminal_obeys_and_is_the_same_value`, and the test of the three sinks |
| HTML, `qm due --html` | The names, the sizes, the additions | `escape` in `src/adapters/cli/page.rs`, for element text and for attribute values. The page takes the name as the receipt printed it, without the encoding of the terminal, so "Copy ticked" copies the name itself. The page loads nothing ([ADR 0012](/decisions/0012-the-next-order-as-a-page-that-is-a-file.md)). | `the_page_holds_every_list_escapes_its_texts_and_loads_nothing`, and the test of the three sinks |
| SQL | Every stored value | Each value is a parameter of a statement in `src/adapters/postgres/`. No table name or column name comes from data. | `a_text_with_the_characters_of_sql_is_stored_and_read_back_as_that_text` in `tests/database.rs` |
| A URL: the Gmail search, the sign-in address | The sender addresses and the client identifier, which are configuration; the moment of the newest stored mail | The `url` library and the HTTP library encode each parameter. `search_query` keeps only the characters of an address, so that a configured sender cannot add an operator to the search. | `the_search_names_the_senders_and_the_moment_and_nothing_else`, `the_authorization_address_asks_for_read_only_offline_access_with_pkce` |
| An HTTP header | The access token, which is an answer of Google's token endpoint | The HTTP library refuses a value with a line break, and sends nothing. | `a_token_with_a_line_break_sends_no_request` |
| A file name, `qm fixture add` | The message key | `write_fixture` in `src/adapters/fixtures.rs` keeps ASCII letters, digits, `.` and `-`, and writes `_` for each other character. | `the_file_lands_under_platform_and_year_with_a_safe_name` |

No value from outside data goes into a shell command: `scripts/due_page.sh` passes only its own constants and the output path that the user gave.

What the encoding does not do:

* A panic is not encoded. The program sets no panic hook, so the text of a panic goes to the terminal as it is, and a panic on a cut of a text quotes a piece of that text. No panic that carries outside data is known. Since step 3 of [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) the code of the project has no cut by position outside its tests, and Clippy refuses a new one (`string_slice`, `indexing_slicing`); a dependency can still panic with a text of its own.
* The encoding for the terminal counts characters, not columns. A name of wide characters can make a row wider than the table, and a narrow terminal then wraps it.
* An argument that you type is shown by the argument parser as you typed it.
* `qm due --html` writes a page, with the encoding of HTML. When you run it without a redirect to a file, the page goes to the terminal, and a control character of a name goes with it. `scripts/due_page.sh` writes the page to a file.
* The redactor changes a mail before storage ([ADR 0004](/decisions/0004-redaction-before-storage.md)). That is a privacy control and not an encoding; the redacted text is still encoded at each context above.

## Where it runs

On the maintainer's machine, as two containers: PostgreSQL with a named volume, and the `qm` command, run with `docker compose run --rm qm ...` ([Develop and run](/runbooks/develop.md)). Nothing of the project is installed on the host. Nothing runs in the cloud except GitHub Actions, which runs the checks.
