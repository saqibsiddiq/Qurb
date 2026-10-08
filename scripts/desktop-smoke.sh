#!/usr/bin/env bash
# Drive the real desktop window, end to end, without touching your desktop.
#
#   ./scripts/desktop-smoke.sh                 # builds qurb and qurb-desktop first
#   ./scripts/desktop-smoke.sh --no-build
#
# Sets a device up through the window, opens every place, adds a second device
# by the code the window shows, and sends it a file -- through the real
# application, its real commands and the real engine. Fails if any command the
# window calls returns an error. See scripts/desktop_smoke.py for the steps.
#
# Why this exists: the fixture page (experiments/desktop-fixtures) answers the
# window's commands from made-up data, so it cannot see a command that fails
# in the application itself. "Show a code" failed that way every time for five
# days -- a synchronous command opening a QUIC endpoint outside the Tokio
# runtime -- while the fixture showed a working screen.
#
# How, without synthetic input (which does not reach a window under Wayland):
# Tauri lets WebKit's own WebDriver control its web view when
# TAURI_WEBVIEW_AUTOMATION=true, and GTK's Broadway backend gives the window a
# display of its own that is not on anybody's screen. Everything else is
# isolated too: its own home, config, runtime directory -- so it neither finds
# your running qurb nor writes to your folder.
#
# Needs broadwayd (GTK 3), WebKitWebDriver (webkit2gtk-4.1) and python3.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

for tool in broadwayd WebKitWebDriver python3; do
    command -v "$tool" >/dev/null || { echo "desktop-smoke: needs $tool" >&2; exit 2; }
done

if [[ ${1:-} != --no-build ]]; then
    cargo build --release -p qurb-cli -p qurb-desktop
fi

WORK=$(mktemp -d "$ROOT/target/desktop-smoke.XXXXXX")
mkdir -p "$WORK/run" && chmod 700 "$WORK/run"
pids=()
cleanup() {
    # The application normally leaves when the WebDriver session ends. If it
    # did not, it leaves when its display does: a GTK application exits when
    # its connection to the display closes.
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT

# A display number and a port nobody else is using.
display=$((20 + RANDOM % 60))
port=$((20000 + RANDOM % 20000))

export HOME=$WORK XDG_CONFIG_HOME=$WORK/.config XDG_DATA_HOME=$WORK/.local/share \
       XDG_CACHE_HOME=$WORK/.cache XDG_RUNTIME_DIR=$WORK/run
unset WAYLAND_DISPLAY DISPLAY
export GDK_BACKEND=broadway BROADWAY_DISPLAY=:$display
export WEBKIT_DISABLE_COMPOSITING_MODE=1 TAURI_WEBVIEW_AUTOMATION=true
# Off your session bus, so nothing the test does can reach your real keyring
# or notifications; and no accessibility bridge to look for on it.
export DBUS_SESSION_BUS_ADDRESS=disabled: NO_AT_BRIDGE=1

# SMOKE_MODE=theme needs a bus, for the desktop's dark preference: a bus of
# its own, with nothing on it but a stand-in for the settings portal
# (scripts/smoke_portal.py), and no services it could start by being asked.
if [[ ${SMOKE_MODE:-} == theme ]]; then
    cat >"$WORK/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path=$WORK/run/bus</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
    dbus-daemon --config-file="$WORK/bus.conf" --nofork >"$WORK/bus.log" 2>&1 & pids+=($!)
    export DBUS_SESSION_BUS_ADDRESS=unix:path=$WORK/run/bus
    for _ in $(seq 50); do [[ -S $WORK/run/bus ]] && break; sleep 0.1; done
    python3 "$ROOT/scripts/smoke_portal.py" 1 >"$WORK/portal.log" 2>&1 & pids+=($!)
    for _ in $(seq 50); do
        gdbus call --session --dest org.freedesktop.portal.Desktop \
            --object-path /org/freedesktop/portal/desktop \
            --method org.freedesktop.portal.Settings.Read \
            org.freedesktop.appearance color-scheme >/dev/null 2>&1 && break
        sleep 0.1
    done
fi

broadwayd ":$display" >"$WORK/broadway.log" 2>&1 & pids+=($!)
WebKitWebDriver --port="$port" >"$WORK/webdriver.log" 2>&1 & pids+=($!)
sleep 1

export SMOKE_DRIVER=http://127.0.0.1:$port
export SMOKE_APP=${SMOKE_APP:-$ROOT/target/release/qurb-desktop}
export SMOKE_QURB=$ROOT/target/release/qurb

if python3 "$ROOT/scripts/desktop_smoke.py"; then
    rm -rf "$WORK"
else
    echo "desktop-smoke: failed; what it left is in $WORK" >&2
    exit 1
fi
