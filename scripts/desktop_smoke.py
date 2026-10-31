"""Drive the real desktop window through WebKit's WebDriver.

Run by scripts/desktop-smoke.sh, which provides the isolated home, the
Broadway display and the WebDriver server this expects; see there.

What it does, in the real application with the real engine behind it:

1. Sets a device up from nothing through the window -- folder and storage;
   nothing to write down since decision 0052.
2. Opens every place, and the pages reached from them.
3. Adds a device: shows a pairing code, has a second device (the command line,
   in a folder not set up) join with its six digits (decision 0066) and take
   the key, and approves it in the window once the two numbers match
   (decision 0053).
4. Sends that device a file, sees it waiting on Home, and clicks the
   status mark, which syncs.
5. Removes that device, from its page, answering the question.
6. Protects the key with a passphrase from Settings, starts again, and
   unlocks from the window -- a wrong passphrase refused first.

With SMOKE_MODE=join it instead sets the window's device up by joining
another device's code (decision 0052): the command line shows the code and
asks y/N, and says yes only when the number the window shows is the number it
asks about (decision 0053). Then it opens every place.

The window is the redesigned one (docs/design/desktop.md): places are opened
with its own `go`, and buttons found by their words.

With SMOKE_MODE=theme it sets a device up and checks that the window follows
the desktop's dark preference (decision 0056): dark when the desktop is dark
as the window opens, light and dark again as the desktop's style is switched,
and the person's own choice in Settings over either. The preference comes
from a stand-in for the desktop's settings portal (scripts/smoke_portal.py).

It fails if any command the window calls returns an error, whichever step
called it, or if a step does not reach the state it should. It checks that
the window works, not how it looks: the Broadway display reports a nonsense
geometry, so screenshots from it are not worth reading.
"""

import base64
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

DRIVER = os.environ["SMOKE_DRIVER"]
APP = os.environ["SMOKE_APP"]
QURB = os.environ["SMOKE_QURB"]
HOME = os.environ["HOME"]


def call(method, path, body=None):
    request = urllib.request.Request(
        DRIVER + path,
        method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as answer:
            return json.load(answer)["value"]
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"{method} {path}: {e.read().decode()[:400]}") from None


