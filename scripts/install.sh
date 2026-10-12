#!/usr/bin/env bash
# Build everything and install it onto this machine, in one go.
#
#   ./scripts/install.sh                 build and install the lot
#   ./scripts/install.sh --no-desktop    daemon and launchers only, no window
#   ./scripts/install.sh --no-build      install what is already built
#   ./scripts/install.sh --prefix DIR    put the binaries somewhere else
#   ./scripts/install.sh --uninstall     remove what a previous run installed
#   ./scripts/install.sh --build-only    build everything, install nothing
#   ./scripts/install.sh --elevate pkexec   ask for root in the desktop's own
#                                           dialog rather than in a terminal
#   ./scripts/install.sh --install-app   macOS: copy the app into /Applications
#
# **The last three exist for the window's own updates** (`R-J105`, feature
# 0049), and each is useful by hand too. `--build-only` needs no root at all.
# `--elevate pkexec` runs the one root step, `dpkg -i`, through polkit, so the
# password is typed into the operating system's dialog — which names the
# command it is for — and never into anything of mogeung's; the window has no
# terminal to type into, and it renders text it did not write, so it is the
# last place a password should pass through. On macOS nothing needs root, and
# `--install-app` does the copy this script otherwise prints and leaves to you.
#
# A successful install records what it installed in `~/.mogeung/install.json`:
# the commit, the checkout it came from, the time. That is how an installed
# window knows where its source is without being told.
#
# Exit status: 0 done · 1 failed · 2 bad arguments · 3 the password dialog was
# dismissed · 4 not authorised — so a caller can tell *you said no* from *it
# broke*.
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
#      sudo). `mogeungd` is what watches; `yolomo` starts claude, `yolomop`
#      starts the same claude session through `llmproxy --claude`, `qwenmo`
#      starts qwen, `codexmo` starts codex and `opencodemo` starts opencode,
#      each under tmux so mogeung can host it in a pane rather than only point
#      at it (ADR-0010).
#   3. **The window**. On Linux that is a `.deb` installed with `dpkg -i`, which
#      is the whole reason this script grew: the Tauri bundler already produces
#      a package that carries the icon and the desktop entry properly, and the
#      last step — actually installing it — was the one thing left to do by
#      hand. On macOS it is a `.dmg`, and the script stops once it is built.
#
# **The platform is detected, not asked for**, because every difference below
# follows from it and a wrong guess is a slow failure: `--bundles deb` on macOS
# builds for minutes and then produces nothing usable. `uname -s` decides the
# default bundle (`deb` on Linux, `dmg` on macOS), whether root is needed at
# all, and what the last line tells you to do next. `--bundles` still wins when
# you pass it.
#
# This script used to stop after step 2 and print a note telling you to run the
# bundler yourself. It does not any more, which makes the default run **slow**:
# `npm run tauri build` compiles the shell *and* the daemon (a path dependency)
# in release — which is why, since 2026-10-12, the daemon is built **there and
# only there** when there is a window to build, and the root workspace is not
# compiled at all (see *the builds*, below). `--no-desktop` is the old
# behaviour when that is not what you want, and `--no-build` installs whatever
# is already sitting in the target directory the same rule names.
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
# `.dmg`, and where those go is a decision this script does not get to make. It
# names both paths rather than the directory, so the next step is a command you
# can copy rather than a hunt. **No password is asked for there**: `sudo` is for
# `dpkg` and macOS has no `dpkg`, so a prompt on that platform would be asking
# for root to do nothing with it.
#
# bash 3.2, which is what macOS ships. No `mapfile`, and no expanding an empty
# array under `set -u`.

set -uo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

