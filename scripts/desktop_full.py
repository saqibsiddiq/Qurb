"""Every function of the desktop window that needs no phone, against a second
device that runs and syncs.

Run by scripts/desktop-smoke.sh with SMOKE_MODE=full, in the same isolated
home, display and WebDriver as the smoke test (see there). Where the smoke
test checks that each step works once, this checks what each function does to
files and to the other device:

 1. Set up through the window; name, notifications, start at login, picture,
    the space limit (and reading one typed), the 24 words shown.
 2. A second device, the command line, joined by the six digits and run, on
    a meeting point of the test's own.
 3. Files both ways: a file made here reaches the other device, and one made
    there arrives here; its details name both holders.
 4. A file's local copy freed, and fetched back.
 5. Deleting: the file gone on both, listed in Recently deleted, restored on
    both; deleted again and deleted for good; emptying Recently deleted.
 6. The Vault: a file moved in leaves the other device, and moved out
    returns to it.
 7. Two versions: the same file changed on both while apart, found, and
    settled by keeping both.
 8. Sharing a folder with this computer only, and with everyone again; a
    folder kept remotely, and here again.
 9. Sending: a file sent to the running device and collected there; one
    sent to it again is said to have gone before; a send cancelled. A file
    sent from the other device to this one, listed in Received.
10. Activity, Home's summary, syncing now, storage and its parts.
11. Joining another device by entering its code; saying two devices are the
    same, and not; removing one, and what it kept here deleted.
12. Quitting, and setting a fresh folder up with the 24 words.

It fails if a step does not reach the state it should, or if any command the
window calls fails other than on purpose. Commands that open another program
(the file manager, a file in its application) are called and must not fail;
what they open is not looked at.
"""

import os
import re
import socket
import subprocess
import time

import desktop_smoke as smoke
from desktop_smoke import HELPERS, HOME, QURB, Window, call_js, go, press, running

# Failures caused on purpose here, beside the smoke test's own.
EXPECTED = smoke.EXPECTED + (
    r"^read_allowance: `lots` is not an amount of space",
)


def say(words):
    print(words, flush=True)


