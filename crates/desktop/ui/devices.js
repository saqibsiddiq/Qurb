// Devices, reached through Home's circle: a device's page, choosing a
// phone's folders, adding a device or a person, and removing one.

// ---------- A device's page ----------

// The phone's device page (the owner's review of 2026-11-03, H-2): its
// circle, name, On or Off and ⓘ; one card of what it keeps here; Remove in
// red and Switch device in blue. Identity, pairing, the path and an earlier
// setup are behind ⓘ, off the page; Send is Home's alone.

function drawDevicePage(main, d) {
  HOME.page = d;
  main.append(
    h("div", { class: "page" },
      h("button", { class: "back", onclick: () => go("home") }, icon("chevron-right", "flip"), "Home"),
      h("div", { class: "dev-hd" },
        circle(d, "circle-xl"),
        h("div", { class: "dev-words" },
          h("h1", {}, d.name),
          h("p", { class: "state" }, d.route ? "On" : d.last_seen ? h("span", {}, "Off", h("small", {}, ago(d.last_seen))) : "Off")),
        h("button", { class: "icon-btn", "aria-label": `About ${d.name}`, onclick: () => aboutDevice(d) }, icon("info"))),
      h("div", { id: "page-list" }),
      // What can be done to the device, as a card like those above it (the
      // owner, 2026-11-09: the two solid bars "aren't good enough"):
      // switching to another, then removing this one, in red.
      // Switch device a quiet button, Remove a red link under it, as the
      // desktop's draft 1 has them.
      h("div", { class: "dev-foot" },
        h("button", { class: "btn btn-quiet btn-sm", onclick: () => switchDevice((to) => go("device", to)) }, icon("arrow-up-down"), "Switch device"),
        h("button", { class: "link-danger", onclick: () => askToRemove(d) }, icon("trash-2"), d.relation === "guest" ? `Remove ${d.name}` : "Remove"))));
  listsDevicePage();
}

/** Who it is, behind ⓘ: its identity, when it was paired, how it is
 * reached now, and what it was before it was set up again. */
async function aboutDevice(d) {
  const all = await invoke("devices").catch(() => []);
  const box = sheet({ ic: circle(d, "circle-lg"), title: d.name });
  const earlier = d.earlier ? all.find((e) => e.id === d.earlier) : null;
  const later = d.relation === "own" && !d.replaced_by
    ? all.filter((x) => x.relation === "own" && !x.replaced_by && x.id !== d.id && x.paired_at > d.paired_at)
    : [];
  box.append(h("div", { class: "grp-box" },
    setting({ ic: "key-round", name: "Identity", value: d.fingerprint.slice(0, 8) }),
    setting({ ic: "clock", name: "Paired", value: ago(d.paired_at) }),
    setting({ ic: "link", name: "Path", value: d.route === "relay" ? "Relayed" : d.route === "direct" ? "Direct" : "Not connected" }),
    earlier ? setting({ ic: "history", name: "Earlier setup", value: earlier.held ? size(earlier.held.bytes) : "", onclick: () => { box.close(); askToRemove(earlier, `the earlier ${earlier.name}`); } }) : null,
    earlier?.said_same ? setting({ ic: "git-compare", name: "Not the same device", onclick: () => { box.close(); sameDevice(earlier, null); } }) : null,
    later.length ? setting({ ic: "git-compare", name: "The same as another device", onclick: () => { box.close(); sameAs(d, later); } }) : null));
  icons(box);
}

async function listsDevicePage() {
  const d = HOME.page;
  const box = $("page-list");
  if (!d || !box) return;
  const rows = [];
  if (d.relation === "guest") {
    rows.push(setting({ ic: "lock-keyhole", name: "Their folder", value: Number(d.kept ?? 0) > 0 ? size(d.kept) : null, onclick: () => openGuest(d) }));
  } else if (d.kind === "phone") {
    const f = await invoke("phone_folders", { phone: d.fingerprint }).catch(() => null);
    const chosen = (f?.folders ?? []).filter((x) => x.chosen).length;
    rows.push(
      setting({ ic: "folder", name: "Folders", value: chosen ? String(chosen) : null, onclick: () => go("phone-folders", d) }),
      setting({ ic: "whatsapp", name: "WhatsApp", value: d.whatsapp ? size(d.whatsapp.bytes) : null }),
      setting({ ic: "lock-keyhole", name: "Private Vault", value: Number(d.vault ?? 0) > 0 ? size(d.vault) : null, onclick: () => go("files") }));
  } else {
    // A computer: this computer's own folders that it keeps (S-4), as a
    // phone's are set on its computer's page.
    rows.push(setting({ ic: "folder", name: "Folders", onclick: () => go("backup") }));
  }
  put(box, label("Keeps here"), h("div", { class: "grp-box" }, ...rows));
}

