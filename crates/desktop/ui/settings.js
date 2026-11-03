// Settings, in the phone's shape (the owner's review of 2026-11-03, S-1 to
// S-5): this computer as one profile card; Manage storage; App; Connection;
// Quit in red at the end. The key, the identity, where received files go and
// the meeting point are in Advanced, as on the phone. Backing up this
// computer's folders is on the page of the computer that keeps them (S-4),
// and *New files go to the Vault* is gone (S-5): where a file is put says it.

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
  const theme = { system: "System", light: "Light", dark: "Dark" }[window.qurbTheme?.chosen?.() ?? "system"];
  put(body,
    // Name and picture as one, as WhatsApp and the phone have them (SE-2).
    h("button", { class: "profile", onclick: () => profileSheet(s) },
      circle({ name: s.name, picture: s.picture, route: "direct" }, "circle-xl circle-plain"),
      h("span", { class: "profile-words" }, h("b", {}, s.name), h("small", {}, "Name and picture")),
      icon("chevron-right", "set-go")),
    group("Manage storage",
      setting({ ic: "hard-drive", name: "This computer", value: st ? size(st.used) : null, onclick: () => go("storage") })),
    group("App",
      setting({ ic: "clock", name: "Activity", onclick: () => go("activity") }),
      setting({ ic: "bell", name: "Notifications", end: toggle(s.notifications, (on) => invoke("set_notifications", { on }), "Notifications") }),
      setting({ ic: "palette", name: "Theme", value: theme, onclick: themeSheet }),
      setting({ ic: "refresh-cw", name: "Start at login", end: toggle(atLogin, (on) => invoke("set_starts_at_login", { on }), "Start at login") }),
      setting({ ic: "download", name: "Update", end: h("span", { id: "version-says", class: "chip chip-ok" }, " "), onclick: updateClicked })),
    group("Connection",
      setting({ ic: "arrow-up-down", name: "Speed limit", value: speedWords(s), onclick: () => go("speed") }),
      setting({ ic: "link", name: "Advanced", onclick: () => go("advanced") })),
    h("button", { class: "big-btn big-red", onclick: () => invoke("quit") }, "Quit QurbSpace"));
  showUpdate();
}

/** This computer's name and picture, together. */
function profileSheet(s) {
  const box = sheet({ ic: circle({ name: s.name, picture: s.picture, route: "direct" }, "circle-lg circle-plain"), title: s.name });
  box.append(h("div", { class: "grp-box" },
    setting({ ic: "laptop", name: "Name", value: s.name, onclick: () => { box.close(); askName(); } }),
    setting({ ic: "image", name: "Picture", onclick: () => { box.close(); pictureSheet(s); } })));
}

// ---------- Updates (decision 0078) ----------

/** Where updating stands, in a few words. */
function updateWords(u) {
  switch (u?.state) {
    case "checking": return "Looking…";
    case "downloading": return `Fetching ${u.version} ${Math.round((u.progress ?? 0) * 100)}%`;
    case "installed":
    case "ready": return "Restart";
    case "manual": return `${u.version} out`;
    case "failed": return "Couldn't check";
    default: return "Up to date";
  }
}

async function showUpdate() {
  const u = await invoke("update_state").catch(() => null);
  const says = $("version-says");
  if (!says) return;
  says.textContent = updateWords(u);
  // The chip says where it stands; ready, it is the button (N-2). Without
  // anybody pressing it the update is put in place once the window is
  // closed and nothing moves (updates.rs).
  const ready = u?.state === "installed" || u?.state === "ready";
  says.className = `chip ${ready ? "chip-do" : u?.state === "manual" || u?.state === "failed" ? "chip-wait" : "chip-ok"}`;
}

async function applyUpdate(u) {
  try { await invoke("apply_update"); } catch (e) { toast(String(e), true); }
}