# ── which machine is this? ───────────────────────────────────────────────────
#
# Decided once, here, and every platform question below asks this variable
# rather than running `uname` again — so the bundle default, the root check and
# the closing message cannot come to different conclusions about where they are.
#
# Anything that is neither is treated as Linux: it is the only other platform
# this project supports (Windows is descoped, see `R-I3`), and a default that
# builds the wrong package is a better failure than one that refuses to try.
case "$(uname -s)" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    *)      PLATFORM="linux" ;;
esac

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
BUILD_ONLY=0
INSTALL_APP=0
# How the one root step is elevated: `sudo`, asked up front in a terminal, or
# `pkexec`, the desktop's own dialog, asked at the moment it is needed.
ELEVATE="sudo"
# The one bundle this machine can actually use, and no more.
#
# On Linux, `deb` and not `all`: the AppImage is ~90 MB, takes the longest of
# the three, and needs the network on a cold cache to fetch `linuxdeploy`. None
# of that earns its place in a script whose job is *install it here*.
#
# On macOS, `dmg` — asked for 2026-09-06, and the reason is that the previous
# hardcoded `deb` was not merely useless there but *expensively* useless: the
# window and the daemon both compile in release first, minutes of work, and
# only then does the bundler find it has been asked for a Debian package on a
# Mac. Since the platform is already known, nobody should have to pass this.
#
# Pass `--bundles all` when you are making something to hand to someone else.
if [ "$PLATFORM" = "macos" ]; then
    BUNDLES="dmg"
else
    BUNDLES="deb"
fi

# The complete list of what this script installs into $PREFIX.
INSTALLABLES="mogeungd yolomo yolomop qwenmo codexmo opencodemo"

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
APP_DIR="desktop/src-tauri/target/release/bundle/macos"
DMG_DIR="desktop/src-tauri/target/release/bundle/dmg"

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
    if [ "$ELEVATE" = "pkexec" ]; then
        # An absolute path, because pkexec runs its program with a sanitised
        # environment and says so when it cannot find one.
        if command -v pkexec >/dev/null 2>&1; then
            SUDO="$(command -v pkexec)"
            return 0
        fi
        echo "installing the window needs root and pkexec is not here." >&2
        echo "  run ./scripts/install.sh from a terminal instead." >&2
        return 1
    fi
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
  --bundles LIST     what the Tauri bundler makes (default: dmg on macOS,
                     deb on Linux; try all)
  --uninstall        remove everything a previous run installed
  --build-only       build everything and install nothing (needs no root)
  --elevate WHICH    sudo (default: asked once, up front, in the terminal) or
                     pkexec (the desktop's own dialog, when it is needed)
  --install-app      macOS: copy the built app into /Applications
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
        --build-only) BUILD_ONLY=1 ;;
        --install-app) INSTALL_APP=1 ;;
        --elevate)
            ELEVATE="${2:?--elevate needs sudo or pkexec}"; shift
            case "$ELEVATE" in
                sudo|pkexec) ;;
                *) echo "--elevate takes sudo or pkexec, not $ELEVATE" >&2; exit 2 ;;
            esac
            ;;
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

# Before anything slow. `--no-desktop` needs no root at all, so it is not
# asked — and neither is macOS, where the only thing root was ever for is
# `dpkg`. Asking there would be a password prompt collected for nothing, which
# is worse than an unnecessary one: it teaches you to type it without reading.
#
# Nor with `--build-only`, which installs nothing, nor with `--elevate pkexec`,
# whose dialog is asked at the moment of `dpkg` and needs no warm credential.
if [ "$DESKTOP" -eq 1 ] && [ "$UNINSTALL" -eq 0 ] && [ "$PLATFORM" = "linux" ] &&
   [ "$BUILD_ONLY" -eq 0 ] && [ "$ELEVATE" = "sudo" ]; then
    authenticate_early || exit 1
fi

# What this checkout is, written after a successful install. `--git-common-dir`
# rather than `--show-toplevel`, so an install from a linked worktree — which
# is how the window builds its updates — records the checkout the worktree
# belongs to, the one whose branch an update follows.
write_install_record() {
    local window="$1"
    local commit source branch dir tmp
    commit="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
    source="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
    case "$source" in
        */.git) source="${source%/.git}" ;;
        *) source="$ROOT" ;;
    esac
    branch="$(git -C "$source" symbolic-ref -q --short HEAD 2>/dev/null || true)"
    dir="$HOME/.mogeung"
    mkdir -p "$dir" || return 1
    give_back "$dir"
    tmp="$dir/install.json.tmp"
    # Paths are the only strings here that could hold a quote or a backslash.
    esc() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
    printf '{"commit":"%s","source":"%s","branch":"%s","at":"%s","platform":"%s","window":%s}\n' \
        "$commit" "$(esc "$source")" "$(esc "$branch")" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$PLATFORM" "$window" \
        >"$tmp" && mv "$tmp" "$dir/install.json" || return 1
    give_back "$dir/install.json"
    echo "▸ recorded ${commit:0:7} from $source in $dir/install.json"
}

