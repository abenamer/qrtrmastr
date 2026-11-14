#!/usr/bin/env bash
# Exercises guard-bash.sh: every command listed under BLOCK must be refused (exit 2) and every
# command under ALLOW must pass (exit 0). The cases live here, in a file, because a command line
# that quoted them would itself be refused by the hook. Needs jq, as the hook does. Exit 1 on any
# mismatch.
set -u
here=$(cd "$(dirname "$0")" && pwd)
hook="$here/guard-bash.sh"
failed=0

run_case() {
  local expect="$1" command="$2" status
  printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$command" | jq -Rs .)" | bash "$hook" >/dev/null 2>&1
  status=$?
  case "$expect" in
    block) if [ "$status" -ne 2 ]; then echo "NOT BLOCKED (status $status): $command"; failed=1; fi ;;
    allow) if [ "$status" -ne 0 ]; then echo "WRONGLY BLOCKED (status $status): $command"; failed=1; fi ;;
  esac
}

while IFS= read -r line; do
  case "$line" in
    ""|"#"*) continue ;;
    "BLOCK "*) run_case block "${line#BLOCK }" ;;
    "ALLOW "*) run_case allow "${line#ALLOW }" ;;
  esac
done <<'CASES'
# Volumes
BLOCK docker compose down -v
BLOCK docker compose down --volumes
BLOCK sudo docker-compose down -v
BLOCK docker volume rm qm_postgres_data
BLOCK docker volume prune -f
ALLOW docker compose down
ALLOW docker compose up -d
ALLOW docker compose ps
# Pushes
BLOCK git push -f origin scaffold
BLOCK git push --force-with-lease origin scaffold
BLOCK env git push -f origin scaffold
BLOCK \git push --force origin main
BLOCK git push origin main
BLOCK git push -u origin main
BLOCK git push origin +HEAD:main
BLOCK git push origin HEAD:refs/heads/main
BLOCK git push origin scaffold:main
BLOCK git -c push.default=current push origin main
BLOCK cd /tmp && git push origin main
ALLOW git push -u origin scaffold
ALLOW git push
ALLOW git commit -m "mention of git push origin main in a message"
ALLOW echo "git push --force is forbidden"
# The qm container
BLOCK docker compose run --rm --entrypoint sh qm -c "cat /config/config.toml"
BLOCK docker compose run --entrypoint cat qm /data/x
ALLOW docker compose run --rm qm due
ALLOW docker compose run --rm qm import --mbox /data/takeout-2026-11/Takeout/Mail/FreshDirect.mbox
ALLOW docker compose run --rm qm fixture add abc@mail.example --dir /fixtures
ALLOW docker compose build qm
# Mail exports
BLOCK cat ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
BLOCK tac ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
BLOCK od -c ~/qm-data/takeout-2026-11/x.mbox
BLOCK head -c 2000 ~/mail/anything.eml
BLOCK grep -i address ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
BLOCK cp ~/qm-data/takeout-2026-11/x.mbox /tmp/a && cat /tmp/a
BLOCK find ~/qm-data/takeout-2026-11 -name "*.mbox" -exec cat {} +
BLOCK bash -c "cat ~/qm-data/takeout-2026-11/x.mbox"
BLOCK python3 -c "print(open('/home/x/qm-data/takeout-2026-11/x.mbox').read())"
ALLOW qm import --mbox ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
ALLOW cargo run -- import --mbox ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
ALLOW ls -la ~/qm-data/takeout-2026-11/Mail/
ALLOW du -sh ~/qm-data/takeout-2026-11/Mail/FreshDirect.mbox
ALLOW unzip -d ~/qm-data/takeout-2026-11 ~/Downloads/takeout.zip
ALLOW pg_ctl -D ~/qm-data/pgdev status
ALLOW psql -h 127.0.0.1 -p 5433 -d qrtrmastr -c "select count(*) from raw_messages"
ALLOW cargo test --lib
CASES

if [ "$failed" = 0 ]; then echo "PASS: guard-bash.sh refuses what it documents and allows the rest"; fi
exit $failed
