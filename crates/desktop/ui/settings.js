// Settings, as the phone groups them — This computer, Files, App,
// Connection — each a row with its value, a switch, or an arrow to its own
// page: Storage, the key, Activity, Advanced.

/** The settings as last read: saving one sends them all back. */
let SETTINGS = null;

async function drawSettings(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Settings", null), body));
  let s, st, atLogin;
  try {
    [s, st, atLogin] = await Promise.all([invoke("settings"), invoke("storage").catch(() => null), invoke("starts_at_login").catch(() => false)]);
  } catch (e) { put(body, oops(e)); return; }
  SETTINGS = s;
  const others = s.key_also_on ?? [];
  const theme = { system: "System", light: "Light", dark: "Dark" }[window.qurbTheme?.chosen?.() ?? "system"];
  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  put(body, 
    h("div", { class: "me" }, circle({ name: s.name, picture: s.picture, route: "direct" }, "circle-xl"), h("h2", {}, s.name)),
    group("This computer",
      setting({ ic: "laptop", name: "Name", value: s.name, onclick: askName }),
      setting({ ic: "image", name: "Picture", end: circle({ name: s.name, picture: s.picture, route: "direct" }, "circle-md circle-plain"), onclick: () => pictureSheet(s) }),
      setting({ ic: "shield-check", name: "Key", value: others.length ? "Safe" : "Only here", onclick: () => go("key") }),
      setting({ ic: "refresh-cw", name: "Start at login", end: toggle(atLogin, (on) => invoke("set_starts_at_login", { on }), "Start at login") }),
      setting({ ic: "hard-drive", name: "Back up folders", onclick: () => go("backup") })),
    group("Files",
      setting({ ic: "lock-keyhole", name: "New files go to the Vault", end: toggle(s.own_files_private, (on) => invoke("set_privacy", { private: on }), "New files go to the Vault") }),
      setting({ ic: "download", name: "Received files", value: downloadsWords(s), onclick: askDownloads }),
      setting({ ic: "hard-drive", name: "Storage", value: st ? size(st.used) : null, onclick: () => go("storage") }),
      setting({ ic: "trash-2", name: "Recently deleted", onclick: () => go("deleted") })),
    group("App",
      setting({ ic: "clock", name: "Activity", onclick: () => go("activity") }),
      setting({ ic: "bell", name: "Notifications", end: toggle(s.notifications, (on) => invoke("set_notifications", { on }), "Notifications") }),
      setting({ ic: "palette", name: "Theme", value: theme, onclick: themeSheet }),
      setting({ ic: "sliders-horizontal", name: "Motion", value: calm ? "Reduced" : "Full" })),
    group("Connection",
      setting({ ic: "arrow-up-down", name: "Speed limit", value: speedWords(s), onclick: () => go("speed") }),
      setting({ ic: "link", name: "Advanced", onclick: () => go("advanced") })),
    group(null, setting({ ic: "x", name: "Quit QurbSpace", danger: true, onclick: () => invoke("quit") })));
}

function downloadsWords(s) {
  const at = s.downloads_at ?? {};
  if ("Err" in at) return "Refused";
  return at.Ok ? base(at.Ok) : "In your QurbSpace folder";
}

/** Saves the settings with one changed. */
async function saveSettings(change) {
  const s = { ...SETTINGS, ...change };
  await invoke("save_settings", { name: s.name, signal: s.signal, relay: s.relay ?? "", port: Number(s.port) || 0, downloads: s.downloads });
}

