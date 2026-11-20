---
type: Technical Reference
title: Rule identifiers
description: Identifier stability and historical mappings for REAS rules.
tags: [rules, compatibility, history]
updated: 2026-11-20
---

# Rule identifiers

A rule identifier names one requirement.
Keep that identifier when its text changes without a change to its purpose.
Do not reassign a published identifier to another requirement.
For a retired rule, keep a record of the identifier and any replacement.
Add an explicit lowercase HTML anchor for each active rule identifier.

## Historical conflict

Version 1.3.0-draft changed several identifiers from version 1.2.0-draft.
It also reassigned `REAS-TEST-002` from async work to evidence reporting.
An unversioned historical reference to that identifier is ambiguous.
This table records the conflict; it cannot make the old references unambiguous.

| Requirement | Identifier in 1.2.0-draft | Identifier from 1.3.0-draft |
| --- | --- | --- |
| Async and CPU work | `REAS-TEST-002` | [REAS-RUST-007](core-rust.md#reas-rust-007) |
| Security properties | `REAS-TEST-003` | [REAS-SEC-001](validation.md#reas-sec-001) |
| Evidence reporting | `REAS-TEST-004` | [REAS-TEST-002](validation.md#reas-test-002) |

## Split in 1.5.0-draft

Version 1.5.0-draft moved the `REAS-SEC` family from `validation.md` to [`security.md`](security.md).
The identifiers did not change.
`validation.md` keeps an anchor for `REAS-SEC-001` that points to the new location.

REAS-SEC-001 kept its identifier and its purpose.
Its requirements for the project's own `unsafe` code moved to [REAS-SEC-004](security.md#reas-sec-004).
A reference to REAS-SEC-001 for a `SAFETY` comment or a Miri result from before 1.5.0-draft now means REAS-SEC-004.

The current identifiers remain in use to avoid another reassignment.
Resolve an old reference against its cited version or commit.
If that context is absent, report the ambiguity instead of selecting a meaning silently.

The documentation checker detects duplicate declarations in the current files.
It does not detect reassignment across commits.
Compare the identifier history during a rule review.
