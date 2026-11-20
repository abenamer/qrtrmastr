---
type: Architecture Decision
title: Ports and adapters for a private tool with replaceable external systems
description: Why a single-user tool uses the REAS ports-and-adapters layout rather than the flat layout REAS-ARCH-003 permits, with the five questions the rule asks and their answers.
tags: [decision, architecture, reas]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-20
---

# ADR 0001 — Ports and adapters for a private tool with replaceable external systems

## Context

REAS-ARCH-003 permits a flat module layout for a private personal tool with no replaceable external dependency, and asks questions before the architecture is selected: four in REAS 1.4.0-draft, against which this decision was made, and a fifth since 1.5.0-draft. Quartermaster is private and single-user, so the exception is on the table.

The questions and their answers:

1. Who will read and maintain the code? The maintainer alone, with an AI assistant, over years. The maintainer comes from Symfony, where a Domain / Application / Infrastructure split is the normal shape.
2. What external systems does it use? A mail export today (a Takeout mbox), the Gmail API later; PostgreSQL; later, recipe pages and a weather source. Each of these is outside the project's control.
3. For how long must the project remain in use? Indefinitely; it accumulates a household's history.
4. How likely are the boundaries to change? Likely. The mail source is planned to change from a file to an API without changing anything downstream, and the deferred design adds more sources.
5. Who can supply outside data to the application? Added on 2026-11-20, for REAS 1.7.0-draft. Three parties:
   * Any party that can send mail to the maintainer's mailbox. A Takeout export and the Gmail mailbox hold what other parties wrote. The import keeps a message only when its From mailbox is an address the project lists for a platform, and it records whether the mail has a DKIM pass. A From address can be forged, so a forged message reaches the mail reader, the classifier and the redactor, and since [ADR 0008](/decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md) the receipt parser. In practice the bytes come from FreshDirect and the restaurant platforms, and the ordinary bad data is a receipt that the platform's own software built wrongly.
   * Google, through the Gmail API over TLS: the JSON of each answer.
   * The maintainer: the configuration file, the command line, and the text of `qm add`.

   The application has no listener and no endpoint. The answer to this question sets the exposure for the REAS security rules: other parties can supply outside data, so the exception that REAS gives a private tool whose only source of outside data is its maintainer does not apply. [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) records what follows from that.

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
