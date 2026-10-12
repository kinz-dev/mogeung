#!/usr/bin/env bash
# A53 — Claude Code keys workspace trust on the main checkout's root, so a
# seat in a worktree of a repository the user has already opened sees no
# trust dialog.
#
# What it does (`R-P2`, phase 0 — docs/features/0051-gau-geung.md):
#
#   1. adds a fresh, detached worktree of this repository under the run
#      directory;
#   2. launches a plain `claude` in it and watches the screen for up to a
#      minute: either the trust dialog comes up, or the prompt does;
#   3. leaves without accepting anything — Escape on the dialog, /exit at the
#      prompt — and removes the worktree;
#   4. says whether ~/.claude.json gained an entry for the worktree's path.
#
# Trust stays the user's act (ADR-0043 clause 6). This script never answers
# the dialog; if one appears the verdict is REFUTED and the screen is kept as a
# candidate A54 fixture.
#
# Usage: scripts/geung-a53.sh        (GEUNG_MODEL=haiku, GEUNG_RUN_ROOT=$TMPDIR/geung-phase0)
#
# bash 3.2.

set -uo pipefail
. "$(dirname "$0")/geung-lib.sh"
geung_need claude tmux git python3

repo=$(geung_repo_root)
run=$(geung_run_dir a53)
cjson=$(geung_claude_json)
wt="$run/worktree"
s="geung-a53"

geung_hdr "A53 · run directory $run"
geung_say "  claude $(claude --version 2>/dev/null | head -1) · model $GEUNG_MODEL"
geung_say "  main checkout $repo"

trusted=$(python3 -I - "$cjson" "$repo" <<'EOF'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    d = {}
p = d.get("projects", {}).get(sys.argv[2], {})
print("yes" if p.get("hasTrustDialogAccepted") else "no")
EOF
)
if [ "$trusted" = yes ]; then
  geung_ok "main checkout is trusted in $cjson (hasTrustDialogAccepted)"
else
  geung_fail "main checkout is not marked trusted in $cjson — open it with claude once by hand, then re-run"
  exit 2
fi

# --- 1. the worktree ----------------------------------------------------------------
if ! git -C "$repo" worktree add --detach "$wt" HEAD >/dev/null 2>&1; then
  geung_fail "git worktree add failed"; exit 2
fi
geung_ok "worktree at $wt ($(git -C "$wt" rev-parse --short HEAD))"

cleanup() {
  geung_alive "$s" && tmux kill-session -t "=$s" 2>/dev/null
  git -C "$repo" worktree remove --force "$wt" >/dev/null 2>&1 || true
  git -C "$repo" worktree prune >/dev/null 2>&1 || true
}
trap cleanup EXIT

# --- 2. a plain launch in it --------------------------------------------------------
geung_hdr "plain: claude in the worktree"
geung_write_launcher "$run/plain.sh" "$wt" claude --model "$GEUNG_MODEL"
geung_launch "$s" "$run/plain.sh"

# The 2.1.295 dialog says "Accessing workspace:" and "Yes, I trust this
# folder" (fixture: tests/fixtures/screens/claude/trust.txt); older builds asked
# "Do you trust the files in this folder?". Both are matched.
trust_re='Accessing workspace|trust this folder|[Tt]rust the files|[Dd]o you trust'
outcome=timeout
i=0
while [ "$i" -lt 60 ]; do
  if ! geung_alive "$s"; then outcome=gone; break; fi
  screen=$(geung_screen "$s")
  if printf '%s\n' "$screen" | grep -Eq -- "$trust_re"; then outcome=trust; break; fi
  if printf '%s\n' "$screen" | grep -Eq -- "$GEUNG_READY_RE"; then outcome=ready; break; fi
  sleep 1; i=$((i + 1))
done
sleep 1
geung_excerpt "$s" > "$run/screen.txt"
geung_note "screen saved to $run/screen.txt"

# --- 3. leave without answering anything ---------------------------------------------
case "$outcome" in
  trust)
    cp "$run/screen.txt" "$run/trust.txt"
    geung_note "the trust dialog came up — excerpt kept at $run/trust.txt (a candidate A54 fixture)"
    geung_key "$s" Escape
    geung_wait_gone "$s" 8 || tmux kill-session -t "=$s" 2>/dev/null
    ;;
  ready)
    geung_ok "at the prompt, no dialog"
    geung_exit_claude "$s"
    ;;
  gone)
    geung_note "claude exited on its own before showing anything"
    ;;
  timeout)
    geung_note "neither the dialog nor the prompt within 60 s"
    geung_exit_claude "$s"
    ;;
esac
sleep 2

# --- 4. what the home says about the worktree ------------------------------------------
geung_hdr "$cjson after the launch"
python3 -I - "$cjson" "$wt" <<'EOF'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    d = {}
p = d.get("projects", {}).get(sys.argv[2])
if p is None:
    print("  no project entry for the worktree path")
else:
    print("  project entry for the worktree path: keys", sorted(p.keys()))
    print("  hasTrustDialogAccepted:", p.get("hasTrustDialogAccepted"))
EOF

case "$outcome" in
  ready)  geung_verdict SUPPORTED A53 "a plain launch in a fresh worktree of a trusted repository went straight to the prompt"; exit 0 ;;
  trust)  geung_verdict REFUTED A53 "the trust dialog came up in the worktree; ADR-0043 clause 6 makes it a row that needs you — a click, not a write"; exit 1 ;;
  *)      geung_verdict INCONCLUSIVE A53 "outcome '$outcome' — read $run/screen.txt"; exit 1 ;;
esac