def wait(what, check, timeout=60, every=0.5):
    """Wait for a Python check to return something truthy, and return it."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(every)
    raise AssertionError(f"never happened: {what}")


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def qurb(*args, check=True):
    done = subprocess.run([QURB, *args], capture_output=True, text=True, timeout=120)
    if check and done.returncode != 0:
        raise AssertionError(f"qurb {' '.join(args)}: {done.stdout}{done.stderr}")
    return done.stdout + done.stderr


def read(path):
    try:
        with open(path) as f:
            return f.read()
    except OSError:
        return None


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


class Other:
    """The second device: the command line, run as a daemon."""

    def __init__(self, folder):
        self.folder = folder
        self.process = None

    def start(self):
        self.log = open(os.path.join(HOME, "other.log"), "a")
        self.process = subprocess.Popen([QURB, "run", self.folder], stdout=self.log, stderr=subprocess.STDOUT)

    def stop(self):
        if self.process:
            self.process.terminate()
            self.process.wait(timeout=30)
            self.process = None

    def path(self, name):
        return os.path.join(self.folder, name)


def settings_and_key(window):
    call_js(window, "set_notifications", {"on": False})
    assert call_js(window, "settings", {})["notifications"] is False
    call_js(window, "set_notifications", {"on": True})

    autostart = os.path.join(os.environ["XDG_CONFIG_HOME"], "autostart")
    call_js(window, "set_starts_at_login", {"on": False})
    assert not call_js(window, "starts_at_login", {})
    call_js(window, "set_starts_at_login", {"on": True})
    assert call_js(window, "starts_at_login", {}), "start at login not turned on"
    assert os.listdir(autostart), "nothing written to the autostart folder"

    # A picture: the smallest JPEG there is, then none.
    jpeg = list(bytes.fromhex("ffd8ffe000104a46494600010100000100010000ffd9"))
    call_js(window, "set_picture", {"picture": jpeg})
    assert call_js(window, "settings", {})["picture"], "the picture was not kept"
    call_js(window, "set_picture", {"picture": None})
    assert not call_js(window, "settings", {})["picture"], "the picture was not removed"

    # The space limit: a size, read back by Storage; one typed, read; none.
    # Counted in 1024s, as the window, the phone and the config file count.
    assert call_js(window, "read_allowance", {"text": "300 GB"}) == str(300 << 30)
    try:
        call_js(window, "read_allowance", {"text": "lots"})
        raise AssertionError("`lots` was read as an amount of space")
    except AssertionError as e:
        if "was read" in str(e):
            raise
    call_js(window, "set_limit", {"bytes": str(100 << 30)})
    assert call_js(window, "storage", {})["limit"] == str(100 << 30)
    go(window, "storage")
    window.until("the limit on the storage page",
        "return [...document.querySelectorAll('.set')].some(s => s.textContent.includes('Space for QurbSpace') && s.textContent.includes('100 GB'))")
    call_js(window, "set_limit", {"bytes": "0"})

    words = call_js(window, "reveal_phrase", {})
    assert len(words) == 24, words
    go(window, "key")
    return words


def rename(window, name, signal):
    s = call_js(window, "settings", {})
    call_js(window, "save_settings", {"name": name, "signal": signal, "relay": "",
                                      "port": int(s.get("port") or 0), "downloads": s["downloads"]})
    assert call_js(window, "settings", {})["name"] == name


def the_other(window):
    return window.until("the other device listed",
        "return (HOME.devices ?? []).length ? HOME.devices[0] : (listsHome(), null)", timeout=30)


def connected(window, timeout=120):
    window.until("the other device online",
        "return (listsHome(), HOME.devices.some(d => d.route))", timeout=timeout)


def path_of(window, name):
    return os.path.join(window.js("return rootPath"), name)


def found(window, name, private=False):
    return [f for f in call_js(window, "find", {"text": name, "private": private}) if f["path"].endswith(name)]


def entry(window, name, private=False):
    hits = found(window, name, private)
    return hits[0] if hits else None


def both_ways(window, other):
    write(path_of(window, "from-here.txt"), "made on the laptop\n")
    wait("the file made here on the other device", lambda: read(other.path("from-here.txt")) == "made on the laptop\n", 120)
    write(other.path("from-there.txt"), "made on the other device\n")
    wait("the file made there here", lambda: read(path_of(window, "from-there.txt")) == "made on the other device\n", 120)
    # The other devices that hold it; this computer is said apart.
    held = call_js(window, "details", {"path": "from-there.txt"})["holders"]
    assert "Lab other" in held, f"holders: {held}"
    return held


def free_and_fetch(window):
    call_js(window, "free_file", {"path": "from-there.txt"})
    wait("the copy freed", lambda: (entry(window, "from-there.txt") or {}).get("availability") == "elsewhere", 30)
    assert not os.path.exists(path_of(window, "from-there.txt")) or os.path.getsize(path_of(window, "from-there.txt")) == 0
    call_js(window, "fetch", {"path": "from-there.txt"})
    wait("the copy fetched back", lambda: read(path_of(window, "from-there.txt")) == "made on the other device\n", 120)


def deleting(window, other):
    write(path_of(window, "to-delete.txt"), "delete me\n")
    wait("the file to delete on the other device", lambda: read(other.path("to-delete.txt")), 120)
    call_js(window, "delete_file", {"path": "to-delete.txt"})
    wait("the file gone here", lambda: not os.path.exists(path_of(window, "to-delete.txt")), 30)
    wait("the file gone there", lambda: not os.path.exists(other.path("to-delete.txt")), 120)
    gone = [d for d in call_js(window, "recently_deleted", {}) if d["path"] == "to-delete.txt"]
    assert gone, "not in Recently deleted"
    go(window, "deleted")
    window.until("its picture in Recently deleted", "return document.querySelectorAll('.rd').length > 0")

    call_js(window, "restore_deleted", {"id": gone[0]["id"]})
    wait("restored here", lambda: read(path_of(window, "to-delete.txt")) == "delete me\n", 30)
    wait("restored there", lambda: read(other.path("to-delete.txt")) == "delete me\n", 120)

    call_js(window, "delete_file", {"path": "to-delete.txt"})
    gone = wait("deleted again", lambda: [d for d in call_js(window, "recently_deleted", {}) if d["path"] == "to-delete.txt"], 30)
    call_js(window, "forget_deleted", {"id": gone[0]["id"]})
    assert not [d for d in call_js(window, "recently_deleted", {}) if d["path"] == "to-delete.txt"], "still listed after Delete"

    write(path_of(window, "also-gone.txt"), "and me\n")
    wait("indexed", lambda: entry(window, "also-gone.txt"), 30)
    call_js(window, "delete_file", {"path": "also-gone.txt"})
    wait("listed", lambda: call_js(window, "recently_deleted", {}), 30)
    call_js(window, "empty_deleted", {})
    assert call_js(window, "recently_deleted", {}) == [], "Recently deleted not emptied"


def vault(window, other):
    write(path_of(window, "secret.txt"), "only mine\n")
    wait("the secret on the other device", lambda: read(other.path("secret.txt")), 120)
    call_js(window, "move_file_area", {"path": "secret.txt", "private": True})
    wait("in the Vault here", lambda: entry(window, "secret.txt", private=True), 30)
    wait("gone from the other device's shared folder", lambda: not os.path.exists(other.path("secret.txt")), 120)
    call_js(window, "vault_keepers", {})
    call_js(window, "move_file_area", {"path": "secret.txt", "private": False})
    wait("shared again here", lambda: entry(window, "secret.txt"), 30)
    wait("back on the other device", lambda: read(other.path("secret.txt")) == "only mine\n", 120)


def two_versions(window, other):
    write(path_of(window, "both.txt"), "first\n")
    wait("both.txt on the other device", lambda: read(other.path("both.txt")) == "first\n", 120)
    other.stop()
    time.sleep(1)
    write(path_of(window, "both.txt"), "changed on the laptop\n")
    write(other.path("both.txt"), "changed on the other device\n")
    wait("the laptop's change indexed", lambda: (entry(window, "both.txt") or {}).get("size") == str(len("changed on the laptop\n")), 30)
    other.start()
    conflicts = wait("two versions found", lambda: call_js(window, "conflicts", {}), 180, every=2)
    go(window, "conflicts")
    window.until("the two versions drawn", "return document.querySelectorAll('.conflict .ver').length >= 2")
    kept = call_js(window, "settle_conflict", {"other": conflicts[0]["other"]["path"], "keep": "both"})
    assert call_js(window, "conflicts", {}) == [], "still two versions after keeping both"
    names = [n for n in os.listdir(window.js("return rootPath")) if n.startswith("both")]
    assert len(names) == 2, f"keeping both left {names}"
    return kept


def sharing_and_keeping(window, other):
    write(path_of(window, "Work/plan.txt"), "a plan\n")
    wait("Work on the other device", lambda: read(other.path("Work/plan.txt")), 120)
    here = [d["id"] for d in call_js(window, "sharing", {})["devices"] if d["here"]]
    call_js(window, "set_sharing", {"folder": "Work", "members": here})
    rules = {f["folder"]: f for f in call_js(window, "sharing", {})["folders"]}
    assert not rules["Work"]["everyone"], rules
    write(path_of(window, "Work/private-plan.txt"), "not for the other device\n")
    time.sleep(15)
    assert not os.path.exists(other.path("Work/private-plan.txt")), "a folder shared with this computer only reached the other device"
    call_js(window, "set_sharing", {"folder": "Work", "members": []})
    wait("shared with everyone again", lambda: read(other.path("Work/private-plan.txt")), 120)

    write(path_of(window, "Photos/a.jpg"), "a picture\n")
    wait("Photos on the other device", lambda: read(other.path("Photos/a.jpg")), 120)
    call_js(window, "keep_remotely", {"folder": "Photos"})
    wait("Photos kept remotely", lambda: (entry(window, "a.jpg") or {}).get("availability") == "elsewhere", 60)
    call_js(window, "keep_locally", {"folder": "Photos"})
    wait("Photos kept here again", lambda: read(path_of(window, "Photos/a.jpg")) == "a picture\n", 120)


def sending(window, other, device, me):
    note = os.path.join(HOME, "a-note.txt")
    write(note, "sent from the laptop\n")
    report = call_js(window, "send_files", {"paths": [note], "to": device["fingerprint"], "leaveOut": None})
    assert report["sent"] == 1, report
    received = os.path.join(HOME, "Downloads", "qurb", "a-note.txt")
    # The other device is the command line: what it receives goes to its
    # downloads folder, in the same home.
    wait("the send collected by the other device", lambda: not [s for s in call_js(window, "outgoing", {}) if s["path"].endswith("a-note.txt")], 120)
    before = call_js(window, "sent_before", {"paths": [note], "to": device["fingerprint"]})
    assert before, "a file sent before is not said to have been"

    big = os.path.join(HOME, "to-cancel.bin")
    with open(big, "wb") as f:
        f.write(os.urandom(64))
    other.stop()
    call_js(window, "send_files", {"paths": [big], "to": device["fingerprint"], "leaveOut": None})
    waiting = [s for s in call_js(window, "outgoing", {}) if s["path"].endswith("to-cancel.bin")]
    assert waiting, "the send is not waiting"
    go(window, "home")
    window.until("the send under Waiting", "return (listsHome(), document.getElementById('home-waiting').textContent.includes('Sending'))")
    go(window, "waiting")
    window.until("its picture on Waiting", "return document.querySelectorAll('.wa-cell').length > 0")
    call_js(window, "cancel_send", {"path": waiting[0]["path"], "to": device["fingerprint"]})
    assert not [s for s in call_js(window, "outgoing", {}) if s["path"].endswith("to-cancel.bin")], "cancelled, still waiting"
    other.start()

    back = os.path.join(HOME, "from-the-other.txt")
    write(back, "sent to the laptop\n")
    qurb("send", other.folder, back, "to", me)
    rows = wait("the file in Received", lambda: [r for r in call_js(window, "activity", {"path": None, "limit": 300, "before": None})
                                                 if r["kind"] == "received" and (r.get("path") or "").endswith("from-the-other.txt")], 180, every=2)
    go(window, "received")
    window.until("listed in Received", "return document.querySelector('.rows-card') && document.querySelector('.rows-card').textContent.includes('from-the-other.txt')")
    call_js(window, "show_received", {"id": rows[0]["id"]})
    call_js(window, "open_downloads", {})
    return received


def the_rest(window):
    assert call_js(window, "activity", {"path": None, "limit": 60, "before": None}), "no activity"
    go(window, "activity")
    window.until("activity's rows", "return document.querySelectorAll('.w-row').length > 0")
    call_js(window, "sync_now", {})
    summary = call_js(window, "summary", {})
    assert summary["peers"] >= 1, summary
    call_js(window, "storage", {})
    call_js(window, "storage_parts", {})
    go(window, "storage")
    window.until("the storage page", "return document.querySelector('.disk .free')")
    call_js(window, "update_state", {})
    call_js(window, "preview", {"path": "from-here.txt", "small": True})


def join_by_entering(window, words):
    """This computer joins a device showing a code: the command line's, one
    of the same person's devices -- set up with the same 24 words, as a
    device of somebody else's is refused."""
    third = os.path.join(HOME, "third")
    qurb("enrol", third, " ".join(words))
    host = subprocess.Popen([QURB, "pair", third], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, bufsize=1)
    said, code = "", None
    while code is None:
        line = host.stdout.readline()
        assert line, f"qurb pair showed no code: {said}"
        said += line
        m = re.search(r"(qurb1-\S+)", line)
        code = m.group(1) if m else None
    go(window, "pair-enter")
    window.js(HELPERS + "__smoke.type('.digits-in', arguments[0])", code)
    window.js("document.querySelector('.digits-in').dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter'}))")
    while "Approve?" not in said:
        more = host.stdout.read(1)
        assert more, f"qurb pair ended without asking: {said}"
        said += more
    host.stdin.write("y\n")
    host.stdin.flush()
    window.until("the device added", "return document.querySelector('.arrived')", timeout=60)
    rest, _ = host.communicate(timeout=60)
    assert "Paired with" in rest, rest
    return third


