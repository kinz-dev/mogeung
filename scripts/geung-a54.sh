#!/usr/bin/env bash
# A54 — the gates a CLI draws on its screen are few, stable enough to pin as
# fixtures, and readable from the last forty lines of a tmux capture.
#
# This is the hand on "taken by hand": it starts a CLI in a tmux session you
# drive into each gate, and saves the excerpt ADR-0044 reads — the last forty
# visible lines, plain text — as a fixture the screen matchers (`R-P11`) will
# be tested against.
#
#   scripts/geung-a54.sh launch <cli> [dir] [-- args…]   start <cli> in tmux session geung-a54-<cli>
#   scripts/geung-a54.sh capture <cli> <gate>            save the excerpt as the fixture for <gate>
#   scripts/geung-a54.sh show <cli>                      print the excerpt now, save nothing
#   scripts/geung-a54.sh list                            the gate × CLI matrix: what is pinned
#   scripts/geung-a54.sh kill <cli>                      end the session
#
#   cli:   claude · codex · qwen · opencode
#   gate:  the closed set in ADR-0044 —
#          permission-menu · plan-approval · question · trust · login ·
#          press-enter · rate-limit · exited · prompt · working
#
# Fixtures land in crates/mogeungd/tests/fixtures/screens/<cli>/<gate>.txt
# with a <gate>.meta beside each (CLI version, size, when). Commit them; they
# are the corpus. A fixture that already exists is not overwritten unless
# GEUNG_FORCE=1 — a later version's screen is a *new* decision, not a
# replacement.
#
# Attach a terminal to drive the session:  tmux attach -t geung-a54-<cli>
# (detach with the prefix and d; the session stays).
#
# bash 3.2.

set -uo pipefail
. "$(dirname "$0")/geung-lib.sh"

repo=$(geung_repo_root)
fixtures="$repo/crates/mogeungd/tests/fixtures/screens"
CLIS="claude codex qwen opencode"
GATES="permission-menu plan-approval question trust login press-enter rate-limit exited prompt working"

usage() { sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

is_cli()  { case " $CLIS " in *" $1 "*) return 0;; esac; return 1; }
is_gate() { case " $GATES " in *" $1 "*) return 0;; esac; return 1; }

cli_version() {
  case "$1" in
    claude)   claude --version 2>/dev/null | head -1 ;;
    codex)    codex --version 2>/dev/null | head -1 ;;
    qwen)     qwen --version 2>/dev/null | head -1 ;;
    opencode) opencode --version 2>/dev/null | head -1 ;;
  esac
}

cmd="${1:-}"; [ -n "$cmd" ] || usage; shift

case "$cmd" in
  launch)
    cli="${1:-}"; is_cli "$cli" || usage; shift
    dir="$PWD"
    if [ -n "${1:-}" ] && [ "$1" != "--" ]; then dir="$1"; shift; fi
    [ "${1:-}" = "--" ] && shift
    geung_need tmux "$cli"
    s="geung-a54-$cli"
    run=$(geung_run_dir "a54-$cli")
    geung_write_launcher "$run/launch.sh" "$dir" "$cli" ${1+"$@"}
    geung_launch "$s" "$run/launch.sh"
    geung_say "$s started in $dir ($(cli_version "$cli"))"
    geung_say "  drive it:    tmux attach -t $s"
    geung_say "  then:        $0 capture $cli <gate>"
    ;;

  show)
    cli="${1:-}"; is_cli "$cli" || usage
    s="geung-a54-$cli"
    geung_alive "$s" || { geung_fail "no session $s — launch first"; exit 1; }
    geung_excerpt "$s"
    ;;

  capture)
    cli="${1:-}"; gate="${2:-}"
    is_cli "$cli" || usage
    is_gate "$gate" || { geung_fail "unknown gate '$gate' — one of: $GATES"; exit 2; }
    s="geung-a54-$cli"
    geung_alive "$s" || { geung_fail "no session $s — launch first"; exit 1; }
    out="$fixtures/$cli/$gate.txt"
    if [ -e "$out" ] && [ "${GEUNG_FORCE:-0}" != 1 ]; then
      geung_fail "$out exists — GEUNG_FORCE=1 to replace it"; exit 1
    fi
    mkdir -p "$fixtures/$cli"
    geung_excerpt "$s" > "$out"
    n=$(wc -l < "$out" | tr -d ' ')
    size=$(tmux display-message -p -t "=$s:" '#{pane_width}x#{pane_height}')
    {
      printf 'cli=%s\n' "$cli"
      printf 'version=%s\n' "$(cli_version "$cli")"
      printf 'gate=%s\n' "$gate"
      printf 'captured=%s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')"
      printf 'pane=%s\n' "$size"
      printf 'lines=%s\n' "$n"
      printf 'how=tmux capture-pane -p, trailing blank lines dropped, last 40\n'
    } > "$fixtures/$cli/$gate.meta"
    geung_ok "$out ($n lines, pane $size)"
    ;;

  list)
    printf '%-16s' "gate"
    for c in $CLIS; do printf ' %-9s' "$c"; done
    printf '\n'
    for g in $GATES; do
      printf '%-16s' "$g"
      for c in $CLIS; do
        if [ -s "$fixtures/$c/$g.txt" ]; then printf ' %-9s' "pinned"; else printf ' %-9s' "·"; fi
      done
      printf '\n'
    done
    printf '\nfixtures: %s\n' "$fixtures"
    ;;

  kill)
    cli="${1:-}"; is_cli "$cli" || usage
    s="geung-a54-$cli"
    tmux kill-session -t "=$s" 2>/dev/null && geung_ok "$s ended" || geung_note "no session $s"
    ;;

  *) usage ;;
esac
