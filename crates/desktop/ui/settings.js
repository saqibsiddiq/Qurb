// Storage, and Settings.
//
// Storage is where Qurb explains its one unusual idea: freeing space without
// losing files (direction §20). Settings is grouped lists, quieter than
// everything else (§23).

// --------------------------------------------------------------- storage

const GIB = 1024 ** 3;
let disk = 0;

async function drawStorage() {
  let st, fr, parts;
  try {
    [st, fr, parts] = await Promise.all([invoke("storage"), invoke("freeable"), invoke("storage_parts")]);
  } catch (e) {
    $("used-says").textContent = String(e);
    return;
  }
  disk = Number(st.disk);
  const used = Number(st.used);
  const freeable = Number(fr.bytes);
  const free = Number(st.free_disk);
  const limit = Number(st.limit);

  // How much: what Qurb takes on this disk, against what it may take.
  $("used-big").textContent = size(st.used);
  $("used-says").textContent = [
    limit > 0 ? `of ${size(st.limit)} allowed${st.over ? ", over the limit" : ""}` : "with no limit set",
    `${size(st.free_disk)} free on this disk`,
  ].join(" · ");
  $("free-all").classList.toggle("hidden", freeable === 0);

  // One bar, against the allowance when there is one, else the disk: what
  // Qurb uses, the part of it that could go, and the room left.
  const whole = Math.max(limit > 0 ? limit : Math.max(disk, used + free), used, 1);
  $("meter-used").style.width = `${(100 * (used - freeable)) / whole}%`;
  $("meter-freeable").style.left = `${(100 * (used - freeable)) / whole}%`;
  $("meter-freeable").style.width = `${(100 * freeable) / whole}%`;
  const legend = $("legend");
  legend.replaceChildren();
  const key = (cls, words) => {
    const k = el("span", cls);
    k.append(el("i"), words);
    legend.append(k);
  };
  key("used", `Qurb uses ${size(st.used)}`);
  if (freeable > 0) key("freeable", `${size(fr.bytes)} can be freed`);
  key("free", limit > 0 ? `${size(String(Math.max(0, limit - used)))} left of the limit` : `${size(st.free_disk)} free on this disk`);

  drawParts(parts, st, fr);
  // The cards count files at their size; the disk holds them compressed,
  // and what files share once, so the two need not add up.
  $("parts-note").textContent = `Sizes above are the files' own. On this disk Qurb takes ${size(st.used)} for all of it: compressed, and what files share kept once.`;

  $("freeable-says").textContent = freeable > 0
    ? `${size(fr.bytes)} from ${count(fr.count, "file")}. They stay in Qurb, available from your other devices.`
    : st.only_here > 0
      ? `${count(st.only_here, "file")} ${st.only_here === 1 ? "is" : "are"} only on this computer, so ${st.only_here === 1 ? "it stays" : "they stay"} until another device has ${st.only_here === 1 ? "it" : "them"}.`
      : "Nothing right now: everything here is either needed here or already freed.";
  const list = $("freeable-list");
  list.replaceChildren();
  for (const f of fr.files.slice(0, FREEABLE_SHOWN)) {
    const go = button("Free local space", "btn small", "cloud-off");
    const li = row({
      iconName: KIND_ICON[kindOf(f.path)],
      name: base(f.path),
      sub: [stateTag(f.availability), size(f.size), folderOf(f.path) || "Shared"],
      trail: [go],
      onClick: () => openDetails(f.path),
    });
    li.dataset.path = f.path;
    go.addEventListener("click", () => freeLocal(f.path, li, drawStorage));
    list.append(li);
  }
  if (fr.count > FREEABLE_SHOWN) {
    list.append(el("p", "note", `The largest ${FREEABLE_SHOWN} of ${count(fr.count, "file")}. Free local space, above, frees them all.`));
  }

  $("usage").textContent = limit > 0
    ? `${size(st.used)} of ${size(st.limit)}${st.over ? " — over the limit" : ""}`
    : "No limit set";

  // Not touched while somebody is dragging: rewriting the control they are
  // holding is the single most irritating thing a live-updating screen does.
  if (document.activeElement !== $("limit")) {
    $("capped").checked = limit > 0;
    $("limit-controls").classList.toggle("hidden", limit === 0);
    $("limit").value = String(toTrack(limit > 0 ? limit / GIB : disk / GIB / 10));
    showLimitLabel();
  }
}