def same_and_remove(window):
    devices = call_js(window, "devices", {})
    own = [d for d in devices if d["relation"] == "own"]
    assert len(own) >= 2, own
    older, newer = own[0], own[1]
    call_js(window, "same_device", {"older": older["fingerprint"], "newer": newer["fingerprint"]})
    call_js(window, "same_device", {"older": older["fingerprint"], "newer": None})

    third = [d for d in call_js(window, "devices", {}) if d["relation"] == "own"][-1]
    plan = call_js(window, "removal_plan", {"device": third["fingerprint"]})
    assert plan["name"], plan
    call_js(window, "remove_device", {"device": third["fingerprint"], "deleteKept": False})
    assert not [d for d in call_js(window, "devices", {}) if d["fingerprint"] == third["fingerprint"]], "still listed"
    removed = call_js(window, "removed_devices", {})
    assert any(r["name"] == third["name"] for r in removed), removed
    parts = call_js(window, "storage_parts", {})
    for r in parts.get("removed") or []:
        call_js(window, "delete_removed", {"id": r["id"]})
    return third["name"]


def enrol_with_words(window, words):
    """Quit, and set a fresh folder up with the 24 words: the same key."""
    try:
        window.js("invoke('quit')")
    except Exception:
        pass
    window.close()
    time.sleep(2)
    fresh = os.path.join(HOME, "fresh")
    os.makedirs(fresh, exist_ok=True)
    window = Window([fresh])
    window.until("the first screen", *smoke.enabled("Start", ".setup-in"))
    press(window, "I already use QurbSpace", ".setup-in")
    window.until("the join step", "return document.querySelector('.setup-in .digits-in')")
    press(window, "Use my 24 words", ".setup-in")
    window.js(HELPERS + "__smoke.type('.setup-in textarea', arguments[0])", " ".join(words))
    press(window, "Join", ".setup-in")
    window.until("ready", *smoke.enabled("Open QurbSpace", ".setup-in"), timeout=60)
    press(window, "Open QurbSpace", ".setup-in")
    running(window)
    return window


