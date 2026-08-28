#!/usr/bin/env bash
# Build everything and install it onto this machine, in one go.
#
#   ./scripts/install.sh                 build and install the lot
#   ./scripts/install.sh --no-desktop    daemon and launchers only, no window
#   ./scripts/install.sh --no-build      install what is already built
#   ./scripts/install.sh --prefix DIR    put the binaries somewhere else
#   ./scripts/install.sh --uninstall     remove what a previous run installed
#
# Three things, in the order they matter:
#
#   1. **The retired shortcut goes.** An egui-era `mogeung.desktop` in
#      ~/.local/share/applications launches `~/.local/bin/mogeung`, a binary
#      nothing has installed since [ADR-0020](../docs/decisions/0020-the-egui-client-is-retired.md).
#      Left there it is a second entry named *mogeung* in the launcher that
#      does nothing when clicked, next to the real one — so it is cleared
#      first, before anything can add a working entry beside it.
#   2. **The daemon and the launchers** into $PREFIX (default ~/.local/bin, no
#      sudo). `mogeungd` is what watches; `yolomo` starts claude, `qwenmo`
#      starts qwen and `codexmo` starts codex, each under tmux so mogeung can
#      host it in a pane rather than only point at it (ADR-0010).
#   3. **The window**, as a `.deb` installed with `dpkg -i`. That is the whole
#      reason this script grew: the Tauri bundler already produces a package
#      that carries the icon and the desktop entry properly, and the last step
#      — actually installing it — was the one thing left to do by hand.
#
# This script used to stop after step 2 and print a note telling you to run the
# bundler yourself. It does not any more, which makes the default run **slow**:
# `npm run tauri build` compiles the shell *and* the daemon (a path dependency)
# in release. `--no-desktop` is the old behaviour when that is not what you
# want, and `--no-build` installs whatever is already sitting in `target/`.
#
# **`sudo` is needed for step 3 and only step 3**, and there are two ways to
# give it. Either works and neither prompts in the middle of the builds:
#
#   sudo ./scripts/install.sh    # sudo asks once, before anything runs
#   ./scripts/install.sh         # the script asks once, up front, then keeps
#                                # the credential warm across the long builds
#
# Running the whole thing under `sudo` needs care, which is why it is handled
# here rather than left to chance: as root, `$HOME` is `/root`, so a naive run
# would build against root's cargo and npm caches and install the launchers
# into `/root/.local/bin` — the one place they are no use to anybody. So when
# invoked through `sudo`, everything except `dpkg` is run back as the
# **invoking** user, and their `$HOME` is what `--prefix` defaults from.
#
# On macOS step 3 builds the bundle and stops: the artefact is a `.app` and a
# `.dmg`, and where those go is a decision this script does not get to make.
#
# bash 3.2, which is what macOS ships. No `mapfile`, and no expanding an empty
# array under `set -u`.

set -uo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