/** Files listed under *Files you can free*; the button frees them all. */
const FREEABLE_SHOWN = 5;

/** What is kept here, four of one size: the phones' backups, the shared
 *  files, guests' folders and Recently deleted, each with what to do. */
function drawParts(parts, st, fr) {
  const box = $("parts");
  box.replaceChildren();
  const card = ({ iconName, title, big, lines, action }) => {
    const c = el("div", "part card glass-frosted");
    const head = el("div", "part-head");
    const tile = el("span", "tile");
    tile.append(icon(iconName));
    head.append(tile, el("strong", null, title));
    c.append(head, el("div", "part-size num", big));
    const said = el("div", "part-lines");
    for (const line of lines) said.append(el("p", "meta", line));
    c.append(said);
    if (action) {
      const [label, act] = action;
      const b = button(label, "btn small");
      b.addEventListener("click", act);
      const foot = el("div", "part-foot");
      foot.append(b);
      c.append(foot);
    }
    box.append(c);
  };
  const sum = (key) => parts.phones.reduce((n, p) => n + Number(p[key]), 0);

  const phoneBytes = sum("whatsapp") + sum("folders") + sum("vault");
  card({
    iconName: "smartphone",
    title: "Your phones' backups",
    big: parts.phones.length ? size(String(phoneBytes)) : "None yet",
    lines: parts.phones.length
      ? parts.phones.map((p) => `${p.name}: ` + [
        Number(p.whatsapp) > 0 ? `WhatsApp ${size(p.whatsapp)}` : null,
        Number(p.folders) > 0 ? `folders ${size(p.folders)}` : null,
        Number(p.vault) > 0 ? `Private Vault ${size(p.vault)}` : null,
      ].filter(Boolean).join(" · "))
      : ["A phone's backups come here once it keeps its Private Vault on this computer."],
    action: parts.phones.length ? ["Open in Files", () => { showScreen("files"); filesBrowser.openPerson("owner"); }] : null,
  });
  card({
    iconName: "folder",
    title: "Shared files",
    big: `${size(parts.shared_here)} here`,
    lines: [
      `${count(parts.shared_files, "file")}, on all your devices`,
      st.evicted > 0 ? `${count(st.evicted, "file")} freed here, fetched when opened` : "Every one of them on this computer",
    ],
    action: ["Open in Files", () => { showScreen("files"); filesBrowser.openShared(); }],
  });
  const guestBytes = parts.guests.reduce((n, g) => n + Number(g.size), 0);
  card({
    iconName: "user-plus",
    title: "Guests' folders",
    big: parts.guests.length ? size(String(guestBytes)) : "None",
    lines: parts.guests.length
      ? parts.guests.map((g) => `${g.name}: ${size(g.size)}, sealed`)
      : ["Nobody else keeps a folder here. A guest's is sealed: you can't open it."],
    action: ["Devices", () => showScreen("devices")],
  });
  card({
    iconName: "trash-2",
    title: "Recently deleted",
    big: size(parts.deleted),
    lines: [parts.deleted_files ? `${count(parts.deleted_files, "file")}, each kept 30 days` : "Nothing deleted lately"],
    action: ["Open", () => showScreen("deleted")],
  });
}

$("free-all").addEventListener("click", async () => {
  const b = $("free-all");
  b.disabled = true;
  let freed = 0, n = 0;
  try {
    // A page at a time, largest first, until nothing more can go.
    for (let round = 0; round < 20; round++) {
      const fr = await invoke("freeable");
      if (fr.files.length === 0) break;
      for (const f of fr.files) {
        try { freed += Number(await invoke("free_file", { path: f.path })); n++; } catch (e) { /* kept, and said below */ }
      }
    }
    toast(`Freed ${size(String(freed))} from ${count(n, "file")}. They stay in Qurb.`);
  } finally {
    b.disabled = false;
    drawStorage();
  }
});

// The track is square-law rather than linear.
//
// A 500 GB disk against an allowance somebody actually wants — ten or twenty
// gigabytes — puts the useful part of a linear slider in its first four
// percent, where it cannot be aimed at. Squaring gives the small end most of
// the track and leaves the large end coarse, which is the right way round:
// nobody needs 380 GB rather than 390.
const TRACK = 1000;

