---
type: Technical Reference
title: Command line
description: The qm commands, their arguments, and their exit status.
tags: [cli, reference]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
sources:
  - id: cli
    resource: ../../src/adapters/cli/mod.rs
    title: Command definitions
  - id: main
    resource: ../../src/main.rs
    title: Dispatch
---

# Command line

`qm` reads its settings from the environment and an optional `.env` file ([Configuration](/reference/configuration.md)). Every command exits with status 0 on success and 1 on failure, with the error on standard error (REAS-RUST-005). Logs go to standard error at the level `RUST_LOG` sets.

| Command | What it does | Output |
| --- | --- | --- |
| `qm migrate` | Applies the pending migrations. A second run applies nothing. | One line per applied migration, then `N applied now, M already applied`. |

Commands of later milestones, in the order they arrive: `qm import --mbox <path>`, `qm review`, `qm reparse --stale`, `qm alias suggest|merge|split`, `qm product rename|mute`, `qm due`, `qm cart`, `qm fixture add`. Each is documented here when it exists.
