---
type: Engineering Standard
title: Ports and adapters
description: Application boundaries and visible adapter implementations.
tags: [architecture, rust, hexagonal]
updated: 2026-11-20
---

# Ports and adapters

<a id="reas-arch-001"></a>
## REAS-ARCH-001: Make the code path visible

### Intent

A reader who cannot follow an operation from its entry point to its effect cannot review it, test it, or replace a part of it.

### Enforcement

Required.

### Scope

Applies to public or human-maintained applications.
Does not apply to a private personal tool that records an exception under [REAS-ARCH-003](#reas-arch-003).

### Rules

Use ports and adapters.
For operations with external I/O, make this code path easy to follow:

1. Inbound adapter.
2. Application use case.
3. Port trait.
4. Outbound adapter.

Construct the adapters and use cases in one composition root for each executable.
A use case can delegate directly to a port when it establishes the application boundary.
Do not add a forwarding layer without a different responsibility.

### Verification

- Review question: for one operation, name the file of each of the four steps and the composition root that connects them.
- Review question: which layer adds a responsibility that the layer below does not have?

<a id="reas-arch-002"></a>
## REAS-ARCH-002: Direct dependencies toward the application

### Intent

Domain code that imports a framework changes when the framework changes.
Domain code that depends only on ports it declares changes only when the business rules change.

### Enforcement

Required.

### Scope

Applies to the dependency graph between domain, application, and infrastructure code in every crate.
Does not apply to a flat module layout chosen under [REAS-ARCH-003](#reas-arch-003).

### Rules

Keep frameworks, database drivers, transports, and deployment tools out of domain code.
Examples include Axum, SQLx, HTTP, and Docker.
Declare the necessary ports in application code.
Use outbound adapters to implement those ports.
Use inbound adapters to convert external input for application use cases.
The inbound adapter is the trust boundary, as [REAS-SEC-002](security.md#reas-sec-002) specifies.

### Verification

- CI: for a workspace with a domain crate, `cargo tree -p <domain> -e normal` contains no framework, driver, or transport crate; the job asserts that list.
- Manifest: the domain crate's `Cargo.toml` is the gate; a framework dependency added there is the review finding.
- Review question: which module imports a framework type, and is it an adapter?

<a id="reas-arch-003"></a>
## REAS-ARCH-003: Select the architecture for private projects

### Intent

Ports and adapters cost files and indirection.
A private tool with one maintainer and no replaceable dependency pays that cost for no reader.

### Enforcement

Required.

### Scope

Applies before the architecture of a private project is selected, and whenever its answers change.
Does not apply to public or human-maintained applications, which use REAS-ARCH-001 without exception.

### Rules

Before you select the architecture, ask these questions:

1. Who will read and maintain the code?
2. What external systems does it use?
3. For how long must the project remain in use?
4. How likely are the boundaries to change?
5. Who can supply outside data to the application?

The answer to question 5 sets the exposure for the [security rules](security.md).
Use a flat module layout for a private personal tool with no replaceable external dependency.
Keep domain behavior separate from I/O even in that layout.
For other exceptions to REAS-ARCH-001, record the answers and the selected boundaries in an architecture decision.

### Verification

- Record: the five answers appear in an architecture decision or in the project README.
- Review question: has an answer changed since the architecture was selected?

<a id="reas-arch-004"></a>
## REAS-ARCH-004: Keep concrete implementations visible

### Intent

A reader finds the code that runs by following names, not by resolving a container at run time.
A port shaped like its storage is an adapter in disguise and leaks the storage format into the application.

### Enforcement

Required.

### Scope

Applies to every port trait and every adapter.
Does not apply to a test double that exists only in test code.

### Rules

Declare a port trait where a test double or a replaceable implementation is necessary.
Use a port to isolate a system outside your control.
Shape each port method in the application's terms: domain types in, domain types or a typed error out.
A port method that mirrors the storage API, such as `append_line(&str)` or `execute_sql(&str)`, is an adapter in disguise; move the translation into the adapter.
Do not add factories that build factories.
Do not use a service locator or hidden dependency registration.
Do not add duplicate interfaces or forwarding layers without a different responsibility.
Name each adapter for the system that it adapts, for example `PostgresTeamRepository`.

### Verification

- Review question: for each port method, which parameter or return type belongs to the storage or transport rather than to the domain?
- Review question: can a reader find the adapter that runs in production by reading the composition root alone?

<a id="reas-arch-005"></a>
## REAS-ARCH-005: Separate rules from infrastructure

### Intent

A business rule that performs I/O cannot be tested without that I/O, and a test that needs several doubles is evidence that the rule reaches infrastructure directly.

### Enforcement

Advisory.

### Scope

Applies to business rules and mathematical operations in application and domain code.
Does not apply to adapters, whose purpose is I/O.

### Rules

Write business rules and mathematical operations as pure functions.
Supply time, random values, and external data as inputs to those functions.
If a side effect is necessary for a rule, record the reason and its boundary.
Put HTTP serialization, SQLx row mapping, and third-party API clients in adapters.
Pass dependencies through constructors.

### Verification

- Test: a unit test of a business rule needs plain values and no test double; a test that needs several doubles names the rule that reaches infrastructure directly.
- Review question: which function takes the clock, the random source, or the external data as an argument, and which one reads it directly?

<a id="reas-arch-006"></a>
## REAS-ARCH-006: Use generic adapter parameters by default

### Intent

A generic parameter shows the concrete adapter in the use case's type at the composition root.
A trait object hides it until run time, which is the right trade only when the implementation must vary at run time.

### Enforcement

Advisory.

### Scope

Applies to the way a use case receives its ports.
Does not apply to a port that is selected at run time by configuration or by request.

### Rules

Give a use case an adapter type parameter with a port trait bound.
For example, use `FindTeam<R: TeamRepository>`.
Here, `TeamRepository` is the port and `R` is the adapter type.
The use case's type shows the concrete adapter at the composition root.

Use a trait object for an implementation that must vary without a change to the enclosing type.
Also consider a trait object when adapter parameters appear in unrelated public APIs.
Record that decision in the composition root.
Use `Box<dyn Port>` for one owner.
Use `Arc<dyn Port + Send + Sync>` for shared ownership across threads.
`Arc` alone does not make the adapter thread-safe.

Make sure that the port is dyn-compatible before you use a trait object.
For a statically dispatched async port, use native `async fn` or an explicit future return type.
Record the `Send` requirement for returned futures.
For a dynamically dispatched async port, use an explicit boxed future with the necessary lifetime and thread bounds.
If a macro supplies this representation, record its name.

### Verification

- Review question: for each `dyn Port`, which implementation varies at run time, and where is that recorded?
- Compiler: a dyn-incompatible port used as a trait object does not compile.

The [ports and adapters example](../examples/ports-and-adapters/) uses a synchronous port.
It includes a CLI inbound adapter, one use case, and an in-memory outbound adapter.
It does not demonstrate an async port or a database transaction.

## Quality test

Can a reader trace one operation from its inbound adapter to its outbound adapter through the composition root alone, and does each port method take and return the application's types rather than the storage's?
