---
type: Architecture Decision
title: A baseline for unsafe code, records for the dependencies and the two sensitive components, and a restore drill
description: How the project counts the unsafe code of its dependencies and holds the count, where the reviews of step 9 are recorded and what they did not read, and how a backup is taken and proved.
tags: [decision, security, dependencies, native-code, backup, tooling]
generated: { by: "process:claude-code", at: "2026-11-22T02:45:00Z" }
updated: 2026-11-21
sources:
  - id: baseline
    resource: ../../unsafe-baseline.tsv
    title: The count of unsafe items for each package
  - id: check
    resource: ../../scripts/check.sh
    title: The unsafe group of the check script
  - id: drill
    resource: ../../scripts/restore_drill.sh
    title: The backup and the restore drill
  - id: reas-native-002
    resource: ../../standards/reas/standard/native-code.md
    title: REAS-NATIVE-002, bound and track the unsafe surface
  - id: reas-sec-001
    resource: ../../standards/reas/standard/security.md
    title: REAS-SEC-001, test security properties
  - id: reas-data-003
    resource: ../../standards/reas/standard/persistence.md
    title: REAS-DATA-003, keep data beyond the container lifecycle
---

# ADR 0018: A baseline for unsafe code, records for the dependencies and the two sensitive components, and a restore drill

## Context

Step 9 of [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) is the step of the records. Before it, four things that REAS asks for did not exist:

* A count of the unsafe code of each package, in a committed baseline that CI compares (REAS-NATIVE-002).
* A recorded review of the code that a dependency runs on the build machine, and a list of the dependencies that outside data reaches and that hold unsafe code (REAS-SEC-001, REAS-NATIVE-001).
* A design review of the two components that handle personal data and credentials: the redactor and the Gmail adapter (REAS-SEC-001).
* A backup outside the Docker volume, and a restore that was tested (REAS-DATA-003).

The maintainer said on 2026-11-21 to prepare all of the step and to run the restore of the real database last, together, when Docker runs.

## Decision

The unsafe count:

* The system shall hold the count of unsafe items for each package that the counter names in `unsafe-baseline.tsv`, written by `scripts/check.sh unsafe write` from the result of `cargo geiger` 0.13.0.
* When the count of a build is not the baseline in any line, `scripts/check.sh unsafe` shall fail and name each package that differs. The check asks for the same lines, not for counts that are not larger, so that the file is the record of the current count.
* When a count is larger, a package with unsafe items is new, or a package no longer forbids unsafe code, the change shall give the reason in [the log](/log.md), and its review shall read it, before the baseline is written again.
* When a change moves `Cargo.lock`, the same change shall write the baseline again. A pull request of Dependabot that moves a package of the baseline therefore fails the `rust` job; the bump is made again on a branch under point 5 of the commit protocol, with the new baseline.
* The group shall run as a step of the `rust` job, which branch protection requires. A job of its own is not required until the maintainer changes a setting of the repository.
* If the counter is another version than the one that the script names, then the group shall stop with a message and count nothing.

The records:

* [Unsafe and native code in the dependencies](/reference/native-code.md) lists the dependencies that outside data reaches and that hold unsafe code, and marks what Miri cannot run in each. `scripts/outside_data_paths.py` writes its table. When the baseline changes, the table is written again.
* [Build scripts and procedural macros of the dependencies](/reference/build-scripts.md) records what each of the 45 packages that are compiled on Linux does on the build machine. When a dependency with a build script or a procedural macro is added, or one of them moves to a new version, the change shall read the script or its changes and add to the page.
* [Design review of the redactor and the Gmail adapter](/reference/design-review.md) is the review against least privilege, fail-safe defaults, complete mediation and defense in depth. It came after the code, and it says so. When a component that handles personal data or credentials is added, its review is written before the code.
* A finding of a review is a gap until a change corrects it. This decision corrects none and records no exception.

The backup:

