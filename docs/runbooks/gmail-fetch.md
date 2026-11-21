---
type: Runbook
title: Fetch new receipts from Gmail
description: The one-time setup of a Google Cloud project and the sign-in, then qm fetch for the receipts that arrived since the last import.
tags: [runbook, import, gmail]
generated: { by: "process:claude-code", at: "2026-11-19T20:00:00Z" }
updated: 2026-11-20
sources:
  - id: gmail
    resource: ../../src/adapters/mail/gmail.rs
    title: The Gmail API adapter
---

# Fetch new receipts from Gmail

`qm fetch` reads the receipts that arrived since the last import, with read-only access ([ADR 0013](/decisions/0013-new-receipts-are-fetched-from-the-gmail-api.md)). The history before that comes from a Takeout export ([Export your FreshDirect mail and import it](/runbooks/takeout-export.md)); do that first. The redaction terms in `config.toml` are needed as for an import.

Google changes the names of the pages of its console. The steps below say what must exist at the end of each; the names are those of November 2026 as the author knew them, not verified against the console.

## 1. A Google Cloud project with an OAuth client (once)

1. Open https://console.cloud.google.com and create a project. The name is yours to choose.
2. In "APIs & Services", enable the **Gmail API** for the project.
3. Configure the consent screen ("Google Auth Platform", or "OAuth consent screen"): user type **External**, your own address as the support and developer contact. Add the scope `https://www.googleapis.com/auth/gmail.readonly` if the page asks for scopes.
4. Set the publishing status to **In production**. While the status is "Testing", a sign-in ends after seven days and `qm fetch` asks you to sign in again. Google shows a warning that the app is not verified when you sign in; that is expected for an app that only you use.
5. Create a client ("Clients", or "Credentials" then "Create credentials", "OAuth client ID"): application type **Desktop app**.
6. Copy the client ID and the client secret into `.env` in the project directory:

   ```
   QM_GMAIL_CLIENT_ID=...apps.googleusercontent.com
   QM_GMAIL_CLIENT_SECRET=...
   ```

## 2. The state directory (once)

The sign-in is kept in a file outside the repository. Create its directory before the first run; Docker creates a missing one as root, and the tool then cannot write to it.

```
mkdir -p ~/.local/state/qrtrmastr && chmod 700 ~/.local/state/qrtrmastr
```

## 3. Sign in (once, and again when Google ends the sign-in)

```
docker compose run --rm qm gmail login
```

1. The command prints an address. Open it in a browser, choose your account, pass the "not verified" warning with "Advanced", and approve read-only access.
2. The browser goes to an address that starts with `http://127.0.0.1:8765` and shows an error page. Nothing listens there; that is expected.
3. Copy the whole address from the address bar, paste it into the terminal and press Enter.

Expected: `signed in; qm fetch reads your new receipts`. The file `~/.local/state/qrtrmastr/gmail-token.json` now exists and only you can read it. It gives read access to your mail: treat it like a password. To end the access, remove the app at https://myaccount.google.com/permissions and delete the file.

## 4. Fetch

```
docker compose run --rm qm fetch
```

Expected: the lines of an import: messages read, new, already known; receipts parsed. A run reads the mail of the configured senders received since a week before the newest stored message, so most messages of a run are already known. `--since 2026-11-01` reads from another day; `--max 2000` raises the limit of one run.

Then write the page again: `scripts/due_page.sh`.

## When it fails

| The command says | Do this |
| --- | --- |
| `Gmail is not set up` | Step 1: the two values are not in `.env`. |
| `no Gmail sign-in is stored` | Step 3. |
| `the token file ...: PermissionDenied; does the state directory exist?` | Step 2. If Docker created the directory as root: `sudo chown -R "$USER" ~/.local/state/qrtrmastr`. |
| `the stored Gmail sign-in is no longer valid` | Step 3 again. If it happens each week, the project is still in "Testing": step 1, point 4. |
| `Google did not give access: access_denied` | You chose "Cancel", or the account is not allowed: with the status "Testing", add your address as a test user. |
| `status 403 (PERMISSION_DENIED)` | The Gmail API is not enabled for the project: step 1, point 2. |
| `the pasted address cannot be used` | Copy the address the browser ended on, whole, from the same `qm gmail login` run. |