# Step 1, and first on purpose: a dead entry sitting next to the real one is
# the confusing state, so clear it before step 3 can create the real one. Not
# for `--build-only`, which touches nothing outside this checkout.
if [ "$BUILD_ONLY" -eq 0 ]; then
    remove_stale_shortcut || exit 1
fi

# ── the builds ───────────────────────────────────────────────────────────────
#
# **The daemon is compiled once.** `npm run tauri build` compiles `mogeungd`
# as the shell's path dependency — the library the window hosts in-process
# (ADR-0009) — in the shell's own workspace under `desktop/src-tauri/target`,
# and the standalone binary is one more link away in that same workspace.
# Until 2026-10-12 this script built it at the root first, `cargo build
# --release`, and then watched the bundler compile the identical crate again:
# 57 s and then 44 s on a 16-core box, for one daemon. So with a window to
# build there is no root build at all, and the installed `mogeungd` is the one
# resolved by the shell's lockfile — which is the daemon the window runs
# anyway. Without a window (`--no-desktop`) the root build is the only one and
# the binary comes from `target/release` as it always did. `--no-build`
# installs whatever is already sitting in whichever of the two the same rule
# names — the window's own update builds with `--build-only` and installs with
# `--no-build`, both with a window, so both look in the same place.
SHELL_MANIFEST="desktop/src-tauri/Cargo.toml"
if [ "$DESKTOP" -eq 1 ]; then
    DAEMON_BIN="desktop/src-tauri/target/release/mogeungd"
else
    DAEMON_BIN="target/release/mogeungd"
fi

if [ "$BUILD" -eq 1 ]; then
    if [ "$DESKTOP" -eq 1 ]; then
        # Only when it is absent. `npm install` on an up-to-date tree is not
        # free, and this script is already the slow one.
        if [ ! -d "desktop/node_modules" ]; then
            echo "▸ installing the window's dependencies…"
            as_user "cd desktop && npm install" || exit 1
        fi
        echo "▸ building the window (release) — the daemon compiles with it, so it is minutes…"
        as_user "cd desktop && npm run tauri build -- --bundles '$BUNDLES'" || exit 1
        # `-p mogeungd` names a package outside the shell's workspace, which
        # cargo allows for a path dependency: it reuses the library the bundler
        # just compiled and links the binary, seconds rather than a minute. No
        # `--features` — cargo refuses them for an outside package, and none
        # is needed, since the shell's `custom-protocol` does not reach the
        # daemon's dependencies.
        echo "▸ linking the daemon binary from the window's workspace…"
        as_user "cargo build --release --manifest-path '$SHELL_MANIFEST' -p mogeungd --bin mogeungd" || exit 1
    else
        echo "▸ building the daemon (release)…"
        as_user "cargo build --release" || exit 1
    fi
fi

if [ ! -x "$DAEMON_BIN" ]; then
    echo "$DAEMON_BIN is missing — build first (or drop --no-build)" >&2
    exit 1
fi

if [ "$BUILD_ONLY" -eq 1 ]; then
    echo "▸ built, and nothing installed (--build-only)."
    exit 0
fi

# ── the daemon and the launchers ─────────────────────────────────────────────

mkdir -p "$PREFIX" || exit 1

