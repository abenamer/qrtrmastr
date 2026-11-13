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
* [Import](concepts/import.md) - How a mail export becomes stored messages and parsed orders, and what each message state means.
* [Product identity](concepts/identity.md) - How a printed name becomes a product, why aliases sit between lines and products, and how a mistake is corrected.
* [Recommendation](concepts/recommendation.md) - The two rules behind qm due, their thresholds, and what they cannot know.

# Reference

* [Database schema](reference/schema.md) - Every table, its purpose, and the migration that made it.
* [Command line](reference/cli.md) - The `qm` commands and their exit status.
* [Configuration](reference/configuration.md) - Every variable the application reads.

# Runbooks

* [Develop](runbooks/develop.md) - Toolchain, database, checks, and the pull-request flow.
* [Export your FreshDirect mail and import it](runbooks/takeout-export.md) - The Takeout export, the redaction terms, and the import.

# Decisions

* [ADR 0001](decisions/0001-ports-and-adapters-for-a-private-tool.md) - Ports and adapters for a private tool with replaceable external systems.
* [ADR 0002](decisions/0002-pull-requests-and-the-security-review-on-every-commit.md) - Pull requests into main and the security review on every commit.
* [ADR 0003](decisions/0003-one-time-takeout-import-with-the-gmail-api-deferred.md) - One-time Takeout import, with the Gmail API deferred.
* [ADR 0004](decisions/0004-redaction-before-storage.md) - Redaction before storage.
* [ADR 0005](decisions/0005-product-identity-through-aliases.md) - Product identity through aliases.
* [ADR 0006](decisions/0006-recommendation-by-usual-gap-with-a-simple-rule-beside-it.md) - Recommendation by usual gap, with a simple rule beside it.

# Log

* [Update log](log.md) - Dated record of every change to this bundle and the application.
