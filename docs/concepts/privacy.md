---
type: Concept
title: Privacy
description: What personal data the mail carries, what the store keeps, what a Claude session may not read, and the two canaries that guard the repository.
tags: [privacy, security, redaction]
generated: { by: "process:claude-code", at: "2026-11-13T18:00:00Z" }
updated: 2026-11-27
sources:
  - id: canary
    resource: ../../scripts/pii_canary.sh
    title: The privacy canary
  - id: guard
    resource: ../../.claude/hooks/guard-bash.sh
    title: The guard hook
  - id: settings
    resource: ../../.claude/settings.json
    title: The permission rules of a Claude session in the repository
  - id: local-example
    resource: ../../.claude/settings.local.example.json
    title: An example of rules for directories in another place
  - id: schema
    resource: ../../migrations/0001_raw_mail.sql
    title: raw_messages
---

# Privacy

## What the mail carries

Every FreshDirect mail carries the maintainer's name, email address, delivery address, phone number, and the last four digits of the payment card. None of these is stored. The repository may become public, so the rule is stricter than "not in the database": nothing that came from a real receipt is committed, even redacted.

## What the store keeps

Since 2026-11-24 the store keeps no subject and no body of a mail ([ADR 0021](/decisions/0021-store-the-extracted-items-and-not-the-mail.md)): a row of `raw_messages` holds the key of a mail, its platform, kind and date, its DKIM result, a hash of its bytes and the state of its parse. The import reads the page of a receipt, hands it to the parser in memory, and stores the order and its lines: the name of each product as the receipt prints it, its department, quantities and prices, and the path of its link. Of a row that the parser could not read it stores the place and the reason, and no cell; such a row can be the line of the payment card. The `Message-ID` of a mail is its key and is stored as the sender wrote it.

A row of before that day holds a redacted subject and redacted bodies, which a redactor made ([ADR 0004](/decisions/0004-redaction-before-storage.md)). The program has no redactor now. `qm erase-stored-mail --yes` sets them to nothing and keeps the orders; run it one time, after one more `qm import` or `qm fetch`, which parses each receipt that no parser has read yet from its stored page. Two short texts of older rows stay: the error of a parse attempt and the reason of an anomaly can quote one cell of a receipt, a date or the digits of an order number. Before it, take a backup if you want the old rows once more, and know that the backup then holds the redacted mail: delete it when you no longer need it. After it, `vacuum full raw_messages, order_line_anomalies;` in a `psql` session writes the two tables again without the old text and gives the space back to the system; it locks the tables while it runs and needs free disk for the copy. Without it the old text stays in the files of the database until the database reuses the space. A backup that was made before the command still holds the text. The [design review](/reference/design-review.md) of 2026-11-21 lists what that redactor could miss.

What the store keeps of a receipt is text that the shop wrote about products. Limits: the name of a product, its department and the path of its link are stored as the receipt has them, and so is the `Message-ID` of the mail. The program is a parser of receipts and no filter against a sender that puts personal data into one of those on purpose.

The Takeout export lives under `~/qm-data/`, outside the repository. A Claude session in the repository is denied that place and the configuration directory, as the next section says, and every shell command outside a short allow list asks you first. The guard hook is a best-effort backstop behind those two controls: it refuses the common forms of a command that names an export file outside the import tooling. A pattern match over command text is not a security boundary, and the documents do not treat it as one.

## What a Claude session may not read

The store keeps no mail. The places where the mail and the access to it are need their own protection: the mail export, the configuration directory, the Gmail token. `.claude/settings.json` is committed, so each clone of the repository has it. For a Claude session that starts at the top of the repository, its `deny` list refuses a read of those places. Claude Code takes the settings from the directory where the session starts, so a session that starts in a subdirectory does not have these rules. No rule holds the name of a person or of an account: a rule that starts with `~/` is read from the home directory of whoever runs the session.

| Rule | What it keeps from a session |
| --- | --- |
| `Read(./.env)` | The database password and the secret of the Gmail client. |
| `Read(./backups/**)` | A dump of the database, which is treated like the database. |
| `Read(./tests/fixtures/local/**)` | Redacted real receipts, which name what one household buys. |
| `Read(/*.html)` | A page of `qm due --html` at the top of the repository: your purchase history. The synthetic receipts, deeper in the tree, stay readable. |
| `Read(~/qm-data/**)` | The exports directory, where Compose looks for it: the Takeout archives, and the page that `scripts/due_page.sh` writes there when you name no other file. |
| `Read(~/.config/qrtrmastr/**)` | The configuration directory: where Compose looks for it on every system, and where a `qm` that runs without a container looks on Linux. It holds the terms of the canary, which are your name, address, phone and mail address. |
| `Read(~/Library/Application Support/qrtrmastr/**)` | Where a `qm` that runs without a container keeps the configuration and the Gmail token on macOS. |
| `Read(~/.local/state/qrtrmastr/**)` | The state directory: where Compose looks for it on every system, and where a `qm` without a container looks on Linux. It holds the Gmail token of `qm fetch`. |
| `Read(//**/*.mbox)` | A mailbox file in any place. A Takeout archive is a `.zip` file, and no rule knows one outside the exports directory: keep the archives there. |
| `Read(//**/*.eml)` | A message file in any place. |