async function updateClicked() {
  const u = await invoke("update_state").catch(() => null);
  if (u?.state === "installed" || u?.state === "ready") return applyUpdate(u);
  if (u?.state === "manual") { toast("Update it with your package manager, or install the AppImage"); return; }
  $("version-says").textContent = "Looking…";
  const now = await invoke("update_now").catch(() => null);
  showUpdate();
  if (now?.state === "idle") toast("Up to date");
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

// This computer's storage (S-2): its disk's free space, one bar coloured by
// what fills it, and a row for each part. No limit and nothing to free: a
// computer keeps everything (F-3). Recently deleted lives here (F-1).
async function drawStorage(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("This computer", { words: "Settings", go: () => go("settings") }), body));
  let st, parts;
  try { [st, parts] = await Promise.all([invoke("storage"), invoke("storage_parts")]); } catch (e) { put(body, oops(e)); return; }
  const disk = Number(st.disk), free = Number(st.free_disk);
  const phoneBytes = Number(parts.phones_once ?? parts.phones.reduce((n, p) => n + Number(p.whatsapp) + Number(p.folders) + Number(p.vault), 0));
  const kinds = [
    { name: "Shared", bytes: Number(parts.shared_here), colour: "var(--green)" },
    { name: SETTINGS?.name ? "Your phones" : "Your phones", bytes: phoneBytes, colour: "var(--blue)", lead: "smartphone" },
    ...parts.guests.map((g) => ({ name: g.name, bytes: Number(g.size), colour: "#d8a0b8", lead: "user-circle" })),
    { name: "Recently deleted", bytes: Number(parts.deleted), colour: "var(--warn)", lead: "trash-2", open: () => go("deleted") },
  ];
  const whole = Math.max(disk, 1);
  put(body,
    h("div", { class: "disk" },
      h("p", { class: "used-big" }, `${size(free)} free`),
      h("p", { class: "note" }, `of ${size(disk)}`),
      h("div", { class: "disk-bar" }, ...kinds.filter((k) => k.bytes > 0).map((k) =>
        h("i", { style: { width: `${Math.max(0.6, (100 * k.bytes) / whole)}%`, background: k.colour }, title: k.name })))),
    h("div", { class: "grp-box" }, ...kinds.map((k) => {
      const lead = k.lead ? icon(k.lead, "set-ic") : h("i", { class: "sw", style: { background: k.colour } });
      return h(k.open ? "button" : "div", { class: "set", onclick: k.open },
        lead, h("span", { class: "set-name" }, k.name), h("span", { class: "set-value" }, size(k.bytes)),
        k.open ? icon("chevron-right", "set-go") : null);
    })));
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
const speed = (rate) => (Number(rate) ? `${size(rate)}/s` : "Off");
function speedWords(s) {
  const [up, down] = [Number(s.upload_limit ?? 0), Number(s.download_limit ?? 0)];
  return up || down ? [up ? `${speed(up)} up` : null, down ? `${speed(down)} down` : null].filter(Boolean).join(", ") : "Off";
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
    return h("div", { class: "speed", title: words }, icon(ic, "set-ic"), chips);
  };
  // One card, ↑ and ↓ (K-3): the arrows say which way.
  put(body, h("div", { class: "grp-box" },
    way("up", "arrow-up", "Sending to your devices"),
    way("down", "arrow-down", "Receiving from them")));
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

// Advanced, as the phone's (S-3): this computer's identity, its key, where
// files sent here go; the meeting point and relay; the version.
async function drawAdvanced(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Advanced", { words: "Settings", go: () => go("settings") }), body));
  let s, sec;
  try { [s, sec] = await Promise.all([invoke("settings"), invoke("security")]); } catch (e) { put(body, oops(e)); return; }
  SETTINGS = s;
  const others = s.key_also_on ?? [];
  const meeting = (v) => (!v || v === "nostr" ? "Public relays" : v.startsWith("nostr:") ? "Chosen relays" : v);
  put(body,
    group("This computer",
      setting({ ic: "info", name: "Identity", value: sec.identity.slice(0, 9) }),
      setting({ ic: "shield-check", name: "Key", value: others.length ? "Safe" : "Only here", onclick: () => keySheet(s, sec) }),
      setting({ ic: "download", name: "Received files", value: downloadsWords(s), onclick: askDownloads })),
    group("Connection",
      setting({ ic: "link", name: "Meeting point", value: meeting(s.signal), onclick: () => askField({
        ic: "link", title: "Meeting point", value: s.signal, placeholder: "nostr",
        note: "nostr: the public relays. Or a rendezvous service's address",
        save: (v) => saveSettings({ signal: v || "nostr" }) }) }),
      setting({ ic: "cloud", name: "Relay", value: s.relay || "None", onclick: () => askField({
        ic: "cloud", title: "Relay", value: s.relay ?? "", placeholder: "None",
        save: (v) => saveSettings({ relay: v }) }) })),
    group("App",
      setting({ ic: "info", name: "Version", value: s.version.split(" · ")[0].replace("window ", "") })));
}