function toTrack(gib) {
  if (maxGib() <= 0) return 0;
  return Math.round(Math.sqrt(Math.min(gib, maxGib()) / maxGib()) * TRACK);
}

function fromTrack(position) {
  const share = position / TRACK;
  return Math.max(1, Math.round(share * share * maxGib()));
}

function maxGib() {
  return Math.max(1, Math.floor(disk / GIB));
}

function chosenGib() {
  return fromTrack(Number($("limit").value));
}

function showLimitLabel() {
  const gib = chosenGib();
  const share = disk > 0 ? ` — ${Math.round((gib * GIB / disk) * 100)}% of this disk` : "";
  $("limit-label").textContent = `${gib} GB${share}`;
}

$("limit").addEventListener("input", showLimitLabel);

$("capped").addEventListener("change", async (event) => {
  $("limit-controls").classList.toggle("hidden", !event.target.checked);
  // Unchecking means no limit, and is worth applying at once: somebody who has
  // just turned a limit off is asking for the cap to stop, not asking to press
  // a second button.
  if (!event.target.checked) await saveLimit("0");
});

$("apply").addEventListener("click", () => saveLimit(String(chosenGib() * GIB)));

async function saveLimit(bytes) {
  try {
    await invoke("set_limit", { bytes });
    const saved = $("saved");
    saved.classList.remove("hidden");
    setTimeout(() => saved.classList.add("hidden"), 1600);
    await drawStorage();
  } catch (e) {
    $("usage").textContent = String(e);
  }
}

// ------------------------------------------------------------- security

const PROTECTION = {
  file: "In a file only you can read. Enough against other people using this computer; " +
    "not against anyone who can read its disk.",
  keystore: "In the system keystore, locked while you're logged out. Qurb opens it without " +
    "asking while you're logged in.",
  passphrase: "Behind a passphrase only you know. Qurb asks for it when it starts, and nothing " +
    "syncs until you type it — the only option that protects the key from someone with the disk and your login.",
};

/** Which change is being made in the passphrase form, if one is. */
let protecting = null;

async function drawSecurity() {
  let s;
  try {
    s = await invoke("security");
  } catch (e) {
    $("protection-says").textContent = String(e);
    return;
  }
  $("set-identity").textContent = s.identity;
  $("protection-says").textContent = PROTECTION[s.protection] ?? s.protection;

  const buttons = $("protection-buttons");
  buttons.replaceChildren();
  const offer = (label, action) => {
    const b = el("button", "btn small", label);
    b.addEventListener("click", action);
    buttons.append(b);
  };
  const was = s.protection;
  if (was !== "passphrase") offer("Protect with a passphrase…", () => protectForm("passphrase", was));
  if (was === "passphrase") offer("Change passphrase…", () => protectForm("passphrase", was));
  if (was !== "keystore") offer("Use the keystore", () => protectForm("keystore", was));

  const events = $("security-events");
  events.replaceChildren();
  if (s.events.length === 0) events.append(el("li", "meta", "Nothing yet"));
  for (const r of s.events.slice(0, 6)) {
    events.append(row({
      iconName: r.kind === "removed" ? "x" : "monitor-smartphone",
      name: r.device ?? r.detail ?? "",
      sub: [r.kind === "removed" ? "Removed" : "Paired", when(r.at)],
    }));
  }
}

/** The fields a change of protection needs, and nothing else. */
function protectForm(to, was) {
  protecting = { to, was };
  const form = $("protect-form");
  form.classList.remove("hidden");
  $("protect-current-wrap").classList.toggle("hidden", was !== "passphrase");
  const wantsNew = to === "passphrase";
  $("protect-new-wrap").classList.toggle("hidden", !wantsNew);
  $("protect-again-wrap").classList.toggle("hidden", !wantsNew);
  $("protect-says").classList.add("hidden");
  for (const id of ["protect-current", "protect-new", "protect-again"]) $(id).value = "";
  $("protect-go").textContent = wantsNew ? "Set the passphrase" : "Use the keystore";
  (was === "passphrase" ? $("protect-current") : wantsNew ? $("protect-new") : $("protect-go")).focus();
}

