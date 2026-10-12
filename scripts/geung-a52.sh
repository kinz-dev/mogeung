#!/usr/bin/env bash
# A52 — a seat launched with flags only leaves the user's home as a plain
# `claude` launch in that directory would: nothing in settings.json, and
# nothing in ~/.claude.json beyond what every launch writes.
#
# What it does (`R-P2`, phase 0 — docs/features/0051-gau-geung.md):
#
#   three launches in this repository, each: start, ask for one word, leave.
#     plain #1   claude                       (warm-up: first-time writes land here)
#     seat       claude --settings <file> --session-id … --name … --model …
#                with the same hooks, status line and ask rule A51 uses
#     plain #2   claude
#   before and after each, a snapshot of settings.json and ~/.claude.json
#   (sha256 plus the file itself), and the names of everything at the top of
#   ~/.claude.
#
#   verdict: settings.json unchanged across all three, and every key path the
#   seat changed in ~/.claude.json is one a plain launch changed too.
#
# Usage: scripts/geung-a52.sh                 (GEUNG_MODEL=haiku, GEUNG_RUN_ROOT=$TMPDIR/geung-phase0)
#        scripts/geung-a52.sh --reread <run>   judge a saved run again, launching nothing
#
# bash 3.2.

set -uo pipefail
. "$(dirname "$0")/geung-lib.sh"
geung_need claude tmux python3

repo=$(geung_repo_root)
home=$(geung_claude_home)
cjson=$(geung_claude_json)
settings="$home/settings.json"

# --- the verdict, from a run directory's saved diffs -----------------------------
# `geung-a52.sh --reread <run>` judges an earlier run again without launching
# anything: the diffs are the evidence, and the rule below may be refined
# after reading them. It was, on 2026-10-09 — see the note on tips.
verdict() {
  local run="$1" t
  geung_hdr "key paths changed in $cjson"
  for t in plain1 seat plain2; do
    geung_say "  $t:"
    sed 's/^/    /' "$run/$t.claude.diff"
  done
  geung_hdr "names changed at the top of $home"
  for t in plain1 seat plain2; do
    geung_say "  $t:"
    if [ -s "$run/$t.home.diff" ]; then sed 's/^/    /' "$run/$t.home.diff"; else geung_say "    (none)"; fi
  done

  # Paths only — "added" in the warm-up and "changed" later is the same path.
  # C collation throughout: sort and comm must agree on the order of `›`.
  cut -d' ' -f2- "$run/seat.claude.diff" | LC_ALL=C sort -u > "$run/seat.paths"
  cut -d' ' -f2- "$run/plain1.claude.diff" "$run/plain2.claude.diff" | LC_ALL=C sort -u > "$run/plain.paths"
  LC_ALL=C comm -23 "$run/seat.paths" "$run/plain.paths" > "$run/seat-only.paths"
  cut -d' ' -f2- "$run/seat.home.diff" | LC_ALL=C sort -u > "$run/seat.home.paths"
  cut -d' ' -f2- "$run/plain1.home.diff" "$run/plain2.home.diff" | LC_ALL=C sort -u > "$run/plain.home.paths"
  LC_ALL=C comm -23 "$run/seat.home.paths" "$run/plain.home.paths" > "$run/seat-only.home.paths"

  # A leaf the seat alone changed is excused when its top-level key is one a
  # plain launch changed too, and that key is not `projects`. The tip
  # rotation is why: every launch draws a tip and bumps `tipsHistory`,
  # `tipLifetimeShownCounts` and `tipsHistoryByCommand` under *that tip's*
  # id, so at leaf level two launches never match and the family always
  # does. Project entries stay leaf-exact — a trust flag, an allowed tool or
  # an MCP server written there is precisely what this test exists to catch;
  # and a top-level key no plain launch touched (`mcpServers`, say) is never
  # excused.
  sed 's/ › .*//' "$run/plain.paths" | LC_ALL=C sort -u > "$run/plain.families"
  : > "$run/seat-only.real"
  : > "$run/seat-only.rotation"
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    fam=${path%% › *}
    if [ "$fam" != projects ] && grep -Fxq -- "$fam" "$run/plain.families"; then
      printf '%s\n' "$path" >> "$run/seat-only.rotation"
    else
      printf '%s\n' "$path" >> "$run/seat-only.real"
    fi
  done < "$run/seat-only.paths"

  local settings_ok=1
  for t in plain1 seat plain2; do
    [ "$(cat "$run/$t.before.settings.sha")" = "$(cat "$run/$t.after.settings.sha")" ] || settings_ok=0
  done

  if [ -s "$run/seat-only.rotation" ]; then
    geung_hdr "seat-only leaves under a key every launch rotates (excused)"
    sed 's/^/  claude.json: /' "$run/seat-only.rotation"
  fi
  if [ "$settings_ok" -eq 1 ] && [ ! -s "$run/seat-only.real" ] && [ ! -s "$run/seat-only.home.paths" ]; then
    geung_verdict SUPPORTED A52 "settings.json untouched by all three; every key the seat changed in claude.json is one a plain launch changes (project entries leaf-exact); nothing new at the top of $home"
    return 0
  fi
  geung_hdr "what the seat changed that no plain launch did"
  [ -s "$run/seat-only.real" ] && sed 's/^/  claude.json: /' "$run/seat-only.real"
  [ -s "$run/seat-only.home.paths" ] && sed "s|^|  $home: |" "$run/seat-only.home.paths"
  [ "$settings_ok" -eq 1 ] || geung_say "  settings.json changed — see the diffs above"
  geung_verdict REFUTED A52 "the seat left something a plain launch does not; read $run. ADR-0043 says the opt-in home becomes Claude Code's default"
  return 1
}

