#!/usr/bin/env bash
# Build the release binaries and install everything onto this machine.
#
#   ./scripts/install.sh                 install to ~/.local/bin
#   ./scripts/install.sh --prefix DIR    install somewhere else
#   ./scripts/install.sh --uninstall     remove what a previous run installed
#
# Installs two things: the daemon (mogeungd), and the yolomo helper that starts
# claude under tmux so mogeung can host it in a pane.
#
# The window is **not** installed from here any more. It is the Tauri client in
# desktop/ (ADR-0020 retired the egui one), and its own bundler produces a .deb,
# .rpm and an AppImage that carry the icon and the desktop entry properly:
#
#   cd desktop && npm install && npm run tauri build
#
# --uninstall still sweeps up the old `mogeung` binary, desktop entry and icon
# that earlier runs of this script installed, so upgrading does not leave a
# launcher pointing at a program that no longer exists.
#
# The default is ~/.local/bin because it needs no sudo. Pass
# `--prefix /usr/local/bin` (with sudo) for a system-wide install.
#
# bash 3.2, which is what macOS ships.

set -uo pipefail
cd "$(dirname "$0")/.."

PREFIX="$HOME/.local/bin"
BUILD=1
UNINSTALL=0

# The complete list of what this script installs into $PREFIX.
INSTALLABLES="mogeungd yolomo"

# What --uninstall removes: what it installs, plus the retired window it used
# to. Left in the sweep deliberately — a machine that ran the old script has a
# `mogeung` binary this one will never overwrite, and forgetting it here is how
# a stale binary outlives the code that built it.
REMOVABLES="$INSTALLABLES mogeung"

# Linux desktop integration (icon + launcher) that earlier versions installed
# for the egui window. Nothing writes these now; --uninstall still clears them.
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_FILE="$DATA_DIR/applications/mogeung.desktop"
ICON_FILE="$DATA_DIR/icons/hicolor/512x512/apps/mogeung.png"

# Nudge the desktop environment to notice a changed entry or icon. Both tools
# are optional and their absence is fine — caches rebuild on login anyway.
refresh_desktop_caches() {
    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$DATA_DIR/applications" 2>/dev/null
    command -v gtk-update-icon-cache >/dev/null 2>&1 &&
        gtk-update-icon-cache -q "$DATA_DIR/icons/hicolor" 2>/dev/null
    return 0
}

# Print the header comment, stopping at the first line that is not one.
usage() {
    awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
    cat <<'EOF'

Options:
  --prefix DIR       install directory (default ~/.local/bin)
  --no-build         skip cargo build; install whatever is already built
  --uninstall        remove the installed binaries instead
  -h, --help         this
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)    PREFIX="${2:?--prefix needs a value}"; shift ;;
        --no-build)  BUILD=0 ;;
        --uninstall) UNINSTALL=1 ;;
        -h|--help)   usage; exit 0 ;;
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
    for file in "$DESKTOP_FILE" "$ICON_FILE"; do
        if [ -e "$file" ]; then
            rm "$file" || exit 1
            echo "▸ removed $file"
        fi
    done
    refresh_desktop_caches
    exit 0
fi

if [ "$BUILD" -eq 1 ]; then
    echo "▸ building (release)…"
    cargo build --release || exit 1
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

for name in $INSTALLABLES; do
    echo "▸ installed $PREFIX/$name"
done

# A desktop entry left behind by an older install would launch a binary this
# script no longer puts there. Say so rather than deleting it silently: an
# --uninstall clears it, and doing that on an *install* run would be a surprise.
if [ -e "$DESKTOP_FILE" ]; then
    echo
    echo "note: $DESKTOP_FILE is from the retired egui window and launches" >&2
    echo "  a binary that is no longer installed. Clear it with --uninstall," >&2
    echo "  then build the Tauri window: (cd desktop && npm run tauri build)" >&2
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
