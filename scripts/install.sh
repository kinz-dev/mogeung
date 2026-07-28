#!/usr/bin/env bash
# Build the release binaries and install everything onto this machine.
#
#   ./scripts/install.sh                 install to ~/.local/bin
#   ./scripts/install.sh --prefix DIR    install somewhere else
#   ./scripts/install.sh --uninstall     remove what a previous run installed
#
# Installs three things: the daemon (mogeungd), the window (mogeung), and the
# yolomo helper that starts claude under tmux so mogeung can host it in a pane.
# On Linux it also installs a desktop entry and icon, so the window shows up
# properly in the dock — Wayland compositors ignore the icon a program sets on
# itself and only honour a desktop entry matching the window's app id.
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

# The complete list of what this script owns in $PREFIX. --uninstall removes
# exactly these and nothing else.
INSTALLABLES="mogeungd mogeung yolomo"

# Linux desktop integration (icon + launcher). Also owned by this script.
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
    for name in $INSTALLABLES; do
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

for bin in mogeungd mogeung; do
    if [ ! -x "target/release/$bin" ]; then
        echo "target/release/$bin is missing — build first (or drop --no-build)" >&2
        exit 1
    fi
done

mkdir -p "$PREFIX" || exit 1

# `install` rather than `cp`: it replaces a running binary atomically enough
# that an already-running daemon keeps its old image instead of crashing on a
# half-written file.
install -m 755 target/release/mogeungd "$PREFIX/mogeungd" || exit 1
install -m 755 target/release/mogeung  "$PREFIX/mogeung"  || exit 1
install -m 755 scripts/yolomo          "$PREFIX/yolomo"   || exit 1

for name in $INSTALLABLES; do
    echo "▸ installed $PREFIX/$name"
done

# Desktop entry and icon, Linux only. Exec is rewritten to the absolute path
# because a desktop launch does not necessarily share the shell's PATH.
if [ "$(uname)" = Linux ]; then
    mkdir -p "$DATA_DIR/applications" "$(dirname "$ICON_FILE")" || exit 1
    sed "s|^Exec=.*|Exec=$PREFIX/mogeung|" crates/mogeung-ui/assets/mogeung.desktop \
        > "$DESKTOP_FILE" || exit 1
    chmod 644 "$DESKTOP_FILE"
    install -m 644 crates/mogeung-ui/assets/mogeung.png "$ICON_FILE" || exit 1
    refresh_desktop_caches
    echo "▸ installed $DESKTOP_FILE"
    echo "▸ installed $ICON_FILE"
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
