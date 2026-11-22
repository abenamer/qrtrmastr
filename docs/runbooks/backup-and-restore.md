---
type: Runbook
title: Back up the database and restore it
description: How to take a backup of the Compose database, how to prove that it restores, where to keep it, and how to restore after the volume is lost.
tags: [runbook, database, backup, operations]
generated: { by: "process:claude-code", at: "2026-11-22T02:30:00Z" }
updated: 2026-11-21
sources:
  - id: script
    resource: ../../scripts/restore_drill.sh
    title: The backup and the restore drill
  - id: compose
    resource: ../../compose.yaml
    title: The Compose file with the named volume and the backups mount
  - id: reas-data-003
    resource: ../../standards/reas/standard/persistence.md
    title: REAS-DATA-003, data outlives the container
---

# Back up the database and restore it

The database of `qm` is in the named Docker volume `qm_postgres_data` (`compose.yaml`). A volume is not a backup: a wrong command, a broken disk or a new machine loses it. This runbook says how to take a backup, how to prove that the backup restores, and how to restore (REAS-DATA-003).

What a loss costs: the orders can be imported again from a Takeout export and from Gmail, for as long as the mail is there. What you typed is only in the database: the aliases that you merged or split, the additions with their reasons, and the products that you muted.

## What never to do

* Do not run `docker compose down -v`. The `-v` removes the volume. `docker compose down` without it keeps the volume.
* Do not run `docker volume rm qm_postgres_data`.
* Do not keep the only backup on the disk that holds the volume.

## 1. Take a backup and prove that it restores (the restore drill)

A backup that was never restored is a hope. The drill takes a backup and proves that one file. Run it for the first backup, after a change of the PostgreSQL version, after a migration that changes a table, and when you want a new backup that you can trust.

The database must run: `docker compose up -d --wait`. Do not run `qm` while the drill runs.

```
scripts/restore_drill.sh
```

The script does this:

1. It runs `pg_dump` inside the `qm-postgres` container and writes one file, `backups/qrtrmastr-<date>-<time>.dump`, that only you can read. While `pg_dump` runs, the file has `.partial` at the end of its name; a file with that name is not a backup.
2. It starts a second PostgreSQL container from the image of the first, named `qm-restore-drill`. That container has no network, and its data is in memory.
3. It restores the file into that container, with the options of step 4 below.
4. It compares each table of the restore with the database: the number of rows, and a hash of the rows.
5. It removes the second container, also when a step fails.

The drill reads the database and writes nothing to it. It does not stop the database, and it does not touch the volume. It calls `docker exec`, not `docker compose`, so it reads no `.env` and no password: each command runs inside a container, with the user of that container.

`backups/` is a directory of this repository on the disk of your machine. Git ignores its files, and a Claude session may not read them.

The last lines of a drill that passed are its record: the file, its hash, the image, and the number of rows of each table. They hold no content of a row. Put them into [the log](/log.md) with the date, and move the row of REAS-DATA-003 of the [self-audit](/self-audit.md).

If the script prints `FAIL`, do not copy its output into the log, an issue or a session: an error of `pg_restore` can hold a piece of a stored row.

| Message | What it means | What to do |
| --- | --- | --- |
| `the database container qm-postgres is not running and ready` | The container is not up. | `docker compose up -d --wait`, then run the script again. |
| `pg_dump did not finish` | No backup was written. | Read the error above the message. |
| `a container named qm-restore-drill is there already` | An earlier drill did not end. | Look at it with `docker ps -a`, then `docker rm -f qm-restore-drill`. This container holds no data that you need. |
| `does not keep its data in memory` | The image has another data directory than the script expects. PostgreSQL 18 moved it. | Do not use the script with that image before it is changed. The backup is written and not proved. |
| `the second PostgreSQL did not start` | The backup is written and not proved. | The script prints the last lines of that server above the message. |
| `pg_restore did not finish` | The backup does not restore. | Keep the file, and do not delete an older backup. |
| `is not the database` | A table has other rows than the database. | If `qm` ran during the drill, run the drill again. If not, the backup is not complete: keep the older backups. |

## 2. Copy the proved backup to another place

Copy the file that the last line of the drill names to another disk or another machine. The backup in `backups/` protects you from a lost volume. It does not protect you from a lost machine.

The file holds the database: the mail after redaction, with what you bought and when. The redactor removes the terms that you configured and three shapes; the [design review](/reference/design-review.md) lists what it can miss. Keep the file as private as the mail. Do not put it into a repository.

## 3. A backup without the proof

```
scripts/restore_drill.sh backup
```

This writes the file of step 1 and stops. Use it before a risky change, when a proved backup exists already. A file from this command passed no drill.

## 4. Restore after a loss

Use this when the volume is gone or the database is damaged. It writes to the database. The restore runs in one transaction: it restores all of the backup or nothing.

1. Start the database and wait for it. With a lost volume, Compose makes a new, empty one:

   ```
   docker compose up -d --wait
   ```

2. Only for a database that is damaged and still there: keep what is there, then make the database empty. The second command removes the database with all its rows.

   ```
   scripts/restore_drill.sh backup
   docker exec -i qm-postgres sh -c 'dropdb -U "$POSTGRES_USER" --force "$POSTGRES_DB" && createdb -U "$POSTGRES_USER" "$POSTGRES_DB"'
   ```

3. Restore the newest backup that passed a drill into the empty database. These are the options that the drill uses:

   ```
   docker exec -i qm-postgres sh -c 'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --no-owner --no-privileges --single-transaction --exit-on-error' < backups/qrtrmastr-<date>-<time>.dump
   ```

4. Apply the migrations that are newer than the backup. The backup holds the list of the migrations that it has, so with none newer the command changes nothing:

   ```
   docker compose run --rm qm migrate
   ```

5. Read the result: `docker compose run --rm qm due`.
6. Import the mail that arrived after the backup: `docker compose run --rm qm fetch`, or a new Takeout export ([Export your FreshDirect mail and import it](/runbooks/takeout-export.md)). A message that the backup holds is counted as known.

Step 2 was not run against Docker. Step 3 is what each drill runs, into a database that is empty.

## What this runbook does not do

* It makes no backup on a schedule. You run the script.
* It does not copy the backup to another place. You do.
* It does not remove old backups.
* The drill compares the rows of the tables of the schema `public`. It does not compare the view, the indexes, the constraints or the roles of the server; the restore makes them, and an error there fails the drill.
* The second container comes from the image that the database runs, by its identifier. A newer image of the same name is not used.
* The backup does not hold your configuration file, the Gmail token or `.env`. Each is on your machine, outside the database ([Configuration](/reference/configuration.md)). The token can be made again with `qm gmail login`.