/** One field in a sheet, saved by Enter or Save. */
function askField({ ic, title, value, placeholder, note, save }) {
  const box = sheet({ ic, title });
  const input = h("input", { class: "field", value, placeholder, spellcheck: "false" });
  const says = h("p", { class: "note" }, note ?? "");
  const done = async () => {
    try { await save(input.value.trim()); box.close(); if (place === "settings") go("settings"); }
    catch (e) { says.textContent = String(e); says.classList.add("warn"); }
  };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") done(); });
  box.append(input, says, actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Save", done, "btn-main")));
  input.focus();
  input.select();
}

function askName() {
  askField({ ic: "laptop", title: "Name", value: SETTINGS.name, note: "As your other devices show it", save: (name) => saveSettings({ name }) });
}

function askDownloads() {
  const at = SETTINGS.downloads_at ?? {};
  askField({
    ic: "download", title: "Received files", value: SETTINGS.downloads, placeholder: at.Ok ?? "",
    note: "Err" in at ? at.Err : "Where files sent to this computer go",
    save: (downloads) => saveSettings({ downloads }),
  });
}

function themeSheet() {
  const box = sheet({ ic: "palette", title: "Theme" });
  const now = window.qurbTheme.chosen();
  rounds(box, [["system", "System"], ["light", "Light"], ["dark", "Dark"]].map(([to, words]) => ({
    ic: to === now ? "check" : "palette", words, act: () => { window.qurbTheme.choose(to); go("settings"); },
  })));
}

/** This computer's picture, round, as its other devices show it. */
function pictureSheet(s) {
  const file = h("input", { type: "file", accept: "image/*", style: { display: "none" } });
  file.addEventListener("change", async () => {
    const f = file.files?.[0];
    if (!f) return;
    try { await invoke("set_picture", { picture: await squareJpeg(f) }); go("settings"); } catch (e) { toast(String(e), true); }
  });
  document.body.append(file);
  const box = sheet({ ic: circle({ name: s.name, picture: s.picture, route: "direct" }, "circle-lg circle-plain"), title: "Picture", onclose: () => setTimeout(() => file.remove(), 60000) });
  rounds(box, [
    { ic: "image", words: s.picture ? "Change" : "Choose", act: () => file.click() },
    s.picture ? { ic: "trash-2", words: "Remove", danger: true, act: async () => { await invoke("set_picture", { picture: null }).catch((e) => toast(String(e), true)); go("settings"); } } : null,
  ]);
}

/** The middle square of a picture, 256 across, as JPEG bytes: what the phone
 * makes of one too. */
async function squareJpeg(file) {
  const address = await new Promise((done, fail) => {
    const r = new FileReader();
    r.onload = () => done(r.result);
    r.onerror = () => fail(new Error("could not read that picture"));
    r.readAsDataURL(file);
  });
  const img = new Image();
  await new Promise((done, fail) => { img.onload = done; img.onerror = () => fail(new Error("that isn't a picture")); img.src = address; });
  const side = Math.min(img.naturalWidth, img.naturalHeight);
  const c = document.createElement("canvas");
  c.width = c.height = 256;
  c.getContext("2d").drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, 256, 256);
  const blob = await new Promise((done, fail) => c.toBlob((b) => (b ? done(b) : fail(new Error("could not make the picture"))), "image/jpeg", 0.85));
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}

// ---------- Storage ----------

const GIB = 1024 ** 3;