`scripts/check.sh rules`, a part of the `privacy` group, reads the settings as JSON and fails when this page and the `deny` list do not hold the same ten `Read` rules, and when a rule names a home directory. It compares the `Read` rules only, and it does not know whether a rule means what its row says.

If you keep one of these directories in another place, the rules above do not know it. The settings that move one are `QM_DATA_DIR`, `QM_CONFIG_DIR` and `QM_STATE_DIR` for Compose, `QM_CONFIG`, `QM_STATE`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME` for a `qm` without a container, `QM_CANARY_TERMS` for the terms of the canary, and the file that you name to `scripts/due_page.sh`. Copy `.claude/settings.local.example.json` to `.claude/settings.local.json`, which git ignores, and write your places into it. A path that starts with two slashes is absolute:

```json
{
  "permissions": {
    "deny": [
      "Read(//path/to/your/exports/**)",
      "Read(//path/to/your/config/**)",
      "Read(//path/to/your/state/**)"
    ]
  }
}
```

What the rules do and do not do:

* Claude Code applies a rule to its file tools, as a best effort for its search tools, and to a shell command that names the file and that it can read as a file command, such as `cat` or `head`. A rule does not stop a program that opens the file itself, such as a script. The sandbox of Claude Code is the control for that, and this repository does not switch it on.
* A rule is about a session of Claude Code only. It does nothing for another tool, another assistant, or a person with a shell.
* A rule says nothing about what git tracks. `.gitignore` and the canaries below are the controls for that.
* The rules are a file in the repository. On 2026-11-21 Claude Code refused the assistant's edit of them, and a person made it; a mode of Claude Code that skips the permission prompts does not give that protection. A change to `.claude/settings.json` is reviewed in a pull request like any other change.
* A copy of a protected file under another name or in another place has no rule: a copy of `.env`, an archive in a downloads directory, a page of `qm due --html` that you write somewhere else.

## The two canaries

`scripts/pii_canary.sh` runs in two modes.

* Shape mode, in CI on every pull request and push: fails when a file under `tests/fixtures/local/` or `backups/` is tracked, or when a tracked file matches the shape of a phone number, an email address, a masked card number, a street address, or the home directory of a person (a path below `/home` or `/Users` with the name of an account, which is why a rule or a document writes `~/`). The canary fails closed: a file it cannot read is a failure, not a pass, and file names reach `grep` as arguments so a space in a name changes nothing.
* Term mode, in the Stop hook on your machine: fails when any line of `~/.config/qrtrmastr/canary-terms.txt`, the real address, phone, name and email, appears in a tracked file. CI cannot run this mode because it does not know the terms, which is the point.

Since 2026-11-21 both modes read every tracked file except three kinds: images, lockfiles, and the synthetic input fixtures. So they read also the settings, hooks and rules of a Claude session, the workflows, the scripts, the instructions for an agent and the vendored standard. A run fails when it does not read each of ten named files, among them `.claude/settings.json` and `CLAUDE.md`, and when it leaves out more tracked files than a number that the script holds, 8 today. An exclusion that is added to the script therefore fails until that number is raised in the same change. `scripts/pii_canary.test.sh` plants each shape in each area in a repository of its own and asserts the failure.

What the shape of a home directory does not see: the two roots in another spelling (`/users/` is also the start of a path of the Gmail API), a Windows path with backslashes, another root such as `/var/home`, the form `~name`, and a made-up account name that is also a real one (`alice`, `user`). A reserved form of one shape also excuses another shape on the same match, and the file name is a part of what the filter reads; both are older than this change. Term mode, with your own terms, is the control for your own name.

The synthetic fixtures under `tests/fixtures/synthetic/input/` carry fake personal data of the right shapes on purpose, in reserved forms that cannot be real: a 555-01xx phone number, an `example.com` address, an invented street. The canary skips that directory and scans what the parser makes of it: the parser snapshot under `src/`, which holds the order and its lines and none of the fake personal data. A test asserts that no stored row holds a subject, a body or a cell of a row that the parser could not read.

No fixture is made from a real receipt: the command that wrote one is gone since 2026-11-27, because the store holds no mail. A file that is still in `tests/fixtures/local/` from before stays ignored by git, and the canary refuses to see it tracked.

## Residual risk

A redacted receipt still names what one household buys. That is why real receipts stay out of git even after redaction. The database itself is local; a backup of it is treated like the database: `backups/` is the Compose mount point for dumps, its contents are ignored by git, and the canary fails if anything but its `.gitkeep` is tracked.