* `scripts/restore_drill.sh backup` shall write one file of `pg_dump` in the custom format to `backups/`, a directory of the host that is not in the Docker volume and that git ignores.
* `scripts/restore_drill.sh` shall take a backup, restore it into a second PostgreSQL container of the same image that has no volume and no network, compare the number and a hash of the rows of each table with the database, and remove that container.
* The script shall write nothing to the database of the Compose project, stop no container of it, and touch no volume.
* When the major version of PostgreSQL changes, or a migration changes a table, the maintainer runs the drill again and puts its record into the log.

## Consequences

* One new tool for a developer who changes a dependency, and for CI: `cargo-geiger` 0.13.0. It is not needed to build, test or run `qm`. `scripts/check.sh all` runs the group when the tool is there and names it as not run when it is not.
* The `rust` job takes longer: the counter builds the program once more, in a build directory of its own, `target/unsafe-count`, because it removes the directory that it works in.
* CI installs the counter with `taiki-e/install-action`, which has no manifest for it and takes a ready-made program through `cargo-binstall`, without the checksum that it has for a tool of its list. The counter publishes no such program, so it is a build of the `cargo-quickinstall` project. `sqlx-cli` and `cargo-fuzz` come the same way; the log entry of this decision records it as open for all three. The job holds a token that can read the repository, the password of the test database, and no secret of the repository.
* A count is not a review. The baseline says that the surface did not move; it does not say that the surface is sound. No unsafe code of a dependency was reviewed.
* The design review found 17 gaps, 8 of them not recorded before. The first three are about personal data that can be stored: a `[redaction]` section that is empty or has a key spelled wrongly still runs the import; a term in percent-encoded form is replaced only in three link attributes; a link keeps its query string in the text body. Each needs a change of its own, and the second and third need a new redaction version ([ADR 0004](/decisions/0004-redaction-before-storage.md)).
* Two statements said more than the code and were corrected with this decision, in [Privacy](/concepts/privacy.md), in the comments of the redactor and in [ADR 0004](/decisions/0004-redaction-before-storage.md): "the query string of every link", and that the assertion is independent of the passes.
* The build review found no download, no connection and no fetched program in a build script. It found that each program that a build starts is found through the environment.
* REAS-DATA-003 stays "not met" until the maintainer has run the drill on the Compose database and the log holds its record. The script ran against the development database with a stand-in for Docker on 2026-11-21; that run proves the comparison, not the commands of Docker.

## Alternatives

* A check that fails only when a count grows. Rejected: after a count went down, the file would name a number that no build has, and the next growth up to that number would pass without a reason.
* The counter in a job of its own. Rejected for now: such a job is not a required check until the maintainer adds it, and the fuzz job is in that state today.
* `cargo vet` for the review of the unsafe code of the dependencies. Not decided here: it is a new tool and a new store of audits, and the maintainer decides it with the other gaps that remain after step 10.
* The restore into a second database of the same server. Rejected: it does not show that the backup restores on a server that never saw the volume.
* A backup of the Docker volume as files. Rejected: a copy of the files of a running server is not a consistent database, and it restores only on the same version.
* A backup on a schedule. Not decided here: the tool has one user, and a schedule on the host is the maintainer's.

## What the records do not cover

* The count is for Linux on x86_64 and for the program. The fuzz package and a build for another platform are not counted. The counter did not scan 232 files that the build read, and 148 of them hold unsafe code that the count does not have ([Unsafe and native code in the dependencies](/reference/native-code.md)).
* The list of the dependencies on a path of outside data follows the dependency graph. It is larger than the set that a mail reaches, and no check compares the page with the baseline.
* The build review did not read the 27 packages of the lockfile that are not compiled on Linux, read the 18 macro crates by a search with patterns, and did not read the C and assembly sources of `ring` or its 17 object files for Windows.
* One assistant wrote the design review in one reading. It did not read the mail library, the HTML library, the TLS library or the Takeout reader.
* The drill compares the rows of the tables of the schema `public`. It does not compare the view, the indexes, the constraints or the roles of the server. No test of the drill script runs in CI, and the script has not run against Docker.
* Nothing holds the step of the counter in the `rust` job: the `lists` group passes when the step is in a job that is not required. The review of a pull request is the control.
