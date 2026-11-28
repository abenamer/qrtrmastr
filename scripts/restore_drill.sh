#!/usr/bin/env bash
# The backup of the Compose database, and the restore drill (REAS-DATA-003).
#
#   scripts/restore_drill.sh           take a backup, restore it into a PostgreSQL container
#                                      that is thrown away, and compare each table with the
#                                      database. The backup that it names is the one it proved
#   scripts/restore_drill.sh backup    take a backup only; this one is not proved
#
# What it does to the database of the Compose project: it reads it. `pg_dump` and a `select`
# for each table run inside the `qm-postgres` container; nothing is written there, no container
# of the project is stopped, and no volume of the project is touched. The script calls
# `docker exec` and not `docker compose`, so it reads no `.env` and no password: each command
# runs inside a container, with the user that the container was started with.
#
# The backup is one file in `backups/`, in the custom format of `pg_dump`. `backups/` is on
# the disk of this machine and not in the Docker volume, and git ignores it. Copy the file to
# another disk or machine: a backup beside the data is lost with the machine. The file holds
# the database: your orders, and for rows of before 2026-11-24 the redacted mail; keep it as
# private as the mail.
#
# The drill starts a second PostgreSQL from the image of the first, with its data in memory
# and without a network, restores the file into it with the command of the runbook, and
# compares the number of rows and a hash of the rows of each table with the database. A table
# that differs fails the drill. Do not run `qm` while the drill runs: a row that is written
# after the backup is a difference. The second container is removed when the script ends, also
# when it fails.
#
# The last lines of a drill that passed are its record. They name tables and counts, no
# content. Put them into docs/log.md with the date (docs/runbooks/backup-and-restore.md). The
# output of a drill that failed can hold a piece of a stored row, in an error of the restore:
# do not put it into a log, an issue or a session.
set -euo pipefail
cd "$(dirname "$0")/.."

SOURCE=qm-postgres
DRILL=qm-restore-drill
DRILL_DATABASE=restore_drill
# The same options as the restore of the runbook, so that the drill proves that command.
RESTORE_OPTIONS='--no-owner --no-privileges --single-transaction --exit-on-error'
mode="${1:-drill}"
case "$mode" in
  drill | backup) ;;
  *)
    echo "usage: scripts/restore_drill.sh [backup]" >&2
    exit 2
    ;;
esac

# One line for each table of the schema: its name, its number of rows, and the sum of a hash
# of each row. The sum does not depend on the order of the rows.
FINGERPRINT="select format('%s %s %s', t.table_name,
    (xpath('/row/n/text()', x.doc))[1]::text, (xpath('/row/h/text()', x.doc))[1]::text)
  from information_schema.tables t,
  lateral (select query_to_xml(format(
    'select count(*) as n, coalesce(sum(hashtextextended(r::text, 0)), 0) as h from %I.%I r',
    t.table_schema, t.table_name), false, true, '') as doc) x
  where t.table_schema = 'public' and t.table_type = 'BASE TABLE'
  order by t.table_name"

in_source() { docker exec -i "$SOURCE" sh -c "$1"; }
in_drill() { docker exec -i "$DRILL" sh -c "$1"; }
fail() {
  echo "FAIL: $1" >&2
  exit 1
}

if [ "$(docker inspect --format '{{.State.Running}}' "$SOURCE" 2> /dev/null || true)" != true ] \
  || ! in_source 'pg_isready -q -U "$POSTGRES_USER" -d "$POSTGRES_DB"'; then
  fail "the database container $SOURCE is not running and ready; start it with: docker compose up -d --wait"
fi

mkdir -p backups
umask 077
stamp=$(date +%Y-%m-%d-%H%M%S)
file="backups/qrtrmastr-$stamp.dump"
partial="$file.partial"
if [ -e "$file" ] || [ -e "$partial" ]; then
  fail "$file or its partial file is there already"
fi
# The backup has its name only when `pg_dump` ended well: a file that was cut short keeps the
# name of a partial file, also when the script is stopped from the keyboard.
if ! in_source 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom' > "$partial"; then
  rm -f "$partial"
  fail "pg_dump did not finish; no backup was written"