if [ "${1:-}" = "--reread" ]; then
  [ -d "${2:-}" ] || { geung_fail "--reread needs a run directory"; exit 2; }
  geung_hdr "A52 · re-reading $2"
  verdict "$2"; exit $?
fi

run=$(geung_run_dir a52)

geung_hdr "A52 · run directory $run"
geung_say "  claude $(claude --version 2>/dev/null | head -1) · model $GEUNG_MODEL · cwd $repo"
geung_say "  home $home"
geung_say "  settings $settings"
geung_say "  claude.json $cjson"
[ -f "$cjson" ] || { geung_fail "$cjson does not exist — nothing to compare against"; exit 2; }

# --- the key-path diff ----------------------------------------------------------
cat > "$run/jsondiff.py" <<'EOF'
# Prints one line per leaf whose value differs: "<added|removed|changed> <path>".
# Arrays are leaves; a project entry's keys are leaves under its path.
import json, sys

def flat(o, p, out):
    if isinstance(o, dict):
        for k, v in o.items():
            flat(v, p + (k,), out)
    else:
        out[p] = json.dumps(o, sort_keys=True)

def load(f):
    try:
        with open(f) as h:
            return json.load(h)
    except FileNotFoundError:
        return {}

a, b = {}, {}
flat(load(sys.argv[1]), (), a)
flat(load(sys.argv[2]), (), b)
for k in sorted(set(a) | set(b)):
    if a.get(k) != b.get(k):
        kind = 'added' if k not in a else 'removed' if k not in b else 'changed'
        print(kind, ' › '.join(k))
EOF

snapshot() {   # snapshot <tag>
  local t="$1"
  cp "$settings" "$run/$t.settings.json" 2>/dev/null || : > "$run/$t.settings.json"
  cp "$cjson" "$run/$t.claude.json"
  ls -1 "$home" > "$run/$t.home.ls" 2>/dev/null
  geung_sha "$run/$t.settings.json" > "$run/$t.settings.sha"
  geung_sha "$run/$t.claude.json" > "$run/$t.claude.sha"
}

one_launch() {   # one_launch <tag> <launcher>
  local t="$1" l="$2" s="geung-a52-$1"
  snapshot "$t.before"
  geung_launch "$s" "$l"
  if geung_wait_screen "$s" "$GEUNG_READY_RE" 60; then
    geung_ok "$t at its prompt"
  else
    geung_note "$t: did not see '$GEUNG_READY_RE' — continuing"
    geung_dump "$s" "$run/$t-not-ready.txt"
  fi
  geung_type "$s" "Reply with exactly one word: pong"
  if geung_wait_screen "$s" '^[^>]*\bpong\b' 90; then
    geung_ok "$t answered"
  else
    geung_note "$t did not answer within 90 s"
    geung_dump "$s" "$run/$t-no-answer.txt"
  fi
  sleep 3
  geung_exit_claude "$s"
  sleep 2
  snapshot "$t.after"
  python3 -I "$run/jsondiff.py" "$run/$t.before.claude.json" "$run/$t.after.claude.json" > "$run/$t.claude.diff"
  diff "$run/$t.before.home.ls" "$run/$t.after.home.ls" | sed -n 's/^> /added  /p; s/^< /removed /p' > "$run/$t.home.diff"
  if [ "$(cat "$run/$t.before.settings.sha")" = "$(cat "$run/$t.after.settings.sha")" ]; then
    geung_ok "$t: settings.json untouched"
  else
    geung_fail "$t: settings.json CHANGED"
    diff "$run/$t.before.settings.json" "$run/$t.after.settings.json" | head -40
  fi
  printf '  %-10s claude.json: %s key path(s) changed; %s name(s) changed at the top of %s\n' \
    "$t" "$(wc -l < "$run/$t.claude.diff" | tr -d ' ')" "$(wc -l < "$run/$t.home.diff" | tr -d ' ')" "$home"
}

# --- the seat's settings, the same shape A51 uses --------------------------------
cat > "$run/receive.sh" <<EOF
#!/usr/bin/env bash
kind="\${1:-hook}"
payload=\$(cat)
printf '%s\t%s\n' "\$(date +%H:%M:%S)" "\$payload" >> "$run/\$kind.log"
if [ "\$kind" = status ]; then printf 'geung-a52 status line\n'; fi
exit 0
EOF
chmod +x "$run/receive.sh"
cat > "$run/seat-settings.json" <<EOF
{
  "hooks": {
    "SessionStart": [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ],
    "Stop":         [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ],
    "Notification": [ { "hooks": [ { "type": "command", "command": "$run/receive.sh hook" } ] } ]
  },
  "statusLine": { "type": "command", "command": "$run/receive.sh status" },
  "permissions": { "ask": [ "Bash(echo geung-a52)" ] }
}
EOF

geung_write_launcher "$run/plain1.sh" "$repo" claude --model "$GEUNG_MODEL"
geung_write_launcher "$run/seat.sh"   "$repo" claude \
  --settings "$run/seat-settings.json" \
  --session-id "$(geung_uuid)" \
  --name geung-a52 \
  --model "$GEUNG_MODEL"
geung_write_launcher "$run/plain2.sh" "$repo" claude --model "$GEUNG_MODEL"

geung_hdr "plain #1 (warm-up)"; one_launch plain1 "$run/plain1.sh"
geung_hdr "seat";               one_launch seat   "$run/seat.sh"
geung_hdr "plain #2";           one_launch plain2 "$run/plain2.sh"

verdict "$run"
exit $?
