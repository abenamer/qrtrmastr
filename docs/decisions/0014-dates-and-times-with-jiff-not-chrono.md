---
type: Architecture Decision
title: Dates and times with jiff, not chrono
description: Why the project left chrono after two unmaintained advisories stopped the deny job, what replaced it at each place a date is read, computed or stored, and what was checked.
tags: [decision, dependencies, supply-chain]
generated: { by: "process:claude-code", at: "2026-11-20T19:00:00Z" }
updated: 2026-11-20
---

# ADR 0014 — Dates and times with jiff, not chrono

## Context

On 2026-11-20 the `deny` job failed on every pull request, and `cargo deny check` failed on `main` itself. The advisory database now carries RUSTSEC-2026-0543, `chrono` is unmaintained, and RUSTSEC-2026-0555, `chrono-tz` is unmaintained. `deny.toml` fails the check for an unmaintained crate that this package depends on directly. Neither advisory reports a vulnerability, and neither names a patched version.

`chrono-tz` was in `Cargo.toml` and no source file used it. `chrono` held every date and time of the project: the delivery date of an order, the day `qm due` judges from, the time a message was received, the date a receipt prints, and the row types of the PostgreSQL adapter through the `chrono` feature of SQLx.

The maintainer was offered four ways and chose to leave `chrono`.

## Decision

* `chrono` and `chrono-tz` leave `Cargo.toml`, and SQLx is built without its `chrono` feature. `Cargo.lock` no longer lists either crate.
* The date and time library is `jiff` 1.0.1. A delivery date and a day to judge from are `jiff::civil::Date`. The time a message was received is `jiff::Timestamp`.
* The PostgreSQL adapter uses the wrapper types of `jiff-sqlx` 1.0.0 at the wire only: `jiff_sqlx::Date` and `jiff_sqlx::Timestamp` in the row types, `to_sqlx()` on a bind, `to_jiff()` on a row. No wrapper type leaves `src/adapters/postgres`.
* "Today" is `jiff::ZonedDateTime::now().date()` in `src/main.rs`. It reads the `TZ` variable and the zone database of the system, as before.
* The number of days between two dates is one function, `days_between` in `src/domain/recommend.rs`.
* The receipt parser reads a date with `Date::strptime`. In `jiff`, `%A` and `%B` read the full name of a weekday and of a month only; `chrono` also read the three-letter name. Each format is therefore tried with the full and the short name of each, so the parser reads the texts it read before.

## Consequences

* The `deny` job passes again without an entry in the `ignore` list of `deny.toml`.
* A `Timestamp` prints as `2026-03-03T11:15:00Z`, where `chrono` printed `+00:00`. Two tests changed their expected text. The database stores the same instant.
* The text of the error for a bad `--as-of` value is the text of `jiff`.
* A year is an `i16` and a month and a day are an `i8` in `jiff`. A date of a receipt is inside that range.
* A `Timestamp` ends at 9999-12-30T22:00:00Z. A mail whose `Date` header is in the last two days of year 9999 is reported as malformed, with the reason `no Date header`.
* When `TZ` is not set and the system has no zone database, `jiff` takes UTC for "today" and reports no error. The container image carries `tzdata`.
* `Cargo.lock` lists crates of the `jiff` family that the Linux build does not compile (`jiff-static`, `jiff-tzdb`, `jiff-tzdb-platform`, `portable-atomic`). An advisory on one of them fails the `deny` job all the same.
* SQLx has no `jiff` feature of its own. The wrapper crate is one more dependency, from the author of `jiff`, and it must follow the major version of SQLx.

## Alternatives

* An entry for each advisory in the `ignore` list of `deny.toml`, with the reason: rejected by the maintainer. It keeps a crate that nobody maintains under every date of the project.
* Remove only `chrono-tz`, which was unused: it closes one advisory of the two.
* The `time` crate, which SQLx supports directly: not tried. `jiff` has a civil date type that fits a delivery date, and it reads the zone for "today" from `TZ` and the system database without a feature flag.

## Runs

2026-11-20, on the `migrate-off-chrono` branch.

| Check | Result |
| --- | --- |
| `cargo deny check` | advisories, bans, licenses and sources pass; before the change it failed at advisories |
| `cargo tree -i chrono` | no such package in the graph |
| `cargo test --lib` | 74 passed, with a new test that reads one date from 17 texts and refuses six bad texts |
| The first 12 of those texts and the six bad texts against the `chrono` code of `main`, in a separate worktree | passed: both libraries read and refuse them alike, a weekday that does not match the date among them |
| The security review compared both libraries on a short weekday with a full month and the reverse | `chrono` read five such texts that `jiff` refused; the parser now tries each pairing, and the five texts are in the test |
| `cargo test --test database` | 6 passed against PostgreSQL 15: a date and a timestamp go to the database and come back through the wrapper types |
| `qm due` with `TZ=Pacific/Kiritimati` and with `TZ=Pacific/Pago_Pago` at 18:34 UTC | the day after and the same day, as the zones require |
| `qm due --json` | a date is `"2026-03-27"`, as before |