fi
if [ ! -s "$partial" ]; then
  rm -f "$partial"
  fail "the backup is empty; no backup was written"
fi
mv "$partial" "$file"
bytes=$(wc -c < "$file" | tr -d ' ')
sum=$(sha256sum "$file" | cut -d' ' -f1)
echo "backup: $file, $bytes bytes, sha256 $sum"
if [ "$mode" = backup ]; then
  echo "PASS: the backup is written. It is not proved: scripts/restore_drill.sh without an argument takes a backup and restores it."
  exit 0
fi

not_proved="the backup $file is written and is not proved"
image=$(docker inspect --format '{{.Image}}' "$SOURCE")
image_name=$(docker inspect --format '{{.Config.Image}}' "$SOURCE")
if docker inspect "$DRILL" > /dev/null 2>&1; then
  fail "a container named $DRILL is there already; look at it, then remove it: docker rm -f $DRILL. $not_proved"
fi
# `-v` removes a volume without a name that belongs to this one container, if the image made
# one. It removes no volume with a name and no volume of another container.
remove_drill() { docker rm -f -v "$DRILL" > /dev/null 2>&1 || true; }
trap remove_drill EXIT
# The password is for a server that lives for this run, has no network, and keeps its data in
# memory.
docker run -d --name "$DRILL" --network none --tmpfs /var/lib/postgresql/data \
  -e POSTGRES_USER=drill -e POSTGRES_PASSWORD="drill-$stamp-$RANDOM$RANDOM" "$image" > /dev/null
# An image that keeps its data in another directory would put it into a volume, on the disk.
if [ "$(docker inspect --format '{{len .Mounts}}' "$DRILL")" != 0 ]; then
  fail "the second PostgreSQL does not keep its data in memory: the image has another data directory than this script expects. $not_proved"
fi
ready=0
for _ in $(seq 1 90); do
  # The image starts a server to set itself up, which listens on no address, and then the
  # server that stays. A check through the address sees the second one only.
  if in_drill 'pg_isready -q -h 127.0.0.1 -U "$POSTGRES_USER" -d postgres' 2> /dev/null; then
    ready=1
    break
  fi
  sleep 1
done
if [ "$ready" != 1 ]; then
  echo "The last lines of the second PostgreSQL:" >&2
  docker logs --tail 20 "$DRILL" >&2 || true
  fail "the second PostgreSQL did not start. $not_proved"
fi
in_drill "createdb -U \"\$POSTGRES_USER\" $DRILL_DATABASE" \
  || fail "the second PostgreSQL made no database for the restore. $not_proved"
in_drill "pg_restore -U \"\$POSTGRES_USER\" -d $DRILL_DATABASE $RESTORE_OPTIONS" < "$file" \
  || fail "pg_restore did not finish: the backup $file does not restore. Do not copy the lines above into a log or a session"

before=$(in_source "psql -X -At -v ON_ERROR_STOP=1 -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -c \"$FINGERPRINT\"") \
  || fail "the tables of the database cannot be read. $not_proved"
after=$(in_drill "psql -X -At -v ON_ERROR_STOP=1 -U \"\$POSTGRES_USER\" -d $DRILL_DATABASE -c \"$FINGERPRINT\"") \
  || fail "the tables of the restore cannot be read. $not_proved"
tables=$(printf '%s\n' "$before" | grep -c . || true)
if [ "$tables" -lt 1 ]; then
  fail "the database has no table; this is no drill"
fi
if [ "$before" != "$after" ]; then
  echo "FAIL: the restore of $file is not the database. Tables that differ, with their rows (database, then restore):" >&2
  diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | grep '^[<>]' | cut -d' ' -f1-3 >&2 || true
  exit 1
fi
rows=$(printf '%s\n' "$before" | awk '{ n += $2 } END { print n + 0 }')
echo
echo "The record of the drill:"
echo "restore drill on $(date +%Y-%m-%d): $file, $bytes bytes, sha256 $sum"
echo "restored into a new container of the image $image_name (${image#sha256:}); $tables tables and $rows rows, each table with the same count and the same hash as the database"
printf '%s\n' "$before" | awk '{ printf "  %s: %s rows\n", $1, $2 }'
echo "PASS: the backup $file restores. Copy this file to another disk or machine."