function listRow(ic, title, sub, progress) {
  return h("div", { class: "row" }, tile(ic),
    h("span", { class: "row-main" }, h("b", {}, title), h("small", {}, sub),
      progress != null && progress < 1 ? h("span", { class: "w-bar" }, h("i", { style: { width: `${Math.round(progress * 100)}%` } })) : null));
}

/** A guest's folder: opened in the file manager once their phone says yes,
 * as Files opens it (F-2). */
async function openGuest(d) {
  const people = await invoke("people").catch(() => []);
  const p = people.find((x) => x.relation === "guest" && (x.devices ?? []).includes(d.name)) ?? people.find((x) => x.id.startsWith(d.fingerprint));
  if (!p) { toast("Nothing kept for them yet"); return; }
  if (p.state === "open") openPerson(p); else askToOpen(p);
}

/** Which later device this one became: chosen, then counted as one. */
function sameAs(d, later) {
  const box = sheet({ ic: circle(d, "circle-lg"), title: `${d.name} is now…`, sub: "Listed as one device from then on" });
  box.append(h("div", { class: "rows" }, ...later.map((x) => row({
    ic: circle(x, "circle-md"), name: x.name, sub: x.route ? "Online" : `Seen ${ago(x.last_seen)}`,
    onclick: () => { box.close(); sameDevice(d, x); },
  }))));
}

async function sameDevice(older, newer) {
  try {
    await invoke("same_device", { older: older.fingerprint, newer: newer?.fingerprint ?? null });
  } catch (e) { toast(String(e), true); return; }
  toast(newer ? `${older.name} is ${newer.name} now` : `${older.name} is listed on its own again`);
  await listsHome();
  const fresh = HOME.devices.find((x) => x.fingerprint === (newer ?? older).fingerprint);
  go("device", fresh ?? newer ?? older);
}

// ---------- A phone's folders (decision 0063) ----------

async function drawPhoneFolders(main, d) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Back up folders", { words: d.name, go: () => go("device", d) }), body));
  const draw = async () => {
    let answer;
    try { answer = await invoke("phone_folders", { phone: d.fingerprint }); } catch (e) { put(body, oops(e)); return; }
    if (answer.seen_at == null || !answer.folders.length) {
      put(body, empty("smartphone", `On ${d.name}: QurbSpace, Settings, Backups, Back up your folders`));
      return;
    }
    put(body, 
      h("p", { class: "note" }, "Over Wi-Fi. Files deleted on the phone stay here"),
      h("div", { class: "grp-box" }, ...answer.folders.map((f) => {
        const depth = f.path.split("/").length - 1;
        const s = setting({
          ic: "folder",
          name: depth ? base(f.path) : f.path,
          value: f.kept_files ? `${f.kept_files.toLocaleString()} of ${f.files.toLocaleString()}` : `${f.files.toLocaleString()} ${f.files === 1 ? "file" : "files"}`,
          end: toggle(f.chosen, async (on) => {
            await invoke("choose_phone_folder", { phone: d.fingerprint, folder: f.path, chosen: on });
            draw();
          }, f.path),
        });
        s.style.paddingLeft = `${16 + depth * 28}px`;
        return s;
      })),
      h("p", { class: "note" }, `As ${d.name} said ${ago(answer.seen_at)}`));
  };
  draw();
}

// ---------- Removing ----------

/** What removing does, said before it is done (brief §35): trust ends here,
 * nothing on the device is touched, and what this computer could no longer
 * get back is named. */
