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
#      sudo). `mogeungd` is what watches; `yolomo` starts claude and `qwenmo`
#      starts qwen, each under tmux so mogeung can host it in a pane rather
#      than only point at it (ADR-0010).
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
# **`sudo` is needed for step 3 and only step 3.** dpkg writes to /usr/bin, so
# the password prompt arrives after the builds rather than at the start; that
# is unavoidable without asking for a password you might not end up needing.
#
# On macOS step 3 builds the bundle and stops: the artefact is a `.app` and a
# `.dmg`, and where those go is a decision this script does not get to make.
#
# bash 3.2, which is what macOS ships. No `mapfile`, and no expanding an empty
# array under `set -u`.

set -uo pipefail
cd "$(dirname "$0")/.."

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
INSTALLABLES="mogeungd yolomo qwenmo"

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

# Step 1, and first on purpose: a dead entry sitting next to the real one is
# the confusing state, so clear it before step 3 can create the real one.
remove_stale_shortcut || exit 1

# ── the daemon and the launchers ─────────────────────────────────────────────

if [ "$BUILD" -eq 1 ]; then
    echo "▸ building the daemon (release)…"
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
install -m 755 scripts/qwenmo          "$PREFIX/qwenmo"   || exit 1

for name in $INSTALLABLES; do
    echo "▸ installed $PREFIX/$name"
done

# ── the window ───────────────────────────────────────────────────────────────

if [ "$DESKTOP" -eq 1 ]; then
    if [ "$BUILD" -eq 1 ]; then
        # Only when it is absent. `npm install` on an up-to-date tree is not
        # free, and this script is already the slow one.
        if [ ! -d "desktop/node_modules" ]; then
            echo "▸ installing the window's dependencies…"
            (cd desktop && npm install) || exit 1
        fi
        echo "▸ building the window (release) — this compiles the daemon again, so it is minutes…"
        (cd desktop && npm run tauri build -- --bundles "$BUNDLES") || exit 1
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
    # Authenticate *before* dpkg, so a password problem reports itself as one.
    # Without this the two failures print the same message, and the commonest
    # one — no terminal to type into, which is every non-interactive run — read
    # as a broken package.
    if [ -n "$SUDO" ] && ! $SUDO -v; then
        echo >&2
        echo "could not authenticate. Run this from a terminal, or install the" >&2
        echo "package yourself once you can:" >&2
        echo "  sudo dpkg -i $DEB" >&2
        exit 1
    fi
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