async function drawStorage(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Storage", { words: "Settings", go: () => go("settings") }), body));
  let st, fr, parts, alone;
  try {
    [st, fr, parts, alone] = await Promise.all([invoke("storage"), invoke("freeable"), invoke("storage_parts"), invoke("not_elsewhere").catch(() => null)]);
  } catch (e) { put(body, oops(e)); return; }
  const used = Number(st.used), freeable = Number(fr.bytes), free = Number(st.free_disk), limit = Number(st.limit), disk = Number(st.disk);
  const whole = Math.max(limit > 0 ? limit : Math.max(disk, used + free), used, 1);
  const pct = (n) => `${Math.max(0, (100 * n) / whole)}%`;

  // The limit: a switch, and a track that is square-law, so that the small
  // amounts people choose get most of it (a 500 GB disk against a 20 GB
  // allowance would otherwise sit in its first four percent).
  const maxGib = Math.max(1, Math.floor(disk / GIB));
  const toTrack = (gib) => Math.round(Math.sqrt(Math.min(gib, maxGib) / maxGib) * 1000);
  const fromTrack = (v) => Math.max(1, Math.round((v / 1000) ** 2 * maxGib));
  const track = h("input", { type: "range", min: "0", max: "1000", class: "track", "aria-label": "Limit" });
  track.value = String(toTrack(limit > 0 ? limit / GIB : maxGib / 10));
  const amount = h("b", { class: "track-v" });
  // Said as every other size is, so that the limit chosen and the limit shown
  // above are the same number.
  const showAmount = () => { amount.textContent = size(fromTrack(Number(track.value)) * GIB); };
  track.addEventListener("input", showAmount);
  showAmount();
  const limitBox = h("div", { class: "limit", hidden: limit === 0 }, track, amount,
    btn("Apply", async () => { await invoke("set_limit", { bytes: String(fromTrack(Number(track.value)) * GIB) }).catch((e) => toast(String(e), true)); go("storage"); }, "btn-main btn-sm"));

  // Each file once: a phone's WhatsApp inside a backed-up folder is stored once.
  const phoneBytes = Number(parts.phones_once ?? parts.phones.reduce((n, p) => n + Number(p.whatsapp) + Number(p.folders) + Number(p.vault), 0));
  const guestBytes = parts.guests.reduce((n, g) => n + Number(g.size), 0);
  put(body, 
    h("div", { class: "used" },
      h("p", { class: "used-big" }, size(st.used)),
      h("p", { class: "note" }, limit > 0 ? `of ${size(st.limit)}${st.over ? ", over" : ""}` : `${size(st.free_disk)} free on this disk`),
      h("div", { class: "meter" }, h("i", { class: "m-used", style: { width: pct(used - freeable) } }), h("i", { class: "m-free", style: { width: pct(freeable) } })),
      h("div", { class: "chips" },
        h("span", { class: "chip chip-quiet" }, h("i", { class: "dot d-used" }), "Used"),
        freeable ? h("span", { class: "chip chip-quiet" }, h("i", { class: "dot d-free" }), `${size(fr.bytes)} can be freed`) : null),
      freeable ? btn(`Free up ${size(fr.bytes)}`, (e) => freeAll(e.currentTarget), "btn-main") : null),
    group("Kept here",
      setting({ ic: "smartphone", name: "Your phones' backups", value: size(phoneBytes), onclick: () => inFiles("owner") }),
      setting({ ic: "folder", name: "Shared", value: size(parts.shared_here), onclick: () => inFiles("shared") }),
      parts.guests.length ? setting({ ic: "users", name: "People's folders", value: size(guestBytes) }) : null,
      setting({ ic: "trash-2", name: "Recently deleted", value: size(parts.deleted), onclick: () => go("deleted") }),
      // Made here, and on no other device yet (component 5: storage's, not
      // Home's Waiting).
      alone?.files ? setting({ ic: "laptop", name: "Only on this computer", value: count(alone.files, "file"), onclick: () => onlyHere(alone) }) : null),
    group("Limit",
      setting({ ic: "sliders-horizontal", name: "Limit QurbSpace's space", end: toggle(limit > 0, async (on) => {
        limitBox.hidden = !on;
        if (!on) { await invoke("set_limit", { bytes: "0" }); go("storage"); }
      }, "Limit QurbSpace's space") }),
      limitBox),
    fr.files.length ? label("Largest you can free") : null,
    fr.files.length ? h("div", { class: "rows" }, ...fr.files.slice(0, 5).map((f) => row({
      ic: fileIcon(f.path), name: base(f.path), sub: [size(f.size), folderOf(f.path) || "Shared"],
      end: h("button", { class: "btn btn-quiet btn-sm", onclick: async () => { await freeUp(f.path); go("storage"); } }, "Free up"),
    }))) : null,
    h("p", { class: "note" }, "Freed files stay in QurbSpace, on your other devices"));
}

/** The files made here that no other device has yet: the first few. */
function onlyHere(alone) {
  const box = sheet({ ic: "laptop", title: "Only on this computer", sub: `${count(alone.files, "file")}, ${size(alone.size)}` });
  box.append(h("div", { class: "rows" }, ...alone.first.map((f) => row({ ic: fileIcon(f.path), name: base(f.path), sub: [size(f.size), folderOf(f.path) || "Shared"] }))),
    h("p", { class: "note" }, "They reach your other devices when one is online"));
}