/** Removing a device, said plainly first (D21; decision 0084): both sides
 * stop syncing; what it keeps here stays, locked, and is its again when the
 * same device is added back. Deleting that too is a choice, off unless
 * chosen. A send waiting for it, or files only on it, are said too. */
async function askToRemove(d, called) {
  let plan;
  try { plan = await invoke("removal_plan", { device: d.fingerprint }); } catch (e) { toast(String(e), true); return; }
  const name = called ?? plan.name;
  const box = sheet({ ic: circle(d, "circle-lg"), title: `Remove ${name}?`, sub: "It stops syncing with this computer, on both", wide: true });
  const rows = [];
  let deleteKept = false;
  if (plan.kept > 0) {
    rows.push(setting({ ic: "lock-keyhole", name: `Its ${size(plan.kept_bytes)} here stay, locked` }),
      setting({ ic: "trash-2", name: "Delete them too", end: toggle(false, (on) => { deleteKept = on; }, "Delete them too") }));
  }
  if (plan.waiting > 0) rows.push(setting({ ic: "send", name: `${count(plan.waiting, "send")} cancelled` }));
  if (plan.holds_ours) rows.push(setting({ ic: "shield-check", name: "It stops keeping your Vault" }));
  if (plan.only_there.length) {
    rows.push(setting({
      ic: "triangle-alert", name: `${count(plan.only_there.length, "file")} only on it`,
      end: h("button", { class: "btn btn-quiet btn-sm", onclick: async (e) => {
        for (const path of plan.only_there) { try { await invoke("fetch", { path }); wanted.add(path); } catch (er) { /* said in Activity */ } }
        e.currentTarget.replaceWith(h("small", { class: "note" }, "Asked for"));
      } }, "Keep here first"),
    }));
  }
  box.append(
    rows.length ? h("div", { class: "grp-box grp-flat" }, ...rows) : null,
    plan.kept > 0 ? h("p", { class: "sheet-note" }, `Add the same ${d.kind === "phone" ? "phone" : "device"} again to open them`) : null,
    actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Remove", async (e) => {
      e.currentTarget.disabled = true;
      try {
        await invoke("remove_device", { device: d.fingerprint, deleteKept });
        box.close();
        go("home");
      } catch (er) { e.currentTarget.disabled = false; toast(String(er), true); }
    }, "btn-danger-solid")));
  icons(box);
}

// ---------- Adding a device ----------

let pairingWatch = null;

function stopPairing() {
  if (!pairingWatch) return;
  clearInterval(pairingWatch);
  pairingWatch = null;
  // Stopped at both ends: off the screen, and no longer answered.
  invoke("stop_pairing").catch(() => {});
}

// Adding a device (P-2 to P-4): *My phone* or *Someone else's*, both to the
// code screen; entering a code from a phone a small link.
function drawAdd(main) {
  const choice = (ic, words, act, cls = "") => h("button", { class: `choice ${cls}`.trim(), onclick: act },
    h("span", { class: "choice-c" }, icon(ic)), h("b", {}, words));
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead("Add a device", { words: "Home", go: () => go("home") }),
    h("div", { class: "choices" },
      choice("smartphone", "My phone", () => go("pair-show", { guest: false })),
      choice("users", "Someone else's", () => go("pair-show", { guest: true }), "choice-person")),
    h("button", { class: "link", onclick: () => go("pair-enter") }, "Enter a code instead")));
}