/** The phone's key sheet (K-4): safe or not, where this computer keeps it,
 * which devices also have it, and the 24 words, asked first. */
function keySheet(s, sec) {
  const others = s.key_also_on ?? [];
  const how = { file: "In a file", keystore: "System keyring", passphrase: "Passphrase" };
  const box = sheet({});
  box.append(h("div", { class: "key-status" },
      h("span", { class: `empty-orb${others.length ? " ok" : " warn"}` }, icon(others.length ? "shield-check" : "triangle-alert")),
      h("h2", {}, others.length ? "Your key is safe" : "Your key is only here")),
    h("div", { class: "grp-box" },
      setting({ ic: "laptop", name: "This computer", value: how[sec.protection] ?? sec.protection, onclick: () => { box.close(); keptWhere(sec.protection); } }),
      others.length ? setting({ ic: "smartphone", name: "Also on", value: others.join(", ") }) : null),
    actions(btn("Show the 24 words", () => { box.close(); askWords(); }, "btn-quiet")));
}

/** Where the key is kept: a file, the system's keyring, or a passphrase. */
function keptWhere(now) {
  const how = { file: "In a file", keystore: "System keyring", passphrase: "Passphrase" };
  const box = sheet({ ic: "lock-keyhole", title: "Where this computer keeps it" });
  box.append(h("div", { class: "grp-box" }, ...["file", "keystore", "passphrase"].map((to) => setting({
    ic: { file: "file", keystore: "key-round", passphrase: "lock-keyhole" }[to],
    name: how[to],
    end: now === to ? icon("check", "set-tick") : null,
    onclick: now === to && to !== "passphrase" ? null : () => { box.close(); protectSheet(to, now); },
  }))));
}

/** The 24 words, after asking: anyone who sees them can read the files. */
function askWords() {
  const box = sheet({ ic: "key-round", title: "Show your 24 words?", sub: "Anyone who sees them can read every file you keep in QurbSpace" });
  box.append(actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Show", async () => {
    try {
      const list = await invoke("reveal_phrase");
      box.close();
      const shown = sheet({ ic: "key-round", title: "Your 24 words" });
      shown.append(h("ol", { class: "words" }, ...list.map((w) => h("li", {}, w))));
    } catch (e) { box.close(); toast(String(e), true); }
  }, "btn-main")));
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

// Activity, as the phone's (the owner's review of 2026-11-03, Y-1, Y-2):
// chips -- All, Backups, Transfers, Problems --; by day; a batch one row
// ("128 files backed up") with its files' previews and a badge for what
// happened, the other device under it, the time at the end; words only for
// a problem. A row opens its entry.

const ACTIVITY_CHIPS = [
  ["all", "All", () => true],
  ["backups", "Backups", (r) => r.kind === "stored" && (r.path ?? "").startsWith(".qurb-outside/")],
  ["transfers", "Transfers", (r) => ["received", "sent", "collected", "cancelled"].includes(r.kind)],
  ["problems", "Problems", (r) => r.kind === "failed"],
];

/** A row's title: one file's name, or what a batch was. */
function activityTitle(r) {
  const n = Number(r.count ?? 1);
  if (n > 1) {
    const what = { stored: "backed up", received: "received", sent: "sent", collected: "collected", deleted: "deleted", restored: "restored", evicted: "freed" }[r.kind];
    if (what) return `${count(n, "file")} ${what}`;
  }
  return r.path ? base(r.path) : r.kind === "paired" ? `${r.device ?? "A device"} added` : (r.device ?? HAPPENED[r.kind]?.[1] ?? r.kind);
}

const BADGE = { received: ["arrow-down", "b-in"], stored: ["arrow-down", "b-in"], sent: ["arrow-up", "b-out"], collected: ["arrow-up", "b-out"],
  failed: ["circle-alert", "b-bad"], conflicted: ["git-compare", "b-bad"], paired: ["link", "b-ok"], deleted: ["trash-2", "b-quiet"], restored: ["rotate-ccw", "b-ok"] };

