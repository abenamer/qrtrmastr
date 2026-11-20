---
type: Engineering Standard
title: Persistence and transactions
description: PostgreSQL storage, transactions, idempotent consumers, and reproducible builds.
tags: [postgresql, sqlx, transactions, idempotency]
updated: 2026-11-20
---

# Persistence and transactions

<a id="reas-data-001"></a>
## REAS-DATA-001: Use explicit storage operations

### Intent

A reader must find the SQL that runs and the index that serves it.
A row type that carries business defaults turns a wire-format change into a business-rule change.

### Enforcement

Required.

### Scope

Applies to every query, migration, row type, and index in a crate that uses a database.
Does not apply to an in-memory adapter.

### Rules

Write a specific SQL query for each operation.
Keep SQL in named concrete adapters.
Version each schema change as a migration.
When a SQLx row type has different invariants, map it to the domain type through validation.
Keep serde and SQLx attributes such as `#[serde(default)]`, `#[serde(rename)]`, and `#[sqlx(rename)]` on the row or wire type in the adapter, never on the domain type.
A default on a domain type makes a missing field a business value.
Store money as [REAS-RUST-008](core-rust.md#reas-rust-008) specifies.
Use database constraints for invariants that PostgreSQL can express as constraints.
Use parameterized queries for data values.

Select indexes from representative query plans and workload measurements.
Record the query that each index serves.
Include storage and write cost in that decision.
Do not add an index only because a query filters a column.

### Verification

- Review question: which struct carries each serde or SQLx attribute, and is that struct in an adapter?
- Record: each index names the query it serves in the migration or a comment.
- Test: a database integration test for each query, as [REAS-TEST-001](validation.md#reas-test-001) specifies.

<a id="reas-data-002"></a>
## REAS-DATA-002: Set transaction boundaries

### Intent

Writes that must succeed or fail together need one transaction, and the code that owns the transaction must be visible.
A transaction cannot cover an HTTP call, so work that crosses that line needs a recorded policy.

### Enforcement

Required.

### Scope

Applies to every operation that writes more than one row or more than one table, and to every operation that mixes a database write with an external side effect.
Does not apply to a single-row write.

### Rules

Put related database writes that must succeed or fail together in one transaction.
Declare the atomic operation in an application port.
Let its concrete adapter own the transaction, including commit and rollback.
For writes across repositories, use one operation-level port with an adapter that shares the transaction.
Do not expose SQLx transaction types in domain or application code.

Guard workflow transitions as [REAS-STATE-003](state-machines.md#reas-state-003) specifies.
Do not include HTTP calls or other external side effects in the guarantee of a local database transaction.
For such work, record the retry, idempotency, and partial-failure policy, and make the consumer idempotent as [REAS-DATA-005](#reas-data-005) specifies.
Do not build a generic repository framework before a second concrete implementation exists.

### Verification

- Review question: which adapter method calls commit, and which port method declares the operation it commits?
- Test: a test that fails the second write and asserts the first write was rolled back.

<a id="reas-data-003"></a>
## REAS-DATA-003: Keep data beyond the container lifecycle

### Intent

A container is disposable.
Data that lives only inside one disappears with it.

### Enforcement

Required.

### Scope

Applies to every PostgreSQL instance that holds data the project must keep, in development and in production.
Does not apply to a throwaway database created by a test run.

### Rules

Store PostgreSQL data in a named Docker volume.
Keep backups in storage that is independent of the database volume.
Do a test of the restore procedure.
Make sure that a container rebuild or replacement preserves the records.
Do not use `docker compose down -v` against a database that you must keep.

The `-v` flag removes named volumes declared by the Compose project.
It also removes anonymous volumes attached to the project's containers.
It does not remove external volumes.
A volume is not a backup.

### Verification

- Procedure: a recorded restore test with its date and the backup it used.
- Review question: which Compose file declares the named volume, and which documented procedure recreates the container without it?

<a id="reas-data-004"></a>
## REAS-DATA-004: Make builds reproducible

### Intent

A build that depends on a developer's machine, an unpinned toolchain, or a floating dependency set produces a different binary each time.

### Enforcement

Required.

### Scope

Applies to every crate that is built in CI or released.
Does not apply to a scratch crate that is never shared.

### Rules

For SQLx checked queries, commit the metadata that `cargo sqlx prepare` writes to `.sqlx/`.
Set `SQLX_OFFLINE=true` in builds that must not connect to a database.
Do not make compilation depend on a developer's local database.
Offline compilation does not establish that the metadata matches the current migrations.

In database integration CI, apply migrations to a new empty database.
Then run `cargo sqlx prepare --check` for the supported targets and features.
Keep the SQLx CLI version compatible with the project's SQLx version.

Commit `Cargo.lock` for application workspaces.
Use `--locked` for release builds.
Record the Rust toolchain, target, enabled features, and native dependencies.
Record the lockfile policy for library examples.
Declare the minimum supported Rust version with `rust-version` in each published crate's manifest, and check it in CI with `cargo hack check --rust-version`.
For a crate with Cargo features, check each combination in CI with `cargo hack check --feature-powerset --no-dev-deps`.

### Verification

- CI: `--locked` on every build, `cargo sqlx prepare --check` where SQLx is used, `cargo hack check --rust-version` where `rust-version` is declared, and the feature powerset where features exist.
- Review question: which file records the toolchain, and does the lockfile change in this pull request match the manifest change?

<a id="reas-data-005"></a>
## REAS-DATA-005: Make event consumers idempotent

### Intent

Delivery between a producer and its consumers is at-least-once in practice: retries, restarts, and replays deliver the same event again.
A consumer that repeats its effect on a duplicate eventually double-commits, double-notifies, or double-counts.

### Enforcement

Required.

### Scope

Applies to every consumer of events, messages, or webhooks, and to every projection fed by live delivery.
Does not apply to a pure function that returns a value and performs no effect.

### Rules

Give each event a stable, unique identifier that the writer assigns when it appends the event.
The identifier does not change across copies or schema upgrades.
Record what each consumer has handled: the set of handled identifiers, or the highest contiguous position processed.
Store that record with the consumer's own state, in the same transaction as the effect where possible.
Check the record before you act, and update it after the effect succeeds.
Where the effect and the record cannot share a transaction, make the effect itself idempotent: an upsert keyed by the event identifier, a compare-and-swap on the expected previous value, or a notification sent with an idempotency key derived from the identifier.
Do not deduplicate on payload equality; two genuine events can carry the same payload.
Do not record an event as handled before its effect succeeds; a crash between the two loses the effect.
Do not rely on exactly-once delivery from the transport.

### Verification

- Test: for each consumer, deliver the same event twice, and out of order with its neighbours where order is not guaranteed, and assert that the effect happened once.
- Review question: which identifier keys the effect, and which transaction holds both the effect and the handled record?

## Quality test

Can two concurrent writers both believe they own the same transition, and can a redelivered event repeat its effect?