$("protect-cancel").addEventListener("click", () => {
  protecting = null;
  $("protect-form").classList.add("hidden");
});

$("protect-go").addEventListener("click", async () => {
  if (!protecting) return;
  const says = $("protect-says");
  const current = $("protect-current").value || null;
  const fresh = $("protect-new").value;
  const again = $("protect-again").value;
  if (protecting.to === "passphrase" && fresh !== again) {
    says.textContent = "The two new passphrases aren't the same.";
    says.classList.remove("hidden");
    return;
  }
  try {
    await invoke("protect_key", {
      to: protecting.to,
      current,
      new: protecting.to === "passphrase" ? fresh : null,
    });
    protecting = null;
    $("protect-form").classList.add("hidden");
    toast("Key protection changed");
    drawSecurity();
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  } finally {
    for (const id of ["protect-current", "protect-new", "protect-again"]) $(id).value = "";
  }
});

// -------------------------------------------------------------- settings

async function drawSettings() {
  drawSecurity();
  let s;
  try {
    s = await invoke("settings");
  } catch (e) {
    $("settings-says").textContent = String(e);
    return;
  }

  // A computer has nothing like a phone's Google backup: what keeps its key
  // safe is another device that holds it, best a phone (decision 0053).
  const others = s.key_also_on ?? [];
  $("key-safe").textContent = others.length === 0
    ? "Only on this computer. Pair your phone: it can give the key to a new computer with a code, and keeps it in its Google backup."
    : `On this computer and on ${others.join(", ")}. Any of them can give it to a new device with a code${s.phone_holds_key ? "; your phone also keeps it in its Google backup" : ". Pair a phone too: it keeps the key in its Google backup"}.`;
  $("key-safe").classList.toggle("warn", others.length === 0);

  // Not rewritten under somebody who is in the middle of typing.
  const editing = document.activeElement?.closest?.("#settings .item");
  if (!editing) {
    $("set-name").value = s.name;
    $("set-signal").value = s.signal;
    $("set-relay").value = s.relay ?? "";
    $("set-port").value = String(s.port);
    $("set-downloads").value = s.downloads;
  }
  showPicture(s.picture, s.name);
  $("set-root").textContent = s.root;
  $("set-version").textContent = s.version;
  $("set-private").checked = s.own_files_private;
  $("set-notify").checked = s.notifications;
  $("motion-says").textContent = calm() ? "Reduced" : "Full";
  showTheme();

  // Where that actually is, since the setting can be empty or "off". Serde
  // sends a Result as {Ok} or {Err}.
  const at = s.downloads_at;
  $("downloads-at").textContent =
    "Err" in at ? `Refused: ${at.Err}` :
    at.Ok === null ? "Kept inside your Qurb folder." :
    `Now: ${at.Ok}`;
  $("open-downloads").classList.toggle("hidden", !("Ok" in at) || at.Ok === null);

  try {
    // A device set up again is one device (item 6).
    const devices = (await invoke("devices")).filter((d) => !d.replaced_by);
    $("paired-says").textContent = devices.length === 0 ? "None yet" : devices.map((d) => d.name).join(", ");
  } catch (e) { /* the row says nothing */ }

  $("at-login").checked = await invoke("starts_at_login");
}

/**
 * This computer's picture (the owner's list of 2026-10-26, item 7): its
 * circle, as its other devices show it, or the first letter of its name.
 */
function showPicture(address, name) {
  const face = $("set-picture");
  face.replaceChildren();
  if (address) {
    const img = el("img");
    img.src = address;
    img.alt = "";
    face.append(img);
  } else {
    face.textContent = (name || "?").trim().slice(0, 1).toUpperCase();
  }
  $("remove-picture").classList.toggle("hidden", !address);
}

/**
 * The middle square of a picture, 256 pixels across, as JPEG bytes: what the
 * phone makes of one too. Read as a data: address, which the window's
 * content policy lets an image load from; a blob: one it does not.
 */
