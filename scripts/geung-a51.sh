#!/usr/bin/env bash
# A51 — hooks and a status line handed to Claude Code with `--settings` fire
# for that session exactly as they would from settings.json, and for that
# session only.
#
# What it does, on this machine, by hand (`R-P2`, phase 0 of the Gau Geung
# spec — docs/features/0051-gau-geung.md):
#
#   1. launches one *seat*: `claude --settings <file>` in this repository,
#      where the file carries SessionStart, Stop and Notification hooks, a
#      status line, and a permissions.ask rule for one command — every hook
#      appends what it was given to a log in the run directory;
#   2. asks it to run that one command, so the ask rule raises a permission
#      menu and the Notification hook has something to fire on; approves it;
#      waits for Stop; leaves;
#   3. launches a plain `claude` in the same directory, asks for one word,
#      leaves;
#   4. reads the log: every entry must carry the seat's session id and none
#      the plain session's.
#
# The receiver the hooks post to is a file here, not the daemon's — `R-P5`
# builds that, and the question this answers comes first: does the flag
# carry hooks at all, and to one session only.
#
# Usage: scripts/geung-a51.sh        (GEUNG_MODEL=haiku, GEUNG_RUN_ROOT=$TMPDIR/geung-phase0)
#
# bash 3.2.

set -uo pipefail
. "$(dirname "$0")/geung-lib.sh"
geung_need claude tmux python3

repo=$(geung_repo_root)
run=$(geung_run_dir a51)
seat_s="geung-a51-seat"
plain_s="geung-a51-plain"
seat_id=$(geung_uuid)

geung_hdr "A51 · run directory $run"
geung_say "  claude $(claude --version 2>/dev/null | head -1) · model $GEUNG_MODEL · cwd $repo"
geung_say "  seat session id $seat_id"

# --- the receiver: one line per call, whatever stdin carried ---------------
cat > "$run/receive.sh" <<EOF
#!/usr/bin/env bash
# Appends the JSON a hook (or the status line) was handed, one line per call.
kind="\${1:-hook}"
payload=\$(cat)
printf '%s\t%s\n' "\$(date +%H:%M:%S)" "\$payload" >> "$run/\$kind.log"
if [ "\$kind" = status ]; then printf 'geung-a51 status line\n'; fi
exit 0
EOF
chmod +x "$run/receive.sh"

# --- the settings the seat gets, and nothing else does ----------------------
cat > "$run/seat-settings.json" <<EOF
{
  "hooks": {
    "SessionStart": [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ],
    "Stop":         [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ],
    "Notification": [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ]
  },
  "statusLine": { "type": "command", "command": "$run/receive.sh status" },
  "permissions": { "ask": [ "Bash(echo geung-a51)" ] }
}
EOF

# --- 1. the seat --------------------------------------------------------------
geung_hdr "seat: claude --settings $run/seat-settings.json"
geung_write_launcher "$run/seat.sh" "$repo" claude \
  --settings "$run/seat-settings.json" \
  --session-id "$seat_id" \
  --name geung-a51 \
  --model "$GEUNG_MODEL"
geung_launch "$seat_s" "$run/seat.sh"

if geung_wait_file "$run/hook.log" 'hook_event_name"[[:space:]]*:[[:space:]]*"SessionStart' 60; then
  geung_ok "SessionStart hook fired"
else
  geung_fail "no SessionStart within 60 s"
  geung_dump "$seat_s" "$run/seat-no-sessionstart.txt"
  geung_exit_claude "$seat_s"
  geung_verdict INCONCLUSIVE A51 "the seat never reported SessionStart; read $run/seat-no-sessionstart.txt"
  exit 1
fi

if geung_wait_screen "$seat_s" "$GEUNG_READY_RE" 45; then
  geung_ok "seat is at its prompt"
else
  geung_note "did not see '$GEUNG_READY_RE' on the seat's screen — sending the prompt anyway"
  geung_dump "$seat_s" "$run/seat-not-ready.txt"
fi

geung_type "$seat_s" "Use the Bash tool to run exactly this command and nothing else: echo geung-a51"

notified=0
if geung_wait_file "$run/hook.log" 'hook_event_name"[[:space:]]*:[[:space:]]*"Notification' 90; then
  notified=1
  geung_ok "Notification hook fired"
  sleep 1
  geung_excerpt "$seat_s" > "$run/permission-menu.txt"
  geung_note "permission menu excerpt saved to $run/permission-menu.txt (a candidate A54 fixture)"
  # The menu's first entry is highlighted; Enter approves this one call and
  # writes nothing.
  geung_key "$seat_s" Enter