# ── who is this actually being installed for? ────────────────────────────────
#
# `RUN_AS` is set only when we are root *because of sudo*. A real root login
# (no `SUDO_USER`) is left alone: that user means /root, and second-guessing
# them would be worse than doing as asked.
RUN_AS=""
if [ "$(id -u)" -eq 0 ] && [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    RUN_AS="$SUDO_USER"
    # `eval echo ~user` rather than `getent`, which macOS does not have.
    HOME="$(eval echo "~$SUDO_USER")"
    export HOME
fi

# Run a build step as the invoking user, or directly when there is none.
#
# `-i` and not a bare `-u`: sudo resets `PATH` to `secure_path`, which on most
# machines contains neither `~/.cargo/bin` nor whatever `node` a version
# manager put on the path — so without a login shell the builds fail to find
# their own toolchain. The `cd` is explicit because `-i` starts in `$HOME`.
as_user() {
    if [ -n "$RUN_AS" ]; then
        sudo -u "$RUN_AS" -i sh -c "cd '$ROOT' && $1"
    else
        sh -c "$1"
    fi
}

# Give a file we installed as root back to the person it is for. Without this
# the launchers end up root-owned in a user's ~/.local/bin, and the next run
# *without* sudo cannot overwrite them.
give_back() {
    [ -n "$RUN_AS" ] || return 0
    chown "$RUN_AS" "$@" 2>/dev/null
    return 0
}

PREFIX="$HOME/.local/bin"
BUILD=1
DESKTOP=1
UNINSTALL=0
# `deb` and not `all`: the AppImage is ~90 MB, takes the longest of the three,
# and needs the network on a cold cache to fetch `linuxdeploy`. None of that
# earns its place in a script whose job is *install it here*. Pass
# `--bundles all` when you are making something to hand to someone else.
BUNDLES="deb"

# The complete list of what this script installs into $PREFIX.
INSTALLABLES="mogeungd yolomo qwenmo codexmo"

# What --uninstall removes: what it installs, plus the retired window it used
# to. Left in the sweep deliberately — a machine that ran the old script has a
# `mogeung` binary this one will never overwrite, and forgetting it here is how
# a stale binary outlives the code that built it.
REMOVABLES="$INSTALLABLES mogeung"

# The Debian package's own name, from `productName` in tauri.conf.json. Used to
# remove it; installing goes by the file the bundler wrote.
PACKAGE="mogeung"

# Linux desktop integration (icon + launcher) that earlier versions installed
# for the egui window. Nothing writes these now; both install and --uninstall
# clear them.
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_FILE="$DATA_DIR/applications/mogeung.desktop"
ICON_FILE="$DATA_DIR/icons/hicolor/512x512/apps/mogeung.png"

DEB_DIR="desktop/src-tauri/target/release/bundle/deb"

# Nudge the desktop environment to notice a changed entry or icon. Both tools
# are optional and their absence is fine — caches rebuild on login anyway.
refresh_desktop_caches() {
    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$DATA_DIR/applications" 2>/dev/null
    command -v gtk-update-icon-cache >/dev/null 2>&1 &&
        gtk-update-icon-cache -q "$DATA_DIR/icons/hicolor" 2>/dev/null
    return 0
}

# The retired egui launcher, gone. Quiet when there is nothing to remove, so a
# clean machine does not report work it did not do.
remove_stale_shortcut() {
    local removed=0
    for file in "$DESKTOP_FILE" "$ICON_FILE"; do
        if [ -e "$file" ]; then
            rm "$file" || return 1
            echo "▸ removed the retired launcher $file"
            removed=1
        fi
    done
    [ "$removed" -eq 1 ] && refresh_desktop_caches
    return 0
}

# `sudo`, or nothing when already root. Echoed rather than run, so callers read
# as `$SUDO dpkg …` and work in both cases.
SUDO=""
need_sudo() {
    [ "$(id -u)" -eq 0 ] && return 0
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
        return 0
    fi
    echo "installing the window needs root and sudo is not here." >&2
    echo "  run this as root, or pass --no-desktop." >&2
    return 1
}

# The pid of the keep-alive below, so it can be stopped. Empty when there is
# none, which is the case whenever the script is already root.
SUDO_KEEPALIVE=""
stop_keepalive() {
    [ -n "$SUDO_KEEPALIVE" ] && kill "$SUDO_KEEPALIVE" 2>/dev/null
    SUDO_KEEPALIVE=""
    return 0
}
trap stop_keepalive EXIT

# Authenticate **before** the builds, not after them.
#
# The builds take minutes, and a password prompt that arrives at the end of
# them is the worst possible moment: you have walked away, and the run you came
# back to has been sitting idle rather than finishing. Asking first costs a
# prompt on a run that might have been `--no-desktop` anyway, which is a much
# smaller price.
#
# `sudo -v` caches the credential, and its timestamp expires (typically 5
# minutes) long before a cold release build finishes — so a background loop
# refreshes it while we work, and the `EXIT` trap above stops that loop
# whatever happens. Nothing is elevated by this: `-v` only extends a
# credential the user just typed.
authenticate_early() {
    need_sudo || return 1
    [ -z "$SUDO" ] && return 0
    echo "▸ this needs root at the end, to install the .deb — asking now so the"
    echo "  builds are not interrupted by it later."
    if ! sudo -v; then
        echo >&2
        echo "could not authenticate. Run this from a terminal, or use" >&2
        echo "  sudo ./scripts/install.sh" >&2
        echo "which asks once before anything runs." >&2
        return 1
    fi
    while true; do
        sudo -n true 2>/dev/null
        sleep 45
        kill -0 "$$" 2>/dev/null || exit
    done &
    SUDO_KEEPALIVE=$!
    return 0
}

# Print the header comment, stopping at the first line that is not one.
usage() {
    awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
    cat <<'EOF'

Options:
  --prefix DIR       install directory for the binaries (default ~/.local/bin)
  --no-build         skip the builds; install whatever is already built
  --no-desktop       skip the window entirely — daemon and launchers only
  --bundles LIST     what the Tauri bundler makes (default deb; try all)
  --uninstall        remove everything a previous run installed
  -h, --help         this
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)     PREFIX="${2:?--prefix needs a value}"; shift ;;
        --no-build)   BUILD=0 ;;
        --no-desktop) DESKTOP=0 ;;
        --bundles)    BUNDLES="${2:?--bundles needs a value}"; shift ;;
        --uninstall)  UNINSTALL=1 ;;
        -h|--help)    usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

