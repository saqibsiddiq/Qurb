// Devices, reached through Home's circle: a device's page, choosing a
// phone's folders, adding a device or a person, and removing one.

// ---------- A device's page ----------

function drawDevicePage(main, d) {
  HOME.page = d;
  const own = d.relation === "own";
  main.append(
    h("div", { class: "page" },
      h("button", { class: "back", onclick: () => go("home") }, icon("arrow-left"), "Home"),
      h("div", { class: "page-hd" },
        circle(d, "circle-xl"),
        h("h1", {}, d.name),
        h("p", { class: `state ${d.route ? "on" : "off"}` }, d.route ? (d.route === "relay" ? "Online, through the relay" : "Online") : d.last_seen ? `Off, seen ${ago(d.last_seen)}` : "Not connected yet")),
      own && chosenDevice()?.fingerprint !== d.fingerprint
        ? h("button", { class: "btn btn-quiet", onclick: () => { choose(d); go("device", d); } }, icon("send"), "Send here")
        : null,
      h("div", { class: "page-list", id: "page-list" }),
      h("details", { class: "tech" },
        h("summary", {}, "Details"),
        h("dl", {},
          h("dt", {}, "Identity"), h("dd", {}, d.fingerprint),
          h("dt", {}, "Paired"), h("dd", {}, ago(d.paired_at)),
          h("dt", {}, "Path"), h("dd", {}, d.route === "relay" ? "Relayed" : d.route === "direct" ? "Direct" : "Not connected"),
          d.address ? [h("dt", {}, "Address"), h("dd", {}, d.address)] : null)),
      h("button", { class: "remove", onclick: () => askToRemove(d) }, d.relation === "guest" ? "Remove this person" : "Remove this device")));
  listsDevicePage();
}

async function listsDevicePage() {
  const d = HOME.page;
  const box = $("page-list");
  if (!d || !box) return;
  const all = await invoke("devices").catch(() => []);
  const out = await invoke("outgoing").catch(() => []);
  const rows = [];
  if (d.whatsapp) rows.push(listRow("whatsapp", "WhatsApp", `${count(d.whatsapp.files, "file")} kept here`));
  if (d.kind === "phone" && d.relation === "own") {
    const f = await invoke("phone_folders", { phone: d.fingerprint }).catch(() => null);
    const chosen = (f?.folders ?? []).filter((x) => x.chosen);
    for (const x of chosen) {
      rows.push(listRow("folder", x.path, x.kept_files >= x.files ? `${count(x.files, "file")} kept here` : `${x.kept_files.toLocaleString()} of ${count(x.files, "file")} kept here`, x.files ? x.kept_files / x.files : 1));
    }
    rows.push(h("button", { class: "row", onclick: () => go("phone-folders", d) }, tile("sliders-horizontal"),
      h("span", { class: "row-main" }, h("b", {}, chosen.length ? "Choose folders" : "Back up its folders")), icon("chevron-right", "row-go")));
  }
  if (d.relation === "guest") {
    rows.push(h("button", { class: "row", onclick: () => openGuest(d) }, tile("lock-keyhole"),
      h("span", { class: "row-main" }, h("b", {}, "Their folder"), h("small", {}, Number(d.kept ?? 0) > 0 ? `${size(d.kept)}, sealed` : "Nothing kept yet")), icon("chevron-right", "row-go")));
  }
  // What it was before it was set up again (item 6), and a way to let it go.
  const earlier = d.earlier ? all.find((e) => e.id === d.earlier) : null;
  if (earlier) {
    rows.push(h("div", { class: "row" }, tile("history"),
      h("span", { class: "row-main" }, h("b", {}, "From before it was set up again"), h("small", {}, earlier.held ? `${count(earlier.held.files, "file")}, ${size(earlier.held.bytes)}` : "Nothing kept")),
      h("button", { class: "btn btn-quiet btn-sm", onclick: () => askToRemove(earlier, `the earlier ${earlier.name}`) }, "Remove")));
  }
  const waiting = out.filter((o) => o.to_id === d.id);
  put(box, 
    ...(rows.length ? [label(d.relation === "guest" ? "Kept here" : "Backed up here"), ...rows] : []),
    ...(waiting.length ? [label("Waiting for it"), ...waiting.map((o) => listRow(fileIcon(o.path), base(o.path), size(o.size)))] : []));
}

function listRow(ic, title, sub, progress) {
  return h("div", { class: "row" }, tile(ic),
    h("span", { class: "row-main" }, h("b", {}, title), h("small", {}, sub),
      progress != null && progress < 1 ? h("span", { class: "w-bar" }, h("i", { style: { width: `${Math.round(progress * 100)}%` } })) : null));
}

/** A guest's folder is a person's folder in Files (decision 0067). */
async function openGuest(d) {
  const people = await invoke("people").catch(() => []);
  const p = people.find((x) => x.relation === "guest" && (x.devices ?? []).includes(d.name)) ?? people.find((x) => x.id.startsWith(d.fingerprint));
  if (p) inFiles(p.id);
  else toast("Nothing kept for them yet");
}

// ---------- A phone's folders (decision 0063) ----------

