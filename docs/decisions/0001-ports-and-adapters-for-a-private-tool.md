---
type: Architecture Decision
title: Ports and adapters for a private tool with replaceable external systems
description: Why a single-user tool uses the REAS ports-and-adapters layout rather than the flat layout REAS-ARCH-003 permits, with the four questions the rule asks and their answers.
tags: [decision, architecture, reas]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
---

# ADR 0001 — Ports and adapters for a private tool with replaceable external systems

## Context

REAS-ARCH-003 permits a flat module layout for a private personal tool with no replaceable external dependency, and asks four questions before the architecture is selected. Quartermaster is private and single-user, so the exception is on the table.

The four questions and their answers:

1. Who will read and maintain the code? The maintainer alone, with an AI assistant, over years. The maintainer comes from Symfony, where a Domain / Application / Infrastructure split is the normal shape.
2. What external systems does it use? A mail export today (a Takeout mbox), the Gmail API later; PostgreSQL; later, recipe pages and a weather source. Each of these is outside the project's control.
3. For how long must the project remain in use? Indefinitely; it accumulates a household's history.
4. How likely are the boundaries to change? Likely. The mail source is planned to change from a file to an API without changing anything downstream, and the deferred design adds more sources.

## Decision

* Use ports and adapters (REAS-ARCH-001). `src/domain` is pure; `src/application` declares the ports and the use cases; `src/adapters` holds the concrete implementations named for what they adapt; `src/main.rs` is the single composition root.
* Declare a port only at a genuine boundary (REAS-ARCH-004): the mail source, the stores, and later the recipe fetcher and weather source. The receipt parsers are pure format adapters, not ports, because they have no I/O and need no test double beyond a fixture.
* Use cases take their adapters as type parameters bound by the port (REAS-ARCH-006). No trait objects until an implementation must vary at run time; that decision would be recorded here.
* Keep the layout in one Cargo package with one binary. A second binary (the MCP server of the deferred design) would be a second composition root in `src/bin/`.

## Consequences

* Swapping the Takeout reader for a Gmail API adapter touches `src/adapters` and `main.rs` only.
* Every use case can be unit-tested with a failing double, which REAS-TEST-001 asks for.
* Two small files (`ports.rs`, `migrate.rs`) exist for a command that could be one function. That is the cost of the shape, paid once.

## Alternatives

* A flat layout with modules by feature, as REAS-ARCH-003 permits. Rejected because the mail source is a replaceable external system by design, and the project's lifetime is long.
* A Cargo workspace with one crate per layer. Rejected as more ceremony than the code size warrants; the module boundary and the review rules enforce the same separation.