// Add your phone (D17): the code to scan, its six digits, how long it
// lasts, and Later. The phone that scans asks in a sheet (D18) -- or, a
// phone removed here coming back, says it is back and what is its again
// (D23; decision 0084).
async function drawShowCode(main, { guest, first }) {
  const stage = h("div", { class: "pair" }, h("span", { class: "empty-orb asking" }, icon("qr-code")));
  const leave = () => { stopPairing(); go(first ? "home" : "add"); };
  main.append(h("div", { class: "center-page" },
    first ? null : h("button", { class: "back back-corner", onclick: leave }, icon("chevron-right", "flip"), "Add a device"),
    h("h1", { class: "ptitle" }, guest ? "Add someone else's phone" : "Add your phone"),
    stage));
  let inv;
  try { inv = await invoke("start_pairing", { guest }); } catch (e) { put(stage, oops(e)); return; }
  const qr = h("div", { class: "qr" });
  if (inv.qr) qr.innerHTML = inv.qr;
  else qr.append(icon("qr-code"));
  const digits = h("p", { class: "digits" }, inv.digits ?? "");
  const time = h("span", {});
  const left = h("p", { class: "count" }, icon("clock"), time);
  // No sentence of instructions: the phone's own screen says Scan (P-2).
  put(stage, qr, digits, left, btn("Later", leave, "btn-quiet btn-later"));

  let asked = null;
  let asking = null;
  const follow = async () => {
    let s;
    try { s = await invoke("pairing_state"); } catch (e) { time.textContent = String(e); return; }
    if (place !== "pair-show") { stopPairing(); asking?.close(); return; }
    if (s.state === "asking") {
      const key = `${s.name}|${s.number}`;
      if (asked === key) return;
      asked = key;
      asking?.close();
      // The phone that scanned and the six digits to compare (D18).
      asking = sheet({
        ic: h("span", { class: "circle circle-lg" }, (s.name || "?").trim()[0].toUpperCase()),
        title: s.back ? `${s.name} is back` : s.name,
        sub: s.back ? h("p", { class: "sub-ic" }, icon("lock-keyhole"), `Its ${size(s.back)} here are its again`) : null,
      });
      asking.append(
        h("div", { class: "six" }, ...String(s.number).replace(/\s/g, "").split("").map((n) => h("span", {}, n))),
        h("p", { class: "sub-ic sub-quiet" }, icon("smartphone"), "Same on the phone?"),
        actions(btn("Decline", () => reply(false), "btn-quiet"), btn("Approve", () => reply(true), "btn-main")));
      return;
    }
    if (s.state === "waiting") {
      if (asked) { asked = null; asking?.close(); asking = null; }
      if (s.digits_withdrawn) digits.classList.add("withdrawn");
      const secs = Math.max(0, inv.expires_at - Math.floor(Date.now() / 1000));
      time.textContent = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
      return;
    }
    asking?.close();
    clearInterval(pairingWatch);
    pairingWatch = null;
    if (s.state === "paired") { toast(`${s.name ?? "It"} added`); go("home"); }
    else time.textContent = s.state === "expired" ? "Expired. Show a new one" : (s.message ?? "Didn't work");
  };
  const reply = async (yes) => {
    asking?.querySelectorAll("button").forEach((b) => (b.disabled = true));
    try { await invoke("answer_pairing", { approve: yes }); } catch (e) { toast(String(e), true); }
  };
  clearInterval(pairingWatch);
  pairingWatch = setInterval(follow, 700);
  follow();
}

function drawEnterCode(main) {
  const says = h("p", { class: "note" }, "On the same Wi-Fi");
  const input = h("input", { class: "digits-in", inputmode: "numeric", autocomplete: "off", spellcheck: "false", placeholder: "000000", "aria-label": "The code" });
  const join = async () => {
    if (input.disabled || !input.value.trim()) return;
    input.disabled = true;
    try {
      says.textContent = "Looking for it";
      const number = await invoke("pairing_number", { code: input.value });
      put(says, "Approve it there. It shows ", h("b", {}, number));
      paired(main, await invoke("join_device", { code: input.value }));
    } catch (e) {
      says.textContent = String(e);
      says.classList.add("warn");
      input.disabled = false;
      input.focus();
    }
  };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") join(); });
  // As the sixth digit is typed, as on the phone.
  input.addEventListener("input", () => { says.classList.remove("warn"); if (/^\d{6}$/.test(input.value.trim())) join(); });
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead("Enter a code", { words: "Add a device", go: () => go("add") }),
    h("div", { class: "pair" }, input, says)));
  input.focus();
}

/** The new device, arrived: its circle, as it will be in the panel. */
function paired(main, s) {
  put(main, h("div", { class: "pg pg-narrow" },
    h("div", { class: "pair arrived" },
      circle({ name: s.name, route: "direct" }, "circle-xl"),
      h("h1", {}, s.name),
      h("p", { class: "note" }, "Added"),
      btn("Done", () => go("home"), "btn-main"))));
  listsHome();
}
