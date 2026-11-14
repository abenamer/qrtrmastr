#!/bin/bash
# PreToolUse hook on Bash: a best-effort backstop that refuses the common forms of commands that
# must never run here. It is a pattern match over the command text, not a security boundary: the
# controls are the Read denies and permission prompts in .claude/settings.json, GitHub branch
# protection on main, and keeping the mail export outside the repository. Exit 2 blocks the
# command and sends the message to Claude as the reason.
#   - `docker compose down -v` and `docker volume rm` delete the data volume (REAS-DATA-003).
#   - A forced push, or any push whose refspec lands on main, bypasses the pull-request flow.
#   - A different entrypoint on the qm container would read the mounted export or configuration.
#   - Any command that names a mail export (.mbox, .eml, the takeout directory) other than the
#     import tooling, natively or through Compose, and plain file management would put the
#     maintainer's address and phone into the transcript.
# A pattern matches in command position (the start of a line, or after ; && || | or an opening
# parenthesis), after an optional sudo, env, command, nice, time or nohup prefix and an optional
# backslash escape, so a commit message or an echo that mentions the command passes.
# `guard-bash.test.sh` beside this file exercises every rule; CI runs it.
INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
AT_COMMAND='(^|[;&|(]|\$\()[[:space:]]*((sudo|env|command|nice|time|nohup)[[:space:]]+)*\\?'

if echo "$COMMAND" | grep -Eq "${AT_COMMAND}docker([[:space:]]+compose|-compose)[^|;&]*[[:space:]]down([[:space:]][^|;&]*)?[[:space:]](-v|--volumes)([[:space:]);&|]|$)"; then
  echo "Blocked: 'docker compose down -v' deletes the data volume. Use 'docker compose down' without -v to stop (REAS-DATA-003)." >&2
  exit 2
fi

if echo "$COMMAND" | grep -Eq "${AT_COMMAND}docker[[:space:]]+volume[[:space:]]+(rm|remove|prune)"; then
  echo "Blocked: removing a Docker volume deletes the data. Back up first and do it by hand (REAS-DATA-003)." >&2
  exit 2
fi

PUSH="${AT_COMMAND}git[[:space:]]+(-[^[:space:]]+[[:space:]]+)*push"
if echo "$COMMAND" | grep -Eq "${PUSH}[^|;&]*[[:space:]](--force|-f|--force-with-lease|\+[^[:space:]]+)([[:space:]]|=|$)"; then
  echo "Blocked: a forced push rewrites history. Make a new commit instead." >&2
  exit 2
fi

if echo "$COMMAND" | grep -Eq "${PUSH}[^|;&]*([[:space:]]|:)(refs/heads/)?main([[:space:]]|$)"; then
  echo "Blocked: nothing is pushed to main directly. Push the branch and open a pull request (ADR 0002)." >&2
  exit 2
fi

if echo "$COMMAND" | grep -Eq "${AT_COMMAND}docker([[:space:]]+compose|-compose)[^|;&]*[[:space:]]run[^|;&]*--entrypoint"; then
  echo "Blocked: a different entrypoint on the qm container reads the mounted export or configuration. Use the qm commands." >&2
  exit 2
fi

if echo "$COMMAND" | grep -Eq '(\.mbox|\.eml|qm-data/takeout)' \
   && ! echo "$COMMAND" | grep -Eq "^[[:space:]]*(qm|cargo[[:space:]]+run|docker[[:space:]]+compose[[:space:]]+run([[:space:]]+--rm)?[[:space:]]+qm|ls|du|stat|file|unzip|mkdir|mv|rm|sha256sum|wc)[[:space:]]"; then
  echo "Blocked: raw mail exports carry your address and phone. Only 'qm import' and plain file management touch them; inspect redacted rows in the database instead." >&2
  exit 2
fi

exit 0