async function drawPhoneFolders(main, d) {
  const body = h("div", {});
  main.append(h("div", { class: "pg" }, pageHead("Back up folders", { words: d.name, go: () => go("device", d) }), body));
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
async function askToRemove(d, called) {
  let plan;
  try { plan = await invoke("removal_plan", { device: d.fingerprint }); } catch (e) { toast(String(e), true); return; }
  const box = sheet({ ic: circle(d, "circle-lg"), title: `Remove ${called ?? plan.name}?`, sub: "It can't sync here any more. Nothing on it is deleted" });
  const lines = [];
  if (plan.waiting > 0) lines.push(row({ ic: "send", name: `${count(plan.waiting, "send")} cancelled` }));
  if (plan.holds_ours) lines.push(row({ ic: "lock-keyhole", name: "It stops keeping your Vault" }));
  if (plan.only_there.length) {
    lines.push(row({
      ic: tile("triangle-alert", "tile-warn"), cls: "row-warn",
      name: `${count(plan.only_there.length, "file")} only on it`,
      end: h("button", { class: "btn btn-quiet btn-sm", onclick: async (e) => {
        for (const path of plan.only_there) { try { await invoke("fetch", { path }); wanted.add(path); } catch (er) { /* said in Activity */ } }
        e.currentTarget.replaceWith(h("small", { class: "note" }, "Asked for"));
      } }, "Keep here first"),
    }));
  }
  let deleteKept = null;
  if (plan.kept > 0) {
    deleteKept = false;
    lines.push(setting({ ic: "trash-2", name: `Also delete what's kept for it (${size(plan.kept_bytes)})`, end: toggle(false, (on) => { deleteKept = on; }, "Also delete what's kept for it") }));
  }
  box.append(...lines,
    h("p", { class: "note" }, "Only on this computer"),
    actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Remove", async (e) => {
      e.currentTarget.disabled = true;
      try {
        await invoke("remove_device", { device: d.fingerprint, deleteKept: deleteKept ?? false });
        box.close();
        toast(`Removed ${plan.name}`);
        go("home");
      } catch (er) { e.currentTarget.disabled = false; toast(String(er), true); }
    }, "btn-danger")));
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

function drawAdd(main) {
  const choice = (ic, words, sub, act) => h("button", { class: "choice", onclick: act },
    h("span", { class: "choice-c" }, icon(ic)), h("b", {}, words), h("small", {}, sub));
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead("Add a device", { words: "Home", go: () => go("home") }),
    h("div", { class: "choices" },
      choice("qr-code", "Show a code", "Scan it with your phone", () => go("pair-show", { guest: false })),
      choice("keyboard", "Enter a code", "Six digits from the other device", () => go("pair-enter")),
      choice("users", "Add a person", "Someone else's phone, as a guest", () => go("pair-show", { guest: true })))));
}

async function drawShowCode(main, { guest }) {
  const stage = h("div", { class: "pair" }, h("span", { class: "empty-orb asking" }, icon("qr-code")));
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead(guest ? "Add a person" : "Show a code", { words: "Add a device", go: () => { stopPairing(); go("add"); } }),
    stage));
  let inv;
  try { inv = await invoke("start_pairing", { guest }); } catch (e) { put(stage, oops(e)); return; }
  const qr = h("div", { class: "qr" });
  if (inv.qr) qr.innerHTML = inv.qr;
  else qr.append(icon("qr-code"));
  const digits = h("p", { class: "digits" }, inv.digits ?? "");
  const left = h("p", { class: "count" });
  const answer = h("div", {});
  const whole = h("details", { class: "tech" }, h("summary", {}, "The whole code"),
    h("pre", { class: "code" }, guest ? `qurb visit ${inv.code}` : `qurb join <folder> ${inv.code}`),
    h("pre", { class: "code" }, inv.spoken));
  put(stage, qr, h("p", { class: "note" }, guest ? "On their phone, on this Wi-Fi" : "Or type these on your phone, on this Wi-Fi"), digits, left, answer, whole);

  let asked = null;
  const follow = async () => {
    let s;
    try { s = await invoke("pairing_state"); } catch (e) { left.textContent = String(e); return; }
    if (place !== "pair-show") { stopPairing(); return; }
    if (s.state === "asking") {
      const key = `${s.name}|${s.number}`;
      if (asked === key) return;
      asked = key;
      qr.classList.add("dim");
      put(answer, h("div", { class: "asking-box" },
        h("b", {}, s.guest ? `${s.name} wants to visit` : `${s.name} wants to join`),
        h("small", {}, "Approve only if it shows"),
        h("p", { class: "digits" }, s.number),
        actions(btn("Decline", () => reply(false), "btn-quiet"), btn("Approve", () => reply(true), "btn-main"))));
      return;
    }
    if (s.state === "waiting") {
      if (asked) { asked = null; put(answer); qr.classList.remove("dim"); }
      if (s.digits_withdrawn) digits.classList.add("withdrawn");
      const secs = Math.max(0, inv.expires_at - Math.floor(Date.now() / 1000));
      left.textContent = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
      return;
    }
    clearInterval(pairingWatch);
    pairingWatch = null;
    if (s.state === "paired") paired(main, s);
    else left.textContent = s.state === "expired" ? "Expired. Show a new one" : (s.message ?? "Didn't work");
  };
  const reply = async (yes) => {
    answer.querySelectorAll("button").forEach((b) => (b.disabled = true));
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
