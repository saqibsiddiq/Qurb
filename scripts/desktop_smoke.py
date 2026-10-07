"""Drive the real desktop window through WebKit's WebDriver.

Run by scripts/desktop-smoke.sh, which provides the isolated home, the
Broadway display and the WebDriver server this expects; see there.

What it does, in the real application with the real engine behind it:

1. Sets a device up from nothing through the window -- folder and storage;
   nothing to write down since decision 0052.
2. Opens every place in the sidebar.
3. Adds a device: shows a pairing code, has a second device (the command line,
   in a folder not set up) join with it and take the key, and approves it in
   the window once the two numbers match (decision 0053).
4. Sends that device a file, and sees it waiting under Transfers.
5. Removes that device, from its details, answering the question.
6. Protects the key with a passphrase from Settings, starts again, and
   unlocks from the window -- a wrong passphrase refused first.

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


def visible(step):
    return (f"return !document.querySelector('[data-step={step}]')"
            f".classList.contains('hidden') && document.querySelector('[data-step={step}]')"
            f".classList.contains('on')")


def set_up(window):
    window.until("the setting-up screens", "return !document.getElementById('setup').classList.contains('hidden')")
    window.click("get-started")
    window.click("choose-new")
    window.js("""const f = document.getElementById('folder-path');
                 f.value = arguments[0]; f.dispatchEvent(new Event('input'));""", f"{HOME}/qurb")
    window.until("the folder accepted", "return !document.getElementById('folder-next').disabled")
    window.click("folder-next")

    window.until("the storage presets", "return document.querySelectorAll('#allowances button:not([disabled])').length")
    window.js("document.querySelector('#allowances button:not([disabled])').click()")
    window.until("a size chosen", "return !document.getElementById('storage-next').disabled")
    window.click("storage-next")

    window.until("the ready screen", visible("ready"))
    window.click("ready-next")
    window.until("the main window", "return document.getElementById('setup').classList.contains('hidden')")


def go(window, place):
    window.js("document.querySelector(`#tabs [data-screen=${arguments[0]}]`).click()", place)
    window.until(f"the {place} screen", "return document.getElementById(arguments[0]).classList.contains('on')", place)


def every_tab(window):
    places = window.js("return [...document.querySelectorAll('#tabs .nav')].map(b => b.dataset.screen)")
    for place in places:
        go(window, place)
        time.sleep(0.5)
    return places


def pair(window):
    other = f"{HOME}/other"

    go(window, "devices")
    window.click("add-device")
    window.until("the two ways to add one", "return document.getElementById('pair-show')")
    window.click("pair-show")
    code = window.until("a code on the screen",
        "const t = document.getElementById('pair-code'); return t && t.textContent.split(' ').pop()")
    qr = window.js("return document.getElementById('qr').innerHTML.length")
    assert qr > 1000, f"no QR drawn ({qr} characters)"

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
        "const a = document.getElementById('pair-asking');"
        "return a && !a.classList.contains('hidden') && document.getElementById('pair-number').textContent")
    assert shown == expected, f"the window shows {shown}, the joining device {expected}"
    # As long as a person comparing two numbers might take.
    time.sleep(float(os.environ.get("SMOKE_THINK", "0")))
    window.click("pair-approve")
    rest, _ = joining.communicate(timeout=60)
    assert joining.returncode == 0, f"qurb join failed: {said}{rest}"
    named = window.until("the window saying paired",
        "return document.getElementById('pair-done') && document.getElementById('pair-with').textContent")
    window.click("pair-finish")
    return other, named


def send(window, device):
    path = f"{HOME}/to-send.txt"
    with open(path, "w") as f:
        f.write("sent by the desktop smoke test\n")
    # Called directly: the page chooses files through the system's file
    # dialog, which WebDriver cannot drive. execute/sync cannot wait on a
    # promise, so it is started and its result polled for.
    window.js("""window.__sent = null;
        window.__TAURI__.core.invoke('send_files', {paths: [arguments[0]], to: arguments[1]})
          .then(r => { window.__sent = r }, e => { window.__sent = {error: String(e)} });""", path, device)
    report = window.until("the send to finish", "return window.__sent")
    assert "error" not in report, f"send failed: {report['error']}"
    assert report["sent"] == 1, f"sent {report}"

    # Transfers appear when something is moving or waiting (direction §25).
    window.until("the Transfers chip", "return !document.getElementById('transfers-chip').classList.contains('hidden')")
    window.click("transfers-chip")
    window.until("the send listed under Transfers",
        "const p = document.querySelector('.panel.transfers'); return p && p.textContent.includes('to-send.txt')")
    window.js("closePanel()")


def remove(window, device):
    """Remove the paired device through its details, and see it gone.

    The file sent to it was never collected -- the other device is a command
    line that never ran -- so the question must say a send will be cancelled.
    """
    go(window, "devices")
    window.until("the device listed", "return document.querySelector('#device-list button.device')")
    window.js("document.querySelector('#device-list button.device').click()")
    window.until("its details", "return document.querySelector('.panel')")
    window.js("[...document.querySelectorAll('.panel button')].find(b => b.textContent.startsWith('Remove this device')).click()")
    said = window.until("the question", "const c = document.querySelector('.confirm'); return c && c.textContent")
    assert f"Remove {device}?" in said, said
    assert "1 file waiting for it to collect will be cancelled" in said, said
    window.js("[...document.querySelectorAll('.confirm button')].find(b => b.textContent === 'Remove device').click()")
    window.until("the device gone",
        "return !document.querySelector('#device-list button.device')")
    window.until("the send no longer waiting",
        "return document.getElementById('transfers-chip').classList.contains('hidden')")


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
    go(window, "settings")
    window.until("the Security section",
        "return [...document.querySelectorAll('#protection-buttons button')].find(b => b.textContent.startsWith('Protect'))")
    window.js("[...document.querySelectorAll('#protection-buttons button')].find(b => b.textContent.startsWith('Protect')).click()")
    window.js("""document.getElementById('protect-new').value = 'correct horse battery';
                 document.getElementById('protect-again').value = 'correct horse battery';
                 document.getElementById('protect-go').click();""")
    window.until("the key kept by passphrase",
        "return document.getElementById('protection-says').textContent.startsWith('Behind a passphrase')")
    EARLIER.extend(window.js("return window.qurbFailures || []"))
    # Quit, not close: closing the window keeps qurb running (decision 0040),
    # and the next launch would only ask that one to show itself.
    try:
        window.js("document.getElementById('quit').click()")
    except Exception:
        pass  # the page may go before the answer does
    window.close()
    time.sleep(2)

    # The folder named: the second device enrolled from the command line is
    # the most recently registered, and would otherwise be the one opened.
    window = Window([f"{HOME}/qurb"])
    window.until("the unlock screen",
        "const s = document.querySelector('[data-step=unlock]'); return s && s.classList.contains('on') && !document.getElementById('setup').classList.contains('hidden')")
    window.js("""document.getElementById('unlock-passphrase').value = 'wrong horse';
                 document.getElementById('unlock-go').click();""")
    said = window.until("the wrong one refused",
        "const s = document.getElementById('unlock-says'); return !s.classList.contains('hidden') && s.textContent")
    assert "does not open" in said, said
    window.js("""document.getElementById('unlock-passphrase').value = 'correct horse battery';
                 document.getElementById('unlock-go').click();""")
    window.until("the main window", "return document.getElementById('setup').classList.contains('hidden')")
    return window


def main():
    window = Window()
    failures = []
    ok = False
    try:
        time.sleep(2)
        set_up(window)
        print("set up through the window")

        print("opened", ", ".join(every_tab(window)))

        other, named = pair(window)
        print(f"paired by code with {named}, approved in the window by its number")

        device = named.split(" (")[0]
        send(window, device)
        print(f"sent a file to {device}")
        remove(window, device)
        print(f"removed {device}, and its waiting send with it")
        # The wrong passphrase is a command that fails on purpose.
        window = lock_and_unlock(window)
        print("protected the key with a passphrase, and unlocked it from the window")
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


if __name__ == "__main__":
    main()
