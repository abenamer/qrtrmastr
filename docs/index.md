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
* [Files and their authority class](reference/files.md) - Each committed file with its class: authored, derived, evidence or companion.
* [Unsafe and native code in the dependencies](reference/native-code.md) - The count of unsafe items for each package, the dependencies that outside data reaches, and what Miri cannot run.
* [Build scripts and procedural macros of the dependencies](reference/build-scripts.md) - What each dependency runs on the build machine, as read on 2026-11-21.
* [Design review of the redactor and the Gmail adapter](reference/design-review.md) - The two components that handle personal data and credentials, against four principles, with the findings that are open.

# Runbooks

* [Develop and run](runbooks/develop.md) - Running qm through Docker Compose, the toolchain, the database options, the checks, and the pull-request flow.
* [Export your FreshDirect mail and import it](runbooks/takeout-export.md) - The Takeout export, the redaction terms, and the import.
* [Fetch new receipts from Gmail](runbooks/gmail-fetch.md) - The Google Cloud project, the sign-in, and qm fetch.
* [Back up the database and restore it](runbooks/backup-and-restore.md) - The backup, the restore drill, where to keep a backup, and the restore after a loss.

# Decisions

* [ADR 0001](decisions/0001-ports-and-adapters-for-a-private-tool.md) - Ports and adapters for a private tool with replaceable external systems.
* [ADR 0002](decisions/0002-pull-requests-and-the-security-review-on-every-commit.md) - Pull requests into main and the security review on every commit.
* [ADR 0003](decisions/0003-one-time-takeout-import-with-the-gmail-api-deferred.md) - One-time Takeout import, with the Gmail API deferred.
* [ADR 0004](decisions/0004-redaction-before-storage.md) - Redaction before storage.
* [ADR 0005](decisions/0005-product-identity-through-aliases.md) - Product identity through aliases.
* [ADR 0006](decisions/0006-recommendation-by-usual-gap-with-a-simple-rule-beside-it.md) - Recommendation by usual gap, with a simple rule beside it.
* [ADR 0007](decisions/0007-restaurant-mail-is-stored-at-import-and-parsed-later.md) - Restaurant mail is stored at import and parsed later.
* [ADR 0008](decisions/0008-receipts-without-a-dkim-pass-are-parsed-and-tagged.md) - Receipts without a DKIM pass are parsed and tagged.
* [ADR 0009](decisions/0009-qm-due-ranks-by-recent-habit.md) - qm due ranks by recent habit.
* [ADR 0010](decisions/0010-sizes-of-one-product-count-as-one-and-a-bulk-purchase-is-a-stock.md) - Sizes of one product count as one, and a bulk purchase is a stock.
* [ADR 0011](decisions/0011-essentials-extras-reminders-and-additions-with-a-reason.md) - Essentials, extras, reminders, and additions with a reason.
* [ADR 0012](decisions/0012-the-next-order-as-a-page-that-is-a-file.md) - The next order as a page that is a file.
* [ADR 0013](decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md) - New receipts are fetched from the Gmail API.
* [ADR 0014](decisions/0014-dates-and-times-with-jiff-not-chrono.md) - Dates and times with jiff, not chrono.
* [ADR 0015](decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md) - REAS 1.7.0-draft, and the order in which its gaps close.
* [ADR 0016](decisions/0016-the-documentation-checker-follows-the-checker-of-reas.md) - The documentation checker follows the checker of REAS and reads Markdown with a parser.
* [ADR 0017](decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md) - A fuzz target for each reader of outside bytes, with a pinned nightly toolchain and a budget for each stage.
* [ADR 0018](decisions/0018-records-for-the-dependencies-the-two-sensitive-components-and-the-backup.md) - A baseline for the unsafe code of the dependencies, the records of the build scripts and of the design review, and a backup with a restore drill.

# Log

* [Update log](log.md) - Dated record of every change to this bundle and the application.