if [ "$UNINSTALL" -eq 1 ]; then
    for name in $REMOVABLES; do
        if [ -e "$PREFIX/$name" ]; then
            rm "$PREFIX/$name" || exit 1
            echo "▸ removed $PREFIX/$name"
        fi
    done
    remove_stale_shortcut || exit 1
    # The package too, when one is installed. `dpkg -s` rather than a glob over
    # the bundle directory: what matters is what this machine *has*, not what
    # happens to be lying in `target/`.
    if [ "$DESKTOP" -eq 1 ] && command -v dpkg >/dev/null 2>&1 &&
       dpkg -s "$PACKAGE" >/dev/null 2>&1; then
        need_sudo || exit 1
        $SUDO dpkg -r "$PACKAGE" || exit 1
        echo "▸ removed the $PACKAGE package"
    fi
    exit 0
fi

# Before anything slow. `--no-desktop` needs no root at all, so it is not asked.
if [ "$DESKTOP" -eq 1 ] && [ "$UNINSTALL" -eq 0 ]; then
    authenticate_early || exit 1
fi

# Step 1, and first on purpose: a dead entry sitting next to the real one is
# the confusing state, so clear it before step 3 can create the real one.
remove_stale_shortcut || exit 1

# ── the daemon and the launchers ─────────────────────────────────────────────

if [ "$BUILD" -eq 1 ]; then
    echo "▸ building the daemon (release)…"
    as_user "cargo build --release" || exit 1
fi

if [ ! -x "target/release/mogeungd" ]; then
    echo "target/release/mogeungd is missing — build first (or drop --no-build)" >&2
    exit 1
fi

mkdir -p "$PREFIX" || exit 1

# `install` rather than `cp`: it replaces a running binary atomically enough
# that an already-running daemon keeps its old image instead of crashing on a
# half-written file.
install -m 755 target/release/mogeungd "$PREFIX/mogeungd" || exit 1
install -m 755 scripts/yolomo          "$PREFIX/yolomo"   || exit 1
install -m 755 scripts/qwenmo          "$PREFIX/qwenmo"   || exit 1
install -m 755 scripts/codexmo         "$PREFIX/codexmo"  || exit 1

for name in $INSTALLABLES; do
    give_back "$PREFIX/$name"
    echo "▸ installed $PREFIX/$name"
done

# ── the window ───────────────────────────────────────────────────────────────

if [ "$DESKTOP" -eq 1 ]; then
    if [ "$BUILD" -eq 1 ]; then
        # Only when it is absent. `npm install` on an up-to-date tree is not
        # free, and this script is already the slow one.
        if [ ! -d "desktop/node_modules" ]; then
            echo "▸ installing the window's dependencies…"
            as_user "cd desktop && npm install" || exit 1
        fi
        echo "▸ building the window (release) — this compiles the daemon again, so it is minutes…"
        as_user "cd desktop && npm run tauri build -- --bundles '$BUNDLES'" || exit 1
    fi

    if [ "$(uname -s)" != "Linux" ]; then
        echo
        echo "▸ built, and stopping here: this is $(uname -s), where the bundle is a .app/.dmg"
        echo "  under desktop/src-tauri/target/release/bundle/ — drag it where you want it."
        exit 0
    fi

    # Newest first, so a rebuild that bumped `version` in tauri.conf.json
    # installs the one just made rather than whichever sorts last.
    DEB="$(ls -t "$DEB_DIR"/*.deb 2>/dev/null | head -1)"
    if [ -z "$DEB" ]; then
        echo "no .deb in $DEB_DIR — build first (or drop --no-build)" >&2
        exit 1
    fi

    need_sudo || exit 1
    echo "▸ installing $DEB (needs root)…"
    # No prompt here: `authenticate_early` asked before the builds and has been
    # keeping the credential warm ever since, or we are already root because
    # this was run under sudo. Either way the password moment is behind us.
    if ! $SUDO dpkg -i "$DEB"; then
        echo >&2
        echo "dpkg refused it. If it named missing dependencies, this fixes them:" >&2
        echo "  sudo apt-get -f install" >&2
        exit 1
    fi

    # dpkg's own triggers usually do this; doing it again costs a moment and
    # covers the desktops where they do not.
    command -v update-desktop-database >/dev/null 2>&1 &&
        $SUDO update-desktop-database /usr/share/applications 2>/dev/null

    echo "▸ installed — 'mogeung' is in the launcher, and /usr/bin/mogeung-desktop"
fi

# A successful install to a directory the shell will never look in is the most
# confusing possible outcome, so check.
case ":$PATH:" in
    *":$PREFIX:"*) ;;
    *)
        echo
        echo "note: $PREFIX is not on your PATH. Add it, e.g.:" >&2
        echo "  export PATH=\"$PREFIX:\$PATH\"" >&2
        ;;
esac