async function freeAll(b) {
  b.disabled = true;
  let freed = 0;
  try {
    // A page at a time, largest first, until nothing more can go.
    for (let round = 0; round < 20; round++) {
      const fr = await invoke("freeable");
      if (!fr.files.length) break;
      for (const f of fr.files) { try { freed += Number(await invoke("free_file", { path: f.path })); } catch (e) { /* the only copy */ } }
    }
    toast(`Freed ${size(freed)}`);
  } finally { go("storage"); }
}

// ---------- The key ----------

async function drawKey(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Key", { words: "Settings", go: () => go("settings") }), body));
  let s, sec;
  try { [s, sec] = await Promise.all([invoke("settings"), invoke("security")]); } catch (e) { put(body, oops(e)); return; }
  const others = s.key_also_on ?? [];
  const words = h("div", {});
  const showWords = async (b) => {
    if (words.childElementCount) { put(words); b.textContent = "Show"; return; }
    try {
      const list = await invoke("reveal_phrase");
      put(words, h("ol", { class: "words" }, ...list.map((w) => h("li", {}, w))));
      b.textContent = "Hide";
    } catch (e) { put(words, oops(e)); }
  };
  const how = { file: "In a file", keystore: "System keystore", passphrase: "Passphrase" };
  put(body, 
    h("div", { class: "locked" },
      h("span", { class: `empty-orb${others.length ? " ok" : " warn"}` }, icon(others.length ? "shield-check" : "triangle-alert")),
      h("p", {}, others.length ? `Also on ${others.join(", ")}` : "Only on this computer"),
      h("p", { class: "note" }, others.length ? "Any of them gives it to a new device with a code" : "Pair your phone: it keeps the key safe")),
    group("Protected",
      ...["file", "keystore", "passphrase"].map((to) => setting({
        ic: { file: "file", keystore: "key-round", passphrase: "lock-keyhole" }[to],
        name: how[to],
        end: sec.protection === to ? icon("check", "set-tick") : null,
        onclick: sec.protection === to && to !== "passphrase" ? null : () => protectSheet(to, sec.protection),
      }))),
    group("Your 24 words",
      setting({ ic: "key-round", name: "Show them", end: btn("Show", (e) => showWords(e.currentTarget), "btn-quiet btn-sm") }),
      words),
    group("Identity", setting({ ic: "info", name: sec.identity })),
    sec.events.length ? label("Paired and removed") : null,
    sec.events.length ? h("div", { class: "rows" }, ...sec.events.slice(0, 8).map((r) => row({
      ic: r.kind === "removed" ? "x" : "monitor-smartphone", name: r.device ?? r.detail ?? "", sub: [r.kind === "removed" ? "Removed" : "Paired", ago(r.at)],
    }))) : null);
}

/** Changing how the key is kept: the fields that change needs, no more. */
function protectSheet(to, was) {
  const box = sheet({ ic: "lock-keyhole", title: to === "passphrase" ? (was === "passphrase" ? "Change passphrase" : "Protect with a passphrase") : to === "keystore" ? "Use the keystore" : "Keep in a file" });
  const field = (placeholder) => h("input", { class: "field", type: "password", placeholder });
  const current = was === "passphrase" ? field("Current passphrase") : null;
  const fresh = to === "passphrase" ? field("New passphrase") : null;
  const again = to === "passphrase" ? field("Again") : null;
  const says = h("p", { class: "note" }, to === "passphrase" ? "Asked for when QurbSpace starts. Nothing syncs until it's typed" : to === "keystore" ? "Locked while you're logged out" : "Enough against other people using this computer");
  box.append(...[current, fresh, again].filter(Boolean), says, actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Change", async () => {
    if (fresh && fresh.value !== again.value) { says.textContent = "The two aren't the same"; says.classList.add("warn"); return; }
    try {
      await invoke("protect_key", { to, current: current?.value || null, new: fresh?.value ?? null });
      box.close();
      toast("Changed");
      go("key");
    } catch (e) { says.textContent = String(e); says.classList.add("warn"); }
  }, "btn-main")));
  (current ?? fresh)?.focus();
}