async function squareJpeg(file) {
  const address = await new Promise((done, fail) => {
    const reader = new FileReader();
    reader.onload = () => done(reader.result);
    reader.onerror = () => fail(new Error("could not read that picture"));
    reader.readAsDataURL(file);
  });
  const image = new Image();
  await new Promise((done, fail) => {
    image.onload = done;
    image.onerror = () => fail(new Error("that isn't a picture"));
    image.src = address;
  });
  const side = Math.min(image.naturalWidth, image.naturalHeight);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  canvas.getContext("2d").drawImage(image,
    (image.naturalWidth - side) / 2, (image.naturalHeight - side) / 2, side, side, 0, 0, 256, 256);
  const blob = await new Promise((done, fail) => canvas.toBlob(
    (b) => (b ? done(b) : fail(new Error("could not make the picture"))), "image/jpeg", 0.85));
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}

$("choose-picture").addEventListener("click", () => $("picture-file").click());
$("picture-file").addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  event.target.value = "";
  if (!file) return;
  try {
    await invoke("set_picture", { picture: await squareJpeg(file) });
    toast("Picture set");
    drawSettings();
  } catch (e) {
    toast(String(e), true);
  }
});
$("remove-picture").addEventListener("click", async () => {
  try {
    await invoke("set_picture", { picture: null });
    drawSettings();
  } catch (e) {
    toast(String(e), true);
  }
});

/** A switch that applies at once, and puts itself back if that fails. */
/** The theme choice (decision 0056): System, Light or Dark, applied at once. */
function showTheme() {
  const chosen = window.qurbTheme.chosen();
  for (const b of $("theme-choice").querySelectorAll("button")) {
    b.setAttribute("aria-checked", String(b.dataset.theme === chosen));
  }
}
for (const b of $("theme-choice").querySelectorAll("button")) {
  b.addEventListener("click", () => { window.qurbTheme.choose(b.dataset.theme); showTheme(); });
}

function applyAtOnce(id, apply, onWords, offWords) {
  $(id).addEventListener("change", async (event) => {
    const on = event.target.checked;
    try {
      await apply(on);
      toast(on ? onWords : offWords);
    } catch (e) {
      event.target.checked = !on;
      toast(String(e), true);
    }
  });
}

applyAtOnce("at-login", (on) => invoke("set_starts_at_login", { on }),
  "Qurb will start when you log in", "Qurb won't start by itself");
applyAtOnce("set-private", (on) => invoke("set_privacy", { private: on }),
  "Files you add here now go to your Private Vault", "Files you add here now go to all your devices");
applyAtOnce("set-notify", (on) => invoke("set_notifications", { on }),
  "Notifications on", "Notifications off");

$("quit").addEventListener("click", () => invoke("quit"));

$("settings-save").addEventListener("click", async () => {
  const says = $("settings-says");
  try {
    await invoke("save_settings", {
      name: $("set-name").value,
      signal: $("set-signal").value,
      relay: $("set-relay").value,
      port: Number($("set-port").value) || 0,
      downloads: $("set-downloads").value,
    });
    toast("Settings saved");
    // So "Now: …" shows what was just saved rather than what was there.
    document.activeElement?.blur?.();
    drawSettings();
  } catch (e) {
    says.textContent = String(e);
    says.classList.add("warn");
  }
});

// The name and where received files go are the ones people change: saved as
// soon as the field is left, rather than waiting for a button at the foot.
for (const id of ["set-name", "set-downloads"]) {
  $(id).addEventListener("change", () => $("settings-save").click());
}

$("open-downloads").addEventListener("click", async () => {
  try { await invoke("open_downloads"); } catch (e) { toast(String(e), true); }
});

$("show-phrase").addEventListener("click", async () => {
  const wrap = $("revealed-wrap");
  const list = $("revealed");
  const says = $("phrase-says");

  // A second press hides them again, so they are not left on a screen somebody
  // walks away from.
  if (!wrap.classList.contains("hidden")) {
    list.replaceChildren();
    wrap.classList.add("hidden");
    $("show-phrase").textContent = "Show";
    return;
  }

  try {
    const words = await invoke("reveal_phrase");
    list.replaceChildren();
    for (const word of words) list.append(el("li", null, word));
    wrap.classList.remove("hidden");
    says.classList.add("hidden");
    $("show-phrase").textContent = "Hide";
  } catch (e) {
    wrap.classList.remove("hidden");
    says.textContent = String(e);
    says.classList.remove("hidden");
  }
});