else
  geung_note "no Notification within 90 s — either the ask rule did not raise a menu or the model did not call Bash"
  geung_dump "$seat_s" "$run/seat-no-notification.txt"
fi

if geung_wait_file "$run/hook.log" 'hook_event_name"[[:space:]]*:[[:space:]]*"Stop' 120; then
  geung_ok "Stop hook fired"
else
  geung_fail "no Stop within 120 s"
  geung_dump "$seat_s" "$run/seat-no-stop.txt"
fi

# Give the status line a beat to render after the turn.
sleep 3
geung_excerpt "$seat_s" > "$run/seat-after-turn.txt"
geung_exit_claude "$seat_s"

# --- 2. a plain launch in the same directory -----------------------------------
geung_hdr "plain: claude, same directory, no --settings"
geung_write_launcher "$run/plain.sh" "$repo" claude --model "$GEUNG_MODEL"
geung_launch "$plain_s" "$run/plain.sh"
if geung_wait_screen "$plain_s" "$GEUNG_READY_RE" 60; then
  geung_ok "plain session is at its prompt"
else
  geung_note "did not see '$GEUNG_READY_RE' on the plain screen — continuing"
  geung_dump "$plain_s" "$run/plain-not-ready.txt"
fi
geung_type "$plain_s" "Reply with exactly one word: pong"
if geung_wait_screen "$plain_s" '^[^>]*\bpong\b' 90; then
  geung_ok "plain session answered"
else
  geung_note "plain session did not answer within 90 s"
  geung_dump "$plain_s" "$run/plain-no-answer.txt"
fi
sleep 3
geung_exit_claude "$plain_s"

# --- 3. the ledger -------------------------------------------------------------
geung_hdr "what the receiver saw"
hooks=0; ss=0; st=0; nt=0; foreign=0; status_n=0
if [ -f "$run/hook.log" ]; then
  hooks=$(wc -l < "$run/hook.log" | tr -d ' ')
  ss=$(grep -Ec 'hook_event_name"[[:space:]]*:[[:space:]]*"SessionStart' "$run/hook.log")
  st=$(grep -Ec 'hook_event_name"[[:space:]]*:[[:space:]]*"Stop' "$run/hook.log")
  nt=$(grep -Ec 'hook_event_name"[[:space:]]*:[[:space:]]*"Notification' "$run/hook.log")
  foreign=$(grep -Evc "session_id\"[[:space:]]*:[[:space:]]*\"$seat_id\"" "$run/hook.log")
fi
[ -f "$run/status.log" ] && status_n=$(wc -l < "$run/status.log" | tr -d ' ')

printf '  %-28s %s\n' "hook calls" "$hooks"
printf '  %-28s %s\n' "  SessionStart" "$ss"
printf '  %-28s %s\n' "  Stop" "$st"
printf '  %-28s %s\n' "  Notification" "$nt"
printf '  %-28s %s\n' "  from another session" "$foreign"
printf '  %-28s %s\n' "status line calls" "$status_n"
if [ -f "$run/hook.log" ]; then
  geung_say ""
  geung_say "  session ids seen by the hooks:"
  sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/    \1/p' "$run/hook.log" | sort -u
fi

if [ "$ss" -ge 1 ] && [ "$st" -ge 1 ] && [ "$status_n" -ge 1 ] && [ "$foreign" -eq 0 ]; then
  if [ "$nt" -ge 1 ]; then
    geung_verdict SUPPORTED A51 "SessionStart, Stop and Notification fired through --settings, the status line rendered, and the plain session reached none of them"
  else
    geung_verdict SUPPORTED A51 "SessionStart and Stop fired through --settings, the status line rendered, the plain session reached none of them; Notification was not exercised (no permission menu came up)"
  fi
  exit 0
fi
if [ "$foreign" -gt 0 ]; then
  geung_verdict REFUTED A51 "$foreign hook call(s) came from a session that was not the seat — the settings leaked. ADR-0043 says the opt-in home becomes Claude Code's default"
  exit 1
fi
geung_verdict REFUTED A51 "SessionStart=$ss Stop=$st status=$status_n — something handed through --settings did not fire; read $run"
exit 1