// ---------- Speed limit ----------

/** The rates a person picks from, bytes a second; 0 for none. */
const SPEEDS = [0, 1e6, 5e6, 10e6, 25e6];
const speed = (rate) => (Number(rate) ? `${size(rate)}/s` : "None");
function speedWords(s) {
  const [up, down] = [Number(s.upload_limit ?? 0), Number(s.download_limit ?? 0)];
  return up || down ? [up ? `${speed(up)} up` : null, down ? `${speed(down)} down` : null].filter(Boolean).join(", ") : "None";
}

/** How fast files may go to and come from other devices: chips for each
 * way, applied at once. */
async function drawSpeed(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Speed limit", { words: "Settings", go: () => go("settings") }), body));
  let s;
  try { s = await invoke("settings"); } catch (e) { put(body, oops(e)); return; }
  const rates = { up: String(s.upload_limit ?? "0"), down: String(s.download_limit ?? "0") };
  const way = (key, ic, words) => {
    const chips = h("div", { class: "amounts amounts-l" });
    const draw = () => put(chips, ...SPEEDS.map((r) => h("button", {
      class: `amount${Number(rates[key]) === r ? " on" : ""}`,
      onclick: async () => {
        const was = rates[key];
        rates[key] = String(r);
        draw();
        try { await invoke("set_bandwidth", rates); } catch (e) { rates[key] = was; draw(); toast(String(e), true); }
      },
    }, speed(r))));
    draw();
    return h("section", { class: "grp" }, label(words), h("div", { class: "speed" }, icon(ic, "set-ic"), chips));
  };
  put(body,
    way("up", "arrow-up", "Sending to your devices"),
    way("down", "arrow-down", "Receiving from them"),
    h("p", { class: "note" }, "Leaves room on your connection for everything else"));
}

// ---------- Backing up this computer's folders ----------

/** Folders of this computer, kept by your other computers (the owner's list
 * of 2026-10-28): which, and by whom. */
async function drawBackup(main) {
  const body = h("div", { id: "backup-body" });
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead("Back up folders", { words: "Settings", go: () => go("settings") },
      h("button", { class: "btn btn-main btn-sm", onclick: addBackupFolder }, icon("plus"), "Add a folder")),
    body));
  await listsBackup();
}

let BACKUPS = null;

async function listsBackup() {
  const body = $("backup-body");
  if (!body) return;
  try { BACKUPS = await invoke("backups"); } catch (e) { put(body, oops(e)); return; }
  const b = BACKUPS;
  const anyOn = b.keepers.some((k) => k.on);
  put(body,
    b.folders.length
      ? h("div", { class: "rows" }, label("Folders"), ...b.folders.map((f) => row({
        ic: f.refused ? tile("triangle-alert", "tile-warn") : "folder",
        name: f.name,
        title: f.path,
        sub: f.refused ? [f.refused] : [f.files ? `${count(f.files, "file")}` : "Not read yet", f.files ? size(f.size) : null],
        cls: f.refused ? "row-warn" : "",
        end: h("button", { class: "icon-btn", "aria-label": `Stop backing up ${f.name}`, title: "Stop backing up", onclick: () => removeBackupFolder(f) }, icon("x")),
      })))
      : empty("hard-drive", "Folders of this computer, kept by your other computers"),
    label("Kept on"),
    b.keepers.length
      ? h("div", { class: "grp-box" }, ...b.keepers.map((k) => setting({
        ic: "laptop",
        name: k.name,
        value: k.online ? null : "Off",
        end: toggle(k.on, async (on) => { await invoke("set_keeper", { device: k.fingerprint, on }); listsBackup(); }, k.name),
      })))
      : h("p", { class: "note" }, "Pair another computer to keep them"),
    b.folders.length && b.keepers.length && !anyOn ? h("p", { class: "note warn" }, "No computer keeps them yet") : null,
    h("p", { class: "note" }, "Files deleted here stay with them. Your Vault goes with them too"));
}

