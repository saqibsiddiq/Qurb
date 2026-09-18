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
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"

if [[ ${1:-} == --uninstall ]]; then
    rm -f "$APPS/qurb.desktop" "$ICONS/qurb.svg" "$BIN/qurb-tray" "$BIN/qurb"
    command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
    echo "Removed qurb from the application menu."
    exit 0
fi

[[ -x target/release/qurb-tray ]] || {
    echo "Build first: cargo build --release" >&2
    exit 1
}

mkdir -p "$APPS" "$ICONS" "$BIN"

# Copied rather than symlinked into the build directory: a menu entry that
# stops working after `cargo clean` is worse than one that is slightly stale.
install -m755 target/release/qurb-tray "$BIN/qurb-tray"
install -m755 target/release/qurb "$BIN/qurb"
install -m644 packaging/qurb.svg "$ICONS/qurb.svg"
install -m644 packaging/qurb.desktop "$APPS/qurb.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
    gtk-update-icon-cache -f -t "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true

echo "Installed:"
echo "    $APPS/qurb.desktop"
echo "    $ICONS/qurb.svg"
echo "    $BIN/qurb-tray  and  $BIN/qurb"
echo

case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Note: $BIN is not on your PATH, so \`qurb\` will not work in a terminal."
       echo "      The menu entry uses the full path and works regardless."
       # The menu entry must not depend on a PATH the desktop may not share.
       sed -i "s|^Exec=qurb-tray$|Exec=$BIN/qurb-tray|" "$APPS/qurb.desktop"
       ;;
esac

echo "qurb should now be in your application menu."
