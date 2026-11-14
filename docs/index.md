---
okf_version: "0.2"
---
# Quartermaster — knowledge bundle

This bundle describes the application, its rules, and its operation. The text targets ASD-STE100 Simplified Technical English. Each concept is one file with YAML front matter. Links between concepts are bundle-relative.

# Standard

This project follows the [REAS engineering standard](https://github.com/abenamer/reas), vendored at `standards/reas/`.

* [Self-audit](self-audit.md) - Evidence and limits of REAS compliance.
* [Project terminology](terminology.md) - Project-specific technical terms.
* [STE review](ste-review.md) - Status and procedure of the Simplified Technical English review.

# Concepts

* [Application overview](concepts/application.md) - What the application does, its layers, and the code path of a command.
* [Privacy](concepts/privacy.md) - What personal data the mail carries, what is stored, and the two canaries that guard it.

# Reference

* [Database schema](reference/schema.md) - Every table, its purpose, and the migration that made it.
* [Command line](reference/cli.md) - The `qm` commands and their exit status.
* [Configuration](reference/configuration.md) - Every variable the application reads.

# Runbooks

* [Develop](runbooks/develop.md) - Toolchain, database, checks, and the pull-request flow.

# Decisions

* [ADR 0001](decisions/0001-ports-and-adapters-for-a-private-tool.md) - Ports and adapters for a private tool with replaceable external systems.
* [ADR 0002](decisions/0002-pull-requests-and-the-security-review-on-every-commit.md) - Pull requests into main and the security review on every commit.

# Log

* [Update log](log.md) - Dated record of every change to this bundle and the application.
