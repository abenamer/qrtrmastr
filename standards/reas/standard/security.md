---
type: Engineering Standard
title: Security at seams
description: Trust boundaries, sizes from outside data, unsafe code, duplicate implementations, and output encoding.
tags: [security, rust, trust-boundary, unsafe, simd]
updated: 2026-11-20
---

# Security at seams

A seam is a place where two individually correct pieces of code can disagree about outside data.
Examples include a parser and a consumer that compute a length differently, a scalar path and a SIMD path, an input filter and an output renderer, and a verification step that a handler can skip.
The rules in this concept put one piece in charge at each seam and test that the other piece agrees.

Scale each rule by exposure.
Question 5 of [REAS-ARCH-003](ports-and-adapters.md#reas-arch-003) records who can supply outside data.
A private tool whose only source of outside data is its maintainer must still apply REAS-SEC-002, REAS-SEC-003, and REAS-SEC-004.
It can record an exception for the CI jobs in REAS-SEC-001, REAS-SEC-005, and [REAS-TEST-003](validation.md#reas-test-003) in an architecture decision.
An application that accepts anonymous or authenticated input from other parties must apply all rules.

<a id="reas-sec-001"></a>
## REAS-SEC-001: Test security properties

### Intent

Credentials, limits, authentication, and dependencies fail quietly.
Each needs a check that runs without a person remembering it.

### Enforcement

Required.

### Scope

Applies to every application with an endpoint, a credential, or a third-party dependency.
A private tool records an exception for the CI jobs as the exposure clause above describes.

### Rules

Keep credentials out of source code and logs.
Set limits on request size, connection count, and execution time.
Apply each limit before the allocation or the work that it bounds.

Document each endpoint as anonymous or authenticated.
Record the reason for anonymous access.
Enforce authorization for each protected operation.
Model each authentication and verification step as a state, as [REAS-STATE-002](state-machines.md#reas-state-002) specifies.
Document each trust assumption based on local access.
Do not trust a caller only because it uses a loopback address.
Before you implement a component that handles payments, personal data, or credentials, or that faces the internet, record a design review against least privilege, fail-safe defaults, complete mediation, and defense in depth, with a stated reason for each item marked not applicable.

Run a dependency vulnerability scan with `cargo deny check` in CI.
Run it on each pull request and on a schedule, because advisories arrive without a code change.
Restrict dependency sources to the registries you approve in the `sources` section of `deny.toml`.
A dependency with a build script or a proc-macro runs code on the build machine; review that code before you add the dependency, and record the review.
Prefer a version published more than a few days ago; flag a version published within days of the change for operator review.
List each dependency that reads outside data and contains `unsafe` code.
Review that unsafe code with `cargo vet` or a recorded manual review, or replace the dependency, before the first release that uses it.
`cargo geiger` counts unsafe items; it does not review them.
Document the project's own unsafe code as [REAS-SEC-004](#reas-sec-004) specifies.

### Verification

- CI: a `cargo deny check` job on pull requests and on a daily schedule.
- Review question: which new dependencies carry a build script or a proc-macro, and where is their review recorded?
- Review question: which dependencies touch outside data, and which of them contain `unsafe`?

<a id="reas-sec-002"></a>
## REAS-SEC-002: Parse outside data at the trust boundary

### Intent

Data that another party wrote is a claim, not a fact.
When the inbound adapter turns it into a validated type once, every function behind the adapter can trust its parameters.

### Enforcement

Required.

### Scope

Applies to every byte that enters the application from another party, including stored user data read back from the application's own database.
Does not apply to constants and configuration that the operator controls.

### Rules

Outside data is data that a party other than this application wrote.
This includes request bodies, uploads, webhooks, email, message-queue payloads, configuration files from users, and stored user data that the application reads back from its own database.
Stored user data does not become trusted because this application stored it.
The trust boundary is the inbound adapter.

Parse outside data into domain types in the inbound adapter.
Parse once, at the point of entry.
Give each port method and each domain function parameters of validated types only.
Do not pass `String`, `Vec<u8>`, `serde_json::Value`, or an unvalidated integer across a port when the value has an invariant.
Order the steps: decode the transport frame, verify the origin, then parse the payload.
Do not parse a payload before its origin is verified, when a verification step exists.
Inside the payload, check in this order and stop at the first failure: presence, then format, then range, then cross-field consistency, then uniqueness and references that need a query.

Make an invalid value impossible to construct:

- Keep the fields private.
- Validate in a constructor, in `TryFrom`, or in `FromStr`.
- For serde, use `#[serde(try_from = "String")]`, or `try_from` with the raw type, so that deserialization uses the same validation.
- Do not derive `Deserialize` directly on a type with an invariant.
- Use `#[serde(deny_unknown_fields)]` when the schema is closed.

All safe public construction paths must preserve the invariant, as [REAS-RUST-001](core-rust.md#reas-rust-001) specifies.

Report a validation failure at two levels.
The caller receives what failed and what a valid value looks like, with no internal detail.
The log receives the rule, the rejected value after [REAS-SEC-006](#reas-sec-006) encoding, and the request context.
For an authentication or authorization failure, the caller receives a generic message and a reference identifier; the log receives the detail.

### Verification

- Review question: for each port method, which parameter carries outside data, and which constructor validated it?
- Test: one rejection test for each invariant through the deserializer, not only through the constructor.
- Lint: `clippy::unwrap_used` set to `deny` in inbound adapters, so that a parse failure becomes a typed error.

<a id="reas-sec-003"></a>
## REAS-SEC-003: Compute sizes from outside data once, with checked arithmetic

### Intent

A length from outside data is a number, not a fact about the buffer.
Two pieces of code that compute the same offset can disagree, and the one that is wrong writes past the end.

### Enforcement

Required.

### Scope

Applies to every length, count, offset, or index that comes from outside data, and to every regular expression applied to outside data.
Does not apply to sizes the compiler knows, such as array lengths.

### Rules

Convert a length from outside data to `usize` with `usize::try_from`.
Compare it with the limit from REAS-SEC-001 before any allocation or loop that it bounds.
Compute each derived offset with `checked_add`, `checked_mul`, or `checked_sub`.
Return a typed error when the computation fails or the offset is outside the buffer.
Compute each offset once.
Pass the result to the code that uses it.
Do not compute it again in that code.
Use `get(start..end)` for a range from outside data.
Do not use `[start..end]` for that range.

Set `overflow-checks = true` in the release profile of each crate that handles outside data.
Then an unchecked overflow panics instead of wrapping silently.
Declare the lint settings under `[workspace.lints]`, as [REAS-RUST-006](core-rust.md#reas-rust-006) specifies, so that they are visible with the profile.

Apply only a linear-time regular expression engine to outside data.
The `regex` crate is linear-time; a backtracking engine such as `fancy-regex` can be driven to exponential time by a crafted input.
Set `size_limit` on each compiled pattern that outside data reaches.

### Verification

- Lint: `clippy::arithmetic_side_effects` and `clippy::indexing_slicing` set to `deny` for the crate or module that reads outside data.
- Review question: is the same length computed in two places?
- Test: a declared length larger than the buffer, equal to the limit, one more than the limit, zero, and the maximum value of the field type.
- Fuzz: the target from [REAS-TEST-003](validation.md#reas-test-003).

<a id="reas-sec-004"></a>
## REAS-SEC-004: Confine unsafe code

### Intent

An `unsafe` block is a promise the compiler cannot check.
One small module with a safe API keeps every such promise in one place a reviewer can read.

### Enforcement

Required.

### Scope

Applies to every crate.
Does not apply to generated bindings whose generator is recorded and reviewed.

### Rules

Set `unsafe_code = "forbid"` under `[lints.rust]` in each crate.
When an operation needs `unsafe`, change that setting to `deny` for the crate and allow it in one module.
Keep that module small.
Give it a safe public API that checks each precondition before the unsafe operation.
Keep the unsafe operation private to the module.

Before you write unsafe code, use a reviewed crate with a safe API for the same operation.
For byte-level reinterpretation, use `zerocopy` or `bytemuck`.
For SIMD, use autovectorization first, then a crate with a safe SIMD API.
Use `std::arch` intrinsics only when a measurement shows the need.
Record the measurement.

Add a `SAFETY` comment immediately before each `unsafe` block.
State the conditions and why they hold at that point.
Do one unsafe operation in each `unsafe` block.
For each public unsafe function, document the caller's obligations in a `# Safety` section.
For an unsafe abstraction, test the boundary conditions.
Run Miri on the module's tests in CI.
Where Miri cannot run, for example across FFI, run AddressSanitizer.
Record the Miri and sanitizer results, including checks that did not run.

### Verification

- Lint: the rustc `unsafe_code` lint; `clippy::undocumented_unsafe_blocks` and `clippy::multiple_unsafe_ops_per_block` set to `deny`.
- CI: `cargo miri test` for the crate that contains the module.
- Review question: which line of the safe API checks each precondition, and which `SAFETY` comment depends on it?

<a id="reas-sec-005"></a>
## REAS-SEC-005: Verify duplicate implementations against the reference

### Intent

A fast path is a second implementation of the same function, and the two can disagree on an input neither author tried.
The reference implementation is the specification; the differential test is the proof.

### Enforcement

Required.

### Scope

Applies to every fast path, cache, SIMD version, or hand-unrolled loop that duplicates a simpler implementation.
Does not apply to a single implementation with no duplicate.

### Rules

Keep the simple implementation as the reference.
Keep it compiled and tested.
Write a differential test that calls both implementations with the same generated inputs and compares the results.
Include the empty input, lengths below, at, and above the block width, and inputs with all bytes `0x00` and `0xFF`.
Assert the same equality in the fuzz target from [REAS-TEST-003](validation.md#reas-test-003).

For a block-wise or SIMD loop, prove the bound before each full-width access.
The remaining length must be at least the block width.
Put that check in the loop condition or in a `chunks_exact` or `as_chunks` call.
Do not put it after the access.
Process the scalar tail with the reference implementation.
Do not process it with a masked or partial block.
For a cache, test that a hit and a miss return the same value.
Test that invalidation after a change to the source returns the new value.

### Verification

- Test: a differential test in the unit test suite.
- CI: run the differential test on each CPU architecture and target-feature set that the code deploys to.
- Review question: which function is the reference, and where is each bound checked?

<a id="reas-sec-006"></a>
## REAS-SEC-006: Encode output for its context

### Intent

Each output context treats different characters as syntax.
Only the code that writes to a context knows which encoding it needs, so encoding belongs there and nowhere earlier.

### Enforcement

Required.

### Scope

Applies to every value the application writes into HTML, a URL, JSON, SQL, a shell command, a log line, or an HTTP header.
Does not apply to a value that never leaves the process.

### Rules

Validate on input.
Encode on output.
Each output context has its own encoding: HTML text, HTML attribute, URL component, JSON string, SQL identifier, shell argument, log line, and HTTP header.
Apply the encoding in the adapter that writes to that context.
Use the library for that context, for example template autoescaping, percent-encoding, or a JSON serializer.
Use parameterized queries for SQL data values.
Quote SQL identifiers with the driver's identifier quoting, because parameters do not cover identifiers.
Remove control characters and line breaks from a value before it goes to a log line or an HTTP header.

Do not sanitize on input.
A sanitizer on input does not know the output context.
It destroys valid data, or it misses the characters that the real context treats as syntax.
Store the validated value as the user entered it.
Treat a stored value as outside data when the application reads it back, as REAS-SEC-002 specifies.

### Verification

- Review question: list each output sink and the encoder that writes to it.
- Test: one test for each sink with a value that contains that sink's special characters.
- Review question: does an inbound adapter remove or replace characters for a context it does not know?

The [trust-boundary example](../examples/trust-boundary/) demonstrates REAS-SEC-002, REAS-SEC-003, REAS-SEC-004, and REAS-SEC-005 in one inbound adapter.
It does not demonstrate output encoding or a dependency scan.

## Quality test

Where does each byte of outside data become a validated type, and which single check does each seam depend on?