def main():
    failures, ok = [], False
    signal_port = free_port()
    meeting = subprocess.Popen([QURB, "signal", f"127.0.0.1:{signal_port}"],
                               stdout=open(os.path.join(HOME, "signal.log"), "w"), stderr=subprocess.STDOUT)
    signal = f"ws://127.0.0.1:{signal_port}"
    window = Window()
    other = None
    earlier = []
    try:
        time.sleep(2)
        smoke.set_up(window)
        rename(window, "Lab laptop", signal)
        words = settings_and_key(window)
        say("1. set up; renamed; notifications, start at login, picture, space limit, the 24 words")

        folder, named = smoke.pair(window)
        other = Other(folder)
        qurb("config", folder, f"signal={signal}", "name=Lab other")
        other.start()
        device = the_other(window)
        connected(window)
        say(f"2. paired {named} by its six digits, running, and online")

        holders = both_ways(window, other)
        say(f"3. a file each way; its details name {', '.join(holders)} as holding it")
        free_and_fetch(window)
        say("4. a local copy freed, and fetched back")
        deleting(window, other)
        say("5. deleted on both, restored on both, deleted for good, Recently deleted emptied")
        vault(window, other)
        say("6. moved into the Vault and off the other device; out again and back on it")
        kept = two_versions(window, other)
        say(f"7. two versions found and settled, both kept ({kept})")
        sharing_and_keeping(window, other)
        say("8. a folder shared with this computer only, then everyone; a folder kept remotely, then here")
        sending(window, other, device, "Lab laptop")
        say("9. sent and collected; said to have gone before; a waiting send cancelled; one received, in Received")
        the_rest(window)
        say("10. activity, summary, sync now, storage and its parts, update state, a preview")
        join_by_entering(window, words)
        name = same_and_remove(window)
        say(f"11. joined a device by entering its code; same and not the same; {name} removed, its files deleted")
        earlier = window.js("return window.qurbFailures || []")
        window = enrol_with_words(window, words)
        say("12. quit, and a fresh folder set up with the 24 words")
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
        if other:
            other.stop()
        meeting.terminate()
        failures = [f for f in earlier + failures if not any(re.search(e, f) for e in EXPECTED)]
        if failures:
            print("commands that failed:", *failures, sep="\n  ")
    if failures or not ok:
        raise SystemExit(1)
    print("no command failed")


if __name__ == "__main__":
    main()