function activityRow(r) {
  const names = r.names?.length ? r.names : r.path ? [r.path] : [];
  const lead = names.length ? previewStack(names, null) : h("span", { class: "w-stack" }, h("span", { class: "w-pic" }, icon(HAPPENED[r.kind]?.[0] ?? "info")));
  const [bi, bc] = BADGE[r.kind] ?? ["info", "b-quiet"];
  lead.append(h("span", { class: `a-badge ${bc}` }, icon(bi)));
  const bad = r.kind === "failed";
  return h("button", { class: "w-row", onclick: () => activityEntry(r) },
    lead,
    h("span", { class: "w-main" },
      h("b", {}, activityTitle(r)),
      bad ? h("small", { class: "a-bad" }, r.detail ?? "Didn't finish")
        : r.device ? h("small", { class: "a-who" }, r.device) : null),
    h("small", { class: "a-time" }, new Date(r.at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })));
}

function dayOf(at) {
  const d = new Date(at * 1000), today = new Date();
  const same = (a, b) => a.toDateString() === b.toDateString();
  if (same(d, today)) return "Today";
  const y = new Date(today); y.setDate(today.getDate() - 1);
  if (same(d, y)) return "Yesterday";
  return d.toLocaleDateString([], { day: "numeric", month: "long" });
}

async function drawActivity(main) {
  const chips = h("div", { class: "a-chips" });
  const list = h("div", {});
  const older = btn("Older", () => more(), "btn-quiet");
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Activity", { words: "Settings", go: () => go("settings") }), chips, list, h("div", { class: "acts acts-c" }, older)));
  let rows = [], oldest = null, chosen = "all";
  const draw = () => {
    const keep = ACTIVITY_CHIPS.find(([k]) => k === chosen)[2];
    const problems = rows.filter((r) => r.kind === "failed").length;
    put(chips, ...ACTIVITY_CHIPS.map(([k, words]) => h("button", { class: `a-chip${k === chosen ? " on" : ""}`, onclick: () => { chosen = k; draw(); } },
      words, k === "problems" && problems ? h("span", { class: "a-n" }, problems) : null)));
    const shown = rows.filter(keep);
    if (!shown.length) { put(list, empty("history", "Nothing yet")); return; }
    const days = [];
    for (const r of shown) {
      const day = dayOf(r.at);
      if (days.at(-1)?.day !== day) days.push({ day, rows: [] });
      days.at(-1).rows.push(r);
    }
    put(list, ...days.flatMap((d) => [label(d.day), h("div", { class: "w-card" }, ...d.rows.map(activityRow))]));
  };
  const more = async () => {
    older.disabled = true;
    try {
      const got = await invoke("activity", { path: null, limit: 60, before: oldest });
      rows = rows.concat(got);
      oldest = got.length ? got[got.length - 1].id : oldest;
      older.hidden = got.length < 60;
      draw();
    } catch (e) { list.append(oops(e)); }
    older.disabled = false;
  };
  more();
}

/** An entry, as the phone's: when, with which device, how big, its files;
 * for a problem, why first. */
function activityEntry(r) {
  const names = r.names?.length ? r.names : r.path ? [r.path] : [];
  const when = r.started && r.started !== r.at
    ? `${new Date(r.started * 1000).toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })} to ${new Date(r.at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
    : new Date(r.at * 1000).toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const box = sheet({ ic: names.length ? previewStack(names, null) : tile(HAPPENED[r.kind]?.[0] ?? "info"), title: activityTitle(r), sub: when });
  box.append(h("div", { class: "grp-box" },
    r.kind === "failed" ? setting({ ic: "circle-alert", name: "Why", value: r.detail ?? "Not said" }) : null,
    r.device ? setting({ ic: "monitor-smartphone", name: "With", value: r.device }) : null,
    r.size ? setting({ ic: "hard-drive", name: "Size", value: size(r.size) }) : null,
    Number(r.count) > 1 ? setting({ ic: "file", name: "Files", value: Number(r.count).toLocaleString() }) : null));
  if (names.length > 1) {
    const grid = h("div", { class: "a-grid" });
    names.slice(0, 11).forEach((n) => { const c = h("span", { class: "w-pic w-pic-row" }); previewInto(c, n); grid.append(c); });
    if (Number(r.count) > names.length) grid.append(h("span", { class: "w-pic w-pic-row a-more" }, `+${(Number(r.count) - Math.min(names.length, 11)).toLocaleString()}`));
    box.append(label("Files"), grid);
  }
  icons(box);
}
