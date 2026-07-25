#!/usr/bin/env bash
# Build and run the daemon and the window together, for development.
#
# mogeung is two processes by design — the daemon is the product and the window
# is a client with no local authority (ADR-0001). That is right, and it makes
# every dev cycle two terminals and two commands. This wraps them.
#
#   ./scripts/start.sh              build, run both, Ctrl-C stops both
#   ./scripts/start.sh --fresh      same, with a throwaway database
#   ./scripts/start.sh --debug      debug build (faster to compile)
#   mprocs                          same two processes, side by side
#
# Ctrl-C stops both. Killing the daemon alone is safe — the window reconnects
# on its own — but leaving an orphan daemon holding port 7717 is the single
# most annoying thing about running this by hand, so cleanup is a trap rather
# than a hope.

set -uo pipefail
cd "$(dirname "$0")/.."

PROFILE=release
PORT=7717
NOTIFY=1
BUILD=1
FRESH=0
MODE=both
PUSH_URL=""

# Print the header comment, stopping at the first line that is not one.
usage() {
    awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
    cat <<'EOF'

Options:
  --debug            build without optimisation (compiles faster, runs slower)
  --port N           listen port (default 7717)
  --fresh            use a throwaway database instead of ~/.mogeung/mogeung.db
  --no-notify        do not pass --notify to the daemon
  --push-url URL     forward notifications to a URL (ntfy, Pushover, webhook)
  --no-build         skip cargo build; run whatever is already there
  --daemon-only      run just the daemon (used by mprocs.yaml)
  --ui-only          run just the window, waiting for the daemon first
  -h, --help         this
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debug)       PROFILE=debug ;;
        --release)     PROFILE=release ;;
        --port)        PORT="${2:?--port needs a value}"; shift ;;
        --fresh)       FRESH=1 ;;
        --no-notify)   NOTIFY=0 ;;
        --push-url)    PUSH_URL="${2:?--push-url needs a value}"; shift ;;
        --no-build)    BUILD=0 ;;
        --daemon-only) MODE=daemon ;;
        --ui-only)     MODE=ui ;;
        -h|--help)     usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

BIN="target/$PROFILE"
HEALTH="http://127.0.0.1:$PORT/api/health"

# A throwaway database is worth having: sessions pin their diff base the first
# time they are seen, so a database carried over from an older build can make
# diffs look wrong for reasons that have nothing to do with the code you are
# testing.
DB=""
if [ "$FRESH" -eq 1 ]; then
    # TMPDIR carries a trailing slash on macOS.
    TMP="${TMPDIR:-/tmp}"
    DB="${TMP%/}/mogeung-dev.db"
fi

# Built as one array rather than returned from a function: macOS ships bash
# 3.2, which has no `mapfile`, and reading an array back from a function
# without it means re-splitting on whitespace — which would break the moment a
# path or a push URL contained a space.
ARGS=(--listen "127.0.0.1:$PORT")
[ -n "$DB" ] && ARGS+=(--db "$DB")
[ "$NOTIFY" -eq 1 ] && ARGS+=(--notify)
[ -n "$PUSH_URL" ] && ARGS+=(--push-url "$PUSH_URL")

build() {
    [ "$BUILD" -eq 0 ] && return 0
    echo "▸ building ($PROFILE)…"
    # Spelled out rather than built as an array: expanding an *empty* array
    # under `set -u` is an error in bash 3.2, which is what macOS ships.
    if [ "$PROFILE" = release ]; then
        cargo build --release || exit 1
    else
        cargo build || exit 1
    fi
}

# Anything still holding the port will make the daemon exit immediately with an
# unhelpful bind error, so say so plainly first.
free_port() {
    local holder
    holder=$(lsof -ti "tcp:$PORT" 2>/dev/null | head -1)
    [ -z "$holder" ] && return 0
    echo "▸ port $PORT held by pid $holder — stopping it"
    kill "$holder" 2>/dev/null
    for _ in $(seq 1 20); do
        lsof -ti "tcp:$PORT" >/dev/null 2>&1 || return 0
        sleep 0.25
    done
    echo "  pid $holder will not die; try: kill -9 $holder" >&2
    return 1
}

wait_for_daemon() {
    for _ in $(seq 1 120); do
        curl -sf "$HEALTH" >/dev/null 2>&1 && return 0
        sleep 0.5
    done
    echo "daemon did not come up on $PORT" >&2
    return 1
}

case "$MODE" in
    daemon)
        build
        free_port || exit 1
        echo "▸ mogeungd ${ARGS[*]}"
        exec "$BIN/mogeungd" "${ARGS[@]}"
        ;;

    ui)
        # Waiting for health means also waiting for whatever build the daemon
        # process is doing — one dependency, not two.
        echo "▸ waiting for the daemon on $PORT…"
        wait_for_daemon || exit 1
        exec "$BIN/mogeung" --url "ws://127.0.0.1:$PORT/ws"
        ;;

    both)
        build
        free_port || exit 1

        DAEMON_PID=""
        UI_PID=""
        cleanup() {
            trap - INT TERM EXIT
            [ -n "$UI_PID" ] && kill "$UI_PID" 2>/dev/null
            [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null
            wait 2>/dev/null
            echo
            echo "▸ stopped"
        }
        trap cleanup INT TERM EXIT

        echo "▸ mogeungd ${ARGS[*]}"
        "$BIN/mogeungd" "${ARGS[@]}" &
        DAEMON_PID=$!

        if ! wait_for_daemon; then
            exit 1
        fi
        [ -n "$DB" ] && echo "▸ database: $DB"
        echo "▸ web client: http://127.0.0.1:$PORT/"

        "$BIN/mogeung" --url "ws://127.0.0.1:$PORT/ws" &
        UI_PID=$!

        # Exit when *either* dies. A daemon that crashed leaves a window that
        # cannot reconnect, and a window you closed usually means you are done.
        while kill -0 "$DAEMON_PID" 2>/dev/null && kill -0 "$UI_PID" 2>/dev/null; do
            sleep 0.5
        done
        kill -0 "$DAEMON_PID" 2>/dev/null || echo "▸ daemon exited" >&2
        ;;
esac
