"""A stand-in for the desktop's settings portal, on the smoke test's own bus.

Run by scripts/desktop-smoke.sh with SMOKE_MODE=theme. The window learns the
desktop's dark preference from the portal: Tauri's toolkit, tao, reads
org.freedesktop.appearance color-scheme when the window opens and listens for
SettingChanged after, and WebKit turns what it learns into
prefers-color-scheme. On the GNOME desktop this was written on, the real
portal answers 1 for a desktop set to dark.

The smoke test cannot use the real one: on your session bus, the window would
find your keyring, your notifications and anything else you have running. So
this answers the one setting, starting at the value it is given -- 1 dark, 2
light, 0 no preference -- and `Set(u)` on org.qurb.SmokePortal changes it and
says so, as a desktop does when its style is switched.
"""

import sys

from gi.repository import Gio, GLib

PATH = "/org/freedesktop/portal/desktop"
SETTINGS = "org.freedesktop.portal.Settings"
NODE = Gio.DBusNodeInfo.new_for_xml(f"""
<node>
  <interface name="{SETTINGS}">
    <method name="Read">
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="v" direction="out"/>
    </method>
    <method name="ReadOne">
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="v" direction="out"/>
    </method>
    <method name="ReadAll">
      <arg type="as" direction="in"/><arg type="a{{sa{{sv}}}}" direction="out"/>
    </method>
    <signal name="SettingChanged">
      <arg type="s"/><arg type="s"/><arg type="v"/>
    </signal>
  </interface>
  <interface name="org.qurb.SmokePortal">
    <method name="Set"><arg type="u" direction="in"/></method>
  </interface>
</node>""")

scheme = int(sys.argv[1]) if len(sys.argv) > 1 else 0


def called(connection, sender, path, interface, method, args, invocation):
    global scheme
    if interface == "org.qurb.SmokePortal":
        scheme = args.unpack()[0]
        connection.emit_signal(None, PATH, SETTINGS, "SettingChanged", GLib.Variant(
            "(ssv)", ("org.freedesktop.appearance", "color-scheme", GLib.Variant("u", scheme))))
        invocation.return_value(None)
        return
    if method == "ReadAll":
        invocation.return_value(GLib.Variant("(a{sa{sv}})", (
            {"org.freedesktop.appearance": {"color-scheme": GLib.Variant("u", scheme)}},)))
        return
    namespace, key = args.unpack()
    if (namespace, key) != ("org.freedesktop.appearance", "color-scheme"):
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound",
                                     f"no {namespace} {key} here")
        return
    value = GLib.Variant("u", scheme)
    # Read, the first version of the call, wraps the value once more than
    # ReadOne does; tao asks with Read.
    invocation.return_value(GLib.Variant("(v)", (
        GLib.Variant("v", value) if method == "Read" else value,)))


def connected(connection, name):
    # Before the name is taken, so that nothing can ask before it is answered.
    for interface in NODE.interfaces:
        connection.register_object(PATH, interface, called, None, None)


def lost(connection, name):
    global failed
    print(f"smoke portal: could not take {name}", file=sys.stderr)
    failed = True
    loop.quit()


failed = False
loop = GLib.MainLoop()
Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop",
                 Gio.BusNameOwnerFlags.NONE, connected, None, lost)
loop.run()
sys.exit(1 if failed else 0)