class Window:
    def __init__(self, args=()):
        caps = {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": APP, "args": list(args)}}}}
        self.id = call("POST", "/session", caps)["sessionId"]

    def js(self, script, *args):
        return call("POST", f"/session/{self.id}/execute/sync",
                    {"script": script, "args": list(args)})

    def click(self, element_id):
        self.js("document.getElementById(arguments[0]).click()", element_id)

    def until(self, what, script, *args, timeout=20):
        """Wait for a script to return something truthy, and return it."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            value = self.js(script, *args)
            if value:
                return value
            time.sleep(0.2)
        raise AssertionError(f"never happened: {what}")

    def screenshot(self, path):
        with open(path, "wb") as f:
            f.write(base64.b64decode(call("GET", f"/session/{self.id}/screenshot")))

    def close(self):
        try:
            call("DELETE", f"/session/{self.id}")
        except Exception:
            pass


# The window's own helpers, defined in the page once: a button by its words,
# and setting a field as typing would.
HELPERS = """
window.__smoke = {
  // The newest of what `scope` names: a sheet closing is still in the page
  // for a moment after the next one opens.
  within: (scope) => (scope ? [...document.querySelectorAll(scope)].pop() : document),
  button: (words, scope) => [...(__smoke.within(scope)?.querySelectorAll('button') ?? [])]
      .find(b => b.textContent.trim() === words),
  press: (words, scope) => {
    const b = __smoke.button(words, scope);
    if (!b) throw new Error(`no button ${words} in ${scope} (${scope ? document.querySelectorAll(scope).length : 1} of them)`);
    b.click();
  },
  type: (selector, value) => { const f = document.querySelector(selector); f.value = value; f.dispatchEvent(new Event('input')); },
};
"""


def press(window, words, scope=None):
    window.js(HELPERS + "__smoke.press(arguments[0], arguments[1])", words, scope)


def enabled(words, scope=None):
    return ("const w = arguments[1] ? [...document.querySelectorAll(arguments[1])].pop() : document;"
            " const b = w && [...w.querySelectorAll('button')].find(b => b.textContent.trim() === arguments[0]);"
            " return b && !b.disabled", words, scope)


def running(window):
    window.until("the main window",
        "return !document.body.classList.contains('setting-up') && document.getElementById('home-mark')")


def folder_and_storage(window, choice):
    window.until("the setting-up screens", "return document.querySelector('.setup-in .choice')")
    press(window, choice, ".setup-in")
    window.until("the folder step", "return document.querySelector('.setup-in .field')")
    window.js(HELPERS + "__smoke.type('.setup-in .field', arguments[0])", f"{HOME}/qurb")
    window.until("the folder accepted", *enabled("Continue", ".setup-in"))
    press(window, "Continue", ".setup-in")
    window.until("a size chosen", "return document.querySelector('.setup-in .amount.on')")
    window.until("Continue", *enabled("Continue", ".setup-in"))
    press(window, "Continue", ".setup-in")


def set_up(window):
    folder_and_storage(window, "Start hereThis is my first device")
    window.until("the ready screen", *enabled("Open QurbSpace", ".setup-in"))
    press(window, "Open QurbSpace", ".setup-in")
    running(window)


def join_by_code(window):
    """Set this computer up with another device's code, and its key."""
    host_dir = f"{HOME}/host"
    subprocess.run([QURB, "init", host_dir], check=True, capture_output=True)
    host = subprocess.Popen([QURB, "pair", host_dir], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, bufsize=1)
    said, code = "", None
    while code is None:
        line = host.stdout.readline()
        assert line, f"qurb pair showed no code: {said}"
        said += line
        found = re.search(r"(qurb1-\S+)", line)
        code = found.group(1) if found else None

    folder_and_storage(window, "Join my phoneWith a code from it")
    window.until("the join step", "return document.querySelector('.setup-in .digits-in')")
    window.js(HELPERS + "__smoke.type('.setup-in .digits-in', arguments[0])", code)
    press(window, "Join", ".setup-in")
    shown = window.until("the number to compare",
        "const b = document.querySelector('.setup-in .note b'); return b && b.textContent")

    while "Approve?" not in said:
        more = host.stdout.read(1)
        assert more, f"qurb pair ended without asking: {said}"
        said += more
    asked = re.findall(r"\d{3} \d{3}", said.split("wants to", 1)[-1])
    assert asked and asked[0] == shown, f"the window shows {shown}, qurb pair asks about {asked}"
    host.stdin.write("y\n")
    host.stdin.flush()

    window.until("the ready screen", *enabled("Open QurbSpace", ".setup-in"))
    press(window, "Open QurbSpace", ".setup-in")
    running(window)
    rest, _ = host.communicate(timeout=60)
    assert "Paired with" in rest, f"qurb pair did not pair: {said}{rest}"
    return shown


def portal(scheme):
    """Switch the desktop's style, as the stand-in portal has it: 1 dark, 2 light."""
    subprocess.run(["gdbus", "call", "--session", "--dest", "org.freedesktop.portal.Desktop",
                    "--object-path", "/org/freedesktop/portal/desktop",
                    "--method", "org.qurb.SmokePortal.Set", str(scheme)],
                   check=True, capture_output=True)


def choose_theme(window, words):
    go(window, "settings")
    window.js("themeSheet()")
    window.until("the theme sheet", "return document.querySelector('.sheet .rounds')")
    press(window, words, ".sheet")


def follows_the_desktop(window):
    """The window's theme, as the desktop's style changes and as chosen."""
    drawn = ("return [document.documentElement.dataset.theme,"
             " getComputedStyle(document.body).backgroundColor]")

    def drawn_as(theme, what):
        window.until(what, "return document.documentElement.dataset.theme === arguments[0]", theme)
        return window.js(drawn)[1]

    seen = {"dark at start": drawn_as("dark", "dark, as the desktop was when it opened")}
    portal(2)
    seen["the desktop switched to light"] = drawn_as("light", "light, when the desktop turned light")
    portal(1)
    seen["and to dark"] = drawn_as("dark", "dark, when the desktop turned dark again")
    if seen["dark at start"] == seen["the desktop switched to light"]:
        raise AssertionError(f"the theme changed and the page did not: {seen}")

    choose_theme(window, "Light")
    seen["Light chosen, the desktop dark"] = drawn_as("light", "light, when chosen")
    portal(2)
    portal(1)
    time.sleep(0.5)
    if window.js(drawn)[0] != "light":
        raise AssertionError("the desktop's style overrode the choice made in Settings")
    choose_theme(window, "System")
    seen["System chosen again"] = drawn_as("dark", "the desktop's again")
    return seen


def go(window, place, arg=None):
    window.js("go(arguments[0], arguments[1] ?? undefined)", place, arg)
    window.until(f"the {place} place", "return place === arguments[0]", place)


# Every place that opens with nothing chosen first: the sidebar's three, and
# the pages reached from them.
PLACES = ["home", "files", "settings", "storage", "key", "advanced", "activity",
          "received", "deleted", "conflicts", "waiting", "add"]


def every_place(window):
    for place in PLACES:
        go(window, place)
        time.sleep(0.6)
    for area in ("shared", "vault"):
        window.js("inFiles(arguments[0])", area)
        time.sleep(0.6)
    return PLACES + ["Shared", "Vault"]


def pair(window):
    other = f"{HOME}/other"

    go(window, "add")
    press(window, "Show a codeScan it with your phone")
    digits = window.until("six digits on the screen",
        "const d = document.querySelector('.pair > .digits'); return d && /\\d{3} \\d{3}/.test(d.textContent) && d.textContent")
    qr = window.js("return document.querySelector('.qr').innerHTML.length")
    assert qr > 1000, f"no QR drawn ({qr} characters)"
    code = digits.replace(" ", "")
    # Joined by the six digits (decision 0066), which fetch the long code over
    # this machine's own network; SMOKE_LONG_CODE=1 for the long code instead.
    if os.environ.get("SMOKE_LONG_CODE"):
        code = window.js("return document.querySelector('.pair .code').textContent.split(' ').pop()")

    # The joining side prints the number before it connects, then waits for
    # the window's answer; the window shows who asks and the number it should
    # be showing. Approved only when the two agree.
    joining = subprocess.Popen([QURB, "join", other, code], stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True)
    said = ""
    expected = None
    for line in joining.stdout:
        said += line
        found = re.search(r"\b(\d{3} \d{3})\b", line)
        if found:
            expected = found.group(1)
            break
    assert expected, f"qurb join printed no number: {said}"
    shown = window.until("the window asking about the device",
        "const d = document.querySelector('.asking-box .digits'); return d && d.textContent")
    assert shown == expected, f"the window shows {shown}, the joining device {expected}"
    # As long as a person comparing two numbers might take.
    time.sleep(float(os.environ.get("SMOKE_THINK", "0")))
    press(window, "Approve", ".asking-box")
    rest, _ = joining.communicate(timeout=60)
    assert joining.returncode == 0, f"qurb join failed: {said}{rest}"
    named = window.until("the window saying paired",
        "const h = document.querySelector('.arrived h1'); return h && h.textContent")
    press(window, "Done", ".arrived")
    return other, named


def send(window, device):
    path = f"{HOME}/to-send.txt"
    with open(path, "w") as f:
        f.write("sent by the desktop smoke test\n")
    # Called directly: the page chooses files through the system's file
    # dialog, which WebDriver cannot drive. execute/sync cannot wait on a
    # promise, so it is started and its result polled for.
    window.js("""window.__sent = null;
        window.__TAURI__.core.invoke('send_files', {paths: [arguments[0]], to: arguments[1], leaveOut: null})
          .then(r => { window.__sent = r }, e => { window.__sent = {error: String(e)} });""", path, device)
    report = window.until("the send to finish", "return window.__sent")
    assert "error" not in report, f"send failed: {report['error']}"
    assert report["sent"] == 1, f"sent {report}"


def home(window):
    """Home lists the send under Waiting, and the mark syncs when clicked.
    The other device never runs, so nothing is synced: what this checks is
    that asking works."""
    go(window, "home")
    window.js("listsHome()")
    window.until("the send under Waiting",
        "const w = document.getElementById('home-waiting'); return w && w.textContent.includes('to-send.txt')")
    window.js("document.getElementById('home-mark').click()")
    time.sleep(1)


def remove(window, device):
    """Remove the paired device from its page, and see it gone.

    The file sent to it was never collected -- the other device is a command
    line that never ran -- so the question must say a send will be cancelled.
    """
    window.until("the device in the circle's list", "return HOME.devices.length")
    window.js("go('device', HOME.devices[0])")
    window.until("its page", "return document.querySelector('.remove')")
    window.js("document.querySelector('.remove').click()")
    said = window.until("the question", "const c = document.querySelector('.sheet'); return c && c.textContent")
    assert f"Remove {device}?" in said, said
    assert "1 send cancelled" in said, said
    press(window, "Remove", ".sheet .acts")
    window.until("back home", "return place === 'home'")
    window.js("listsHome()")
    window.until("the device gone, and its send", "return HOME.devices.length === 0 && HOME.sends.length === 0")


# Failures a step causes on purpose, and so not a reason to fail the run.
EXPECTED = ("unlock: that passphrase does not open this key",)

# Failures from windows already closed: starting again opens a new page, and
# its list starts empty.
EARLIER = []


def lock_and_unlock(window):
    """Protect the key with a passphrase from Settings, start again, and
    unlock from the window -- refused once with the wrong one first.

    Returns the new window: the old one is closed by starting again.
    """
    go(window, "key")
    window.until("how the key is kept", "return [...document.querySelectorAll('.set')].some(s => s.textContent.startsWith('Passphrase'))")
    window.js("[...document.querySelectorAll('.set')].find(s => s.textContent.startsWith('Passphrase')).click()")
    window.until("the passphrase sheet", "return document.querySelectorAll('.sheet .field').length === 2")
    window.js("""const f = document.querySelectorAll('.sheet .field');
                 f[0].value = 'correct horse battery'; f[1].value = 'correct horse battery';""")
    press(window, "Change", ".sheet")
    window.until("the key kept by passphrase",
        "return [...document.querySelectorAll('.set')].some(s => s.textContent.startsWith('Passphrase') && s.querySelector('.set-tick'))")
    EARLIER.extend(window.js("return window.qurbFailures || []"))
    # Quit, not close: closing the window keeps qurb running (decision 0040),
    # and the next launch would only ask that one to show itself.
    try:
        window.js("invoke('quit')")
    except Exception:
        pass  # the page may go before the answer does
    window.close()
    time.sleep(2)

    # The folder named: the second device enrolled from the command line is
    # the most recently registered, and would otherwise be the one opened.
    window = Window([f"{HOME}/qurb"])
    window.until("the unlock screen",
        "const h = document.querySelector('.setup-in h1'); return h && h.textContent === 'QurbSpace is locked'")
    window.js("document.querySelector('.setup-in .field').value = 'wrong horse'")
    press(window, "Unlock", ".setup-in")
    said = window.until("the wrong one refused",
        "const s = document.querySelector('.setup-in .note.warn'); return s && s.textContent")
    assert "does not open" in said, said
    window.js("document.querySelector('.setup-in .field').value = 'correct horse battery'")
    press(window, "Unlock", ".setup-in")
    running(window)
    return window


def main():
    window = Window()
    failures = []
    ok = False
    try:
        time.sleep(2)
        if os.environ.get("SMOKE_MODE") == "join":
            number = join_by_code(window)
            print(f"set up by joining another device's code, approved there at {number}")
            print("opened", ", ".join(every_place(window)))
        elif os.environ.get("SMOKE_MODE") == "theme":
            set_up(window)
            for step, background in follows_the_desktop(window).items():
                print(f"{step}: the page drawn on {background}")
        else:
            window = run_through(window)
        ok = True
    except Exception:
        try:
            window.screenshot(os.path.join(HOME, "failed.png"))
        except Exception:
            pass
        raise
    finally:
        try:
            failures = window.js("return window.qurbFailures || []")
        except Exception:
            pass
        window.close()
        # Printed on the way out of a failed step too: which command failed,
        # and with what, is usually the reason the step did not finish.
        failures = [f for f in EARLIER + failures if f not in EXPECTED]
        if failures:
            print("commands that failed:", *failures, sep="\n  ")

    if failures or not ok:
        sys.exit(1)
    print("no command failed")


def run_through(window):
    """Steps 1 to 6: a device set up, paired, sent to, removed, and locked.
    Returns the window, which unlocking replaces."""
    set_up(window)
    print("set up through the window")

    print("opened", ", ".join(every_place(window)))

    other, named = pair(window)
    print(f"paired by its six digits with {named}, approved in the window by its number")

    device = named.split(" (")[0]
    send(window, device)
    print(f"sent a file to {device}")
    home(window)
    print("Home lists it as waiting, and the mark syncs when clicked")
    remove(window, device)
    print(f"removed {device}, and its waiting send with it")
    # The wrong passphrase is a command that fails on purpose.
    window = lock_and_unlock(window)
    print("protected the key with a passphrase, and unlocked it from the window")
    return window


if __name__ == "__main__":
    main()