async function addBackupFolder() {
  const dialog = window.__TAURI__?.dialog;
  if (!dialog) { toast("No folder chooser here", true); return; }
  const chosen = await dialog.open({ directory: true, multiple: false });
  if (!chosen) return;
  const folders = [...(BACKUPS?.folders ?? []).map((f) => f.path), Array.isArray(chosen) ? chosen[0] : chosen];
  try { await invoke("set_backup_folders", { folders }); } catch (e) { toast(String(e), true); return; }
  listsBackup();
}

async function removeBackupFolder(f) {
  const folders = (BACKUPS?.folders ?? []).map((x) => x.path).filter((p) => p !== f.path);
  try { await invoke("set_backup_folders", { folders }); toast("What's kept already stays kept"); } catch (e) { toast(String(e), true); }
  listsBackup();
}

// ---------- Advanced ----------

async function drawAdvanced(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Advanced", { words: "Settings", go: () => go("settings") }), body));
  let s;
  try { s = await invoke("settings"); } catch (e) { put(body, oops(e)); return; }
  SETTINGS = s;
  const field = (name, value, placeholder) => h("label", { class: "fld" }, h("span", {}, name), h("input", { class: "field", value: value ?? "", placeholder, spellcheck: "false" }));
  const signal = field("Rendezvous", s.signal);
  const relay = field("Relay", s.relay, "None");
  const port = field("Port", String(s.port), "Any");
  const says = h("p", { class: "note" });
  put(body, 
    group("Connection", signal, relay, port),
    actions(btn("Save", async () => {
      try {
        await saveSettings({ signal: signal.querySelector("input").value, relay: relay.querySelector("input").value, port: port.querySelector("input").value });
        toast("Saved");
      } catch (e) { says.textContent = String(e); says.classList.add("warn"); }
    }, "btn-main")),
    says,
    group("This computer",
      setting({ ic: "folder-open", name: "Folder", value: s.root, onclick: () => invoke("show_root").catch((e) => toast(String(e), true)) }),
      setting({ ic: "info", name: "Version", value: s.version.split(" · ")[0].replace("window ", "") })));
}

// ---------- Activity ----------

const HAPPENED = {
  stored: ["hard-drive", "Saved here"],
  received: ["download", "Arrived"],
  deleted: ["trash-2", "Deleted"],
  sent: ["send", "Ready to send"],
  collected: ["circle-check", "Collected"],
  evicted: ["cloud-off", "Freed here"],
  restored: ["rotate-ccw", "Restored"],
  conflicted: ["git-compare", "Two versions"],
  paired: ["monitor-smartphone", "Paired"],
  failed: ["circle-alert", "Didn't finish"],
  cancelled: ["x", "Stopped"],
  removed: ["x", "Removed"],
};

async function drawActivity(main) {
  const list = h("div", { class: "rows" });
  const older = btn("Older", () => more(), "btn-quiet");
  main.append(h("div", { class: "pg" }, pageHead("Activity", { words: "Settings", go: () => go("settings") }), list, h("div", { class: "acts acts-c" }, older)));
  let oldest = null;
  const more = async () => {
    older.disabled = true;
    try {
      const rows = await invoke("activity", { path: null, limit: 60, before: oldest });
      if (!rows.length && oldest === null) list.append(empty("history", "Nothing yet"));
      oldest = rows.length ? rows[rows.length - 1].id : oldest;
      older.hidden = rows.length < 60;
      for (const r of rows) {
        const [ic, words] = HAPPENED[r.kind] ?? ["info", r.kind];
        list.append(row({
          ic: tile(ic, r.kind === "failed" ? "tile-warn" : ""),
          name: r.path ? base(r.path) : (r.device ?? ""),
          sub: [r.device && r.path ? `${words}, ${r.device}` : words, ago(r.at), r.size ? size(r.size) : null],
          title: r.detail && r.detail !== r.device ? r.detail : undefined,
        }));
      }
    } catch (e) { list.append(oops(e)); }
    older.disabled = false;
  };
  more();
}