# `install` rather than `cp`: it replaces a running binary atomically enough
# that an already-running daemon keeps its old image instead of crashing on a
# half-written file.
install -m 755 "$DAEMON_BIN"           "$PREFIX/mogeungd" || exit 1
install -m 755 scripts/yolomo          "$PREFIX/yolomo"   || exit 1
install -m 755 scripts/yolomop         "$PREFIX/yolomop"  || exit 1
install -m 755 scripts/qwenmo          "$PREFIX/qwenmo"   || exit 1
install -m 755 scripts/codexmo         "$PREFIX/codexmo"  || exit 1
install -m 755 scripts/opencodemo      "$PREFIX/opencodemo" || exit 1

for name in $INSTALLABLES; do
    give_back "$PREFIX/$name"
    echo "▸ installed $PREFIX/$name"
done

# ── the window ───────────────────────────────────────────────────────────────

if [ "$DESKTOP" -eq 1 ]; then
    if [ "$PLATFORM" != "linux" ]; then
        # Newest first, for the same reason the .deb below is: a rebuild that
        # bumped `version` must not be reported as whichever name sorts last.
        APP="$(ls -td "$APP_DIR"/*.app 2>/dev/null | head -1)"
        DMG="$(ls -t "$DMG_DIR"/*.dmg 2>/dev/null | head -1)"
        if [ "$INSTALL_APP" -eq 1 ]; then
            if [ -z "$APP" ]; then
                echo "no .app in $APP_DIR — build first (or drop --no-build)" >&2
                exit 1
            fi
            # Copied beside the old one and swapped in, so a copy that fails
            # halfway leaves the installed app as it was rather than half of
            # each. The running app keeps the files it has open; the next
            # launch is the new one.
            dest="/Applications/$(basename "$APP")"
            if ! cp -R "$APP" "$dest.new"; then
                echo "could not copy into /Applications — is it writable by you?" >&2
                exit 1
            fi
            rm -rf "$dest" && mv "$dest.new" "$dest" || exit 1
            echo "▸ installed $dest"
            write_install_record true || exit 1
            exit 0
        fi
        echo
        echo "▸ built, and stopping here: on macOS where the bundle goes is your call."
        [ -n "$DMG" ] && echo "  disk image: $DMG"
        if [ -n "$APP" ]; then
            echo "  app:        $APP"
            echo
            echo "  to install it over the running one:"
            echo "    rm -rf /Applications/mogeung.app && cp -R '$APP' /Applications/"
        fi
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
    # No prompt here with sudo: `authenticate_early` asked before the builds
    # and has been keeping the credential warm ever since, or we are already
    # root because this was run under sudo. With pkexec this *is* the prompt —
    # the desktop's dialog, naming `dpkg` — and the paths are absolute because
    # pkexec does not run its program where we are standing.
    if [ "$ELEVATE" = "pkexec" ] && [ -n "$SUDO" ]; then
        $SUDO "$(command -v dpkg)" -i "$ROOT/$DEB"
        status=$?
        case "$status" in
            0) ;;
            126) echo "the password dialog was dismissed — nothing was installed." >&2; exit 3 ;;
            127) echo "not authorised to install — nothing was installed." >&2; exit 4 ;;
            *) echo "dpkg refused it (exit $status). If it named missing dependencies:" >&2
               echo "  sudo apt-get -f install" >&2
               exit 1 ;;
        esac
    elif ! $SUDO dpkg -i "$DEB"; then
        echo >&2
        echo "dpkg refused it. If it named missing dependencies, this fixes them:" >&2
        echo "  sudo apt-get -f install" >&2
        exit 1
    fi

    # dpkg's own triggers usually do this; doing it again costs a moment and
    # covers the desktops where they do not. Not through pkexec, where it
    # would be a second password dialog for something the triggers did.
    if [ "$ELEVATE" = "sudo" ]; then
        command -v update-desktop-database >/dev/null 2>&1 &&
            $SUDO update-desktop-database /usr/share/applications 2>/dev/null
    fi
    write_install_record true || exit 1

    echo "▸ installed — 'mogeung' is in the launcher, and /usr/bin/mogeung-desktop"
fi

# Daemon and launchers only: still an install, and still worth recording.
if [ "$DESKTOP" -eq 0 ] && [ "$BUILD_ONLY" -eq 0 ]; then
    write_install_record false || exit 1
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
