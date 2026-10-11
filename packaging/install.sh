#!/usr/bin/env bash
# Put qurb in the application menu, for this user.
#
#   ./packaging/install.sh            # install
#   ./packaging/install.sh --uninstall
#
# Deliberately per-user rather than system-wide: it needs no root, touches
# nothing outside $HOME, and `--uninstall` genuinely undoes it. A packaged
# build for distribution is a separate job -- see docs/phases/phase-4-product.md.
set -euo pipefail
cd "$(dirname "$0")/.."

APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"

if [[ ${1:-} == --uninstall ]]; then
    rm -f "$APPS/qurb.desktop" "$ICONS/qurb.svg" "$BIN/qurb-desktop" "$BIN/qurb-tray" "$BIN/qurb" \
          "$AUTOSTART/qurb.desktop"
    command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
    echo "Removed qurb from the application menu."
    exit 0
fi

for program in qurb qurb-tray qurb-desktop; do
    [[ -x target/release/$program ]] || {
        echo "Build first: cargo build --release   (target/release/$program is missing)" >&2
        exit 1
    }
done

mkdir -p "$APPS" "$ICONS" "$BIN"

# Copied rather than symlinked into the build directory: a menu entry that
# stops working after `cargo clean` is worse than one that is slightly stale.
# The menu opens the window (decision 0040); the tray and the terminal
# program come too, for whoever wants them by name.
install -m755 target/release/qurb-desktop "$BIN/qurb-desktop"
install -m755 target/release/qurb-tray "$BIN/qurb-tray"
install -m755 target/release/qurb "$BIN/qurb"
install -m644 packaging/qurb.svg "$ICONS/qurb.svg"
install -m644 packaging/qurb.desktop "$APPS/qurb.desktop"

# Started at login, with no window: a laptop that is not running cannot be
# reached by the phone. The same entry the window's Settings writes and
# removes; the same format as crates/desktop/src/autostart.rs.
mkdir -p "$AUTOSTART"
cat >"$AUTOSTART/qurb.desktop" <<ENTRY
[Desktop Entry]
Type=Application
Name=Qurb
Comment=Keeps your files in sync in the background
Exec=$BIN/qurb-desktop --hidden
Icon=qurb
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
ENTRY

command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
    gtk-update-icon-cache -f -t "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true

echo "Installed:"
echo "    $APPS/qurb.desktop"
echo "    $ICONS/qurb.svg"
echo "    $BIN/qurb-desktop, $BIN/qurb-tray  and  $BIN/qurb"
echo "    $AUTOSTART/qurb.desktop  — qurb starts when you log in; turn that off in its Settings"
echo

# The menu entry names the program by its full path, always. It must not
# depend on a PATH the desktop session may not share, and whether this shell's
# PATH has $BIN says nothing about the session's.
sed -i "s|^Exec=qurb-desktop$|Exec=$BIN/qurb-desktop|" "$APPS/qurb.desktop"

case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Note: $BIN is not on your PATH, so \`qurb\` will not work in a terminal."
       echo "      The menu entry uses the full path and works regardless."
       ;;
esac

echo "qurb should now be in your application menu."
