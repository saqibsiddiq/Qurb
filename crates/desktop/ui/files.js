// Files, Private Vault, a file's details, Recently deleted, and sending.
//
// Files should feel like a familiar file browser, not a dashboard (direction
// §18): search, where you are, folders standing apart, then files -- each with
// its name, a little metadata and where its bytes are. Freeing local space
// never looks like deleting (§12), and the only copy is protected (§13).

/** The synced folder on this computer, from `situation`, for sending. */
let rootPath = "";

/** Paths asked for with Keep on this device and not here yet: drawn as
 *  Downloading until a redraw finds them here (§30). */
const wanted = new Set();

/**
 * One file browser: the shared area (Files) or this computer's own vault
 * (Private Vault). Same query, same rows; only the area differs.
 */
function makeBrowser({ isPrivate, rootName, find, crumbs, folders, list }) {
  const b = { dir: "", searching: "" };

  b.draw = async () => {
    const out = $(list);
    try {
      if (b.searching) {
        const found = await invoke("find", { text: b.searching, private: isPrivate });
        $(crumbs).replaceChildren(el("span", "quiet", `${count(found.length, "result")} for “${b.searching}”`));
        $(folders).replaceChildren();
        out.replaceChildren();
        if (!isPrivate) drawDeletedLink();
        if (found.length === 0) out.append(empty("search", "Nothing matches that name."));
        for (const f of found) out.append(fileRow(f, b, true));
        return;
      }
      const d = await invoke("browse", { dir: b.dir, private: isPrivate });
      drawCrumbs();
      $(folders).replaceChildren();
      // Each kind labelled whenever it is there, as on the phone.
      if (d.folders.length > 0) $(folders).append(el("div", "list-label", "Folders"));
      for (const name of d.folders) $(folders).append(folderTile(name));
      out.replaceChildren();
      if (d.files.length > 0) out.append(el("div", "list-label", "Files"));
      for (const f of d.files) out.append(fileRow(f, b, false));
      if (!isPrivate) drawDeletedLink();
      if (d.files.length === 0 && d.folders.length === 0) {
        out.append(isPrivate
          ? empty("lock-keyhole", b.dir ? "This folder is empty."
            : "Nothing here yet. Turn on Keep new files private in Settings, and files you add on this computer come here.")
          : empty("folder-open", b.dir ? "This folder is empty."
            : "Your Qurb folder is empty. Put files in it, and they appear on your other devices."));
      }
    } catch (e) {
      oops(out, e);
    }
  };

  function drawCrumbs() {
    const nav = $(crumbs);
    nav.replaceChildren();
    const parts = b.dir ? b.dir.split("/") : [];
    const go = (dir) => { b.dir = dir; closePanel(); b.draw(); };
    const root = el("button", null, rootName);
    root.addEventListener("click", () => go(""));
    nav.append(root);
    parts.forEach((part, i) => {
      nav.append(icon("chevron-right"));
      const crumb = el("button", null, part);
      crumb.addEventListener("click", () => go(parts.slice(0, i + 1).join("/")));
      nav.append(crumb);
    });
  }

  function folderTile(name) {
    const tile = el("button", "folder");
    tile.append(icon("folder"), el("span", null, name));
    const path = b.dir ? `${b.dir}/${name}` : name;
    tile.addEventListener("click", () => { b.dir = path; closePanel(); b.draw(); });
    // Who has a folder, and whether this computer keeps it, from its own
    // menu: set once and rarely looked at again.
    if (!isPrivate) {
      tile.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        menu(tile, [["Folder options", "settings", () => openFolderOptions(path.split("/")[0])]]);
      });
    }
    return tile;
  }

  // Debounced, because every keystroke is a query and a list that redraws
  // under the cursor while somebody is still typing is worse than one that
  // waits.
  let typing = null;
  $(find).addEventListener("input", (event) => {
    clearTimeout(typing);
    typing = setTimeout(() => { b.searching = event.target.value.trim(); b.draw(); }, 180);
  });

  return b;
}

const filesBrowser = makeBrowser({
  isPrivate: false, rootName: "Qurb",
  find: "find", crumbs: "crumbs", folders: "folders", list: "file-list",
});
const vaultBrowser = makeBrowser({
  isPrivate: true, rootName: "Private Vault",
  find: "vault-find", crumbs: "vault-crumbs", folders: "vault-folders", list: "vault-list",
});

/** The state a file is in right now, including one being downloaded. */
function liveState(f) {
  if (f.availability !== "elsewhere") wanted.delete(f.path);
  if (wanted.has(f.path)) {
    const tag = el("span", "state moving");
    tag.append(icon("download"), el("span", null, "Downloading"));
    return tag;
  }
  return stateTag(f.availability);
}

function fileRow(f, browser, showFolder) {
  const more = el("button", "icon-btn reveal");
  more.setAttribute("aria-label", `More for ${base(f.path)}`);
  more.append(icon("ellipsis"));
  more.addEventListener("click", (event) => {
    event.stopPropagation();
    menu(more, fileActions(f, () => browser.draw()));
  });

  const trail = [];
  // The one contextual action worth a button of its own: getting back a file
  // that is not here.
  if (f.availability === "elsewhere" && !wanted.has(f.path)) {
    const keep = button("Keep here", "btn small reveal", "download");
    keep.addEventListener("click", (event) => { event.stopPropagation(); keepHere(f.path, li); });
    trail.push(keep);
  }
  trail.push(more);

  const li = row({
    iconName: KIND_ICON[kindOf(f.path)],
    name: base(f.path),
    sub: [
      liveState(f),
      size(f.size),
      showFolder ? (folderOf(f.path) || "Qurb") : when(f.modified),
    ],
    trail,
    cls: wanted.has(f.path) ? "arriving" : "",
    onClick: () => openDetails(f.path),
  });
  li.dataset.path = f.path;
  li.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    menu(more, fileActions(f, () => browser.draw()));
  });
  return li;
}

/** Whether a file's bytes are on this computer. */
const isHere = (f) => f.availability === "here" || f.availability === "only here";

/** What can be done to a file, in the order people reach for them. A file on
 *  no device can be looked at and deleted, and nothing else: there is nothing
 *  to open and nowhere to fetch it from (decision 0055). */
function fileActions(f, redraw) {
  const here = isHere(f);
  const items = [];
  if (here) {
    items.push(["Open", "external-link", () => openFile(f.path)]);
    items.push(["Show in folder", "folder-search", () => showFile(f.path)]);
  } else if (f.availability === "elsewhere") {
    items.push(["Keep on this device", "download", () => keepHere(f.path, rowFor(f.path))]);
  }
  if (f.availability === "here") {
    items.push(["Free local space", "cloud-off", () => freeLocal(f.path, rowFor(f.path), redraw)]);
  }
  if (here) items.push(["Send to device…", "send", () => openSend([`${rootPath}/${f.path}`])]);
  if (here) {
    items.push(f.private
      ? ["Move to shared", "folder", () => moveArea(f, false, redraw)]
      : ["Move to Private Vault…", "lock-keyhole", () => moveArea(f, true, redraw)]);
  }
  items.push(["Details", "info", () => openDetails(f.path)]);
  items.push("-");
  items.push(["Delete", "trash-2", () => deleteFile(f.path, redraw), true]);
  return items;
}

const rowFor = (path) => document.querySelector(`.screen.on li[data-path="${CSS.escape(path)}"]`);

async function openFile(path) {
  try { await invoke("open_file", { path }); } catch (e) { toast(String(e), true); }
}

async function showFile(path) {
  try { await invoke("show_file", { path }); } catch (e) { toast(String(e), true); }
}

/** Keep on this device (§30): asked for now, fetched at the next sync with a
 *  device that has it -- which is also what makes asking while offline work. */
async function keepHere(path, li) {
  try {
    await invoke("fetch", { path });
    wanted.add(path);
    if (li) {
      li.classList.add("arriving");
      li.querySelector(".sub > :first-child")?.replaceWith(liveState({ path, availability: "elsewhere" }));
      li.querySelector(".trail .btn")?.remove();
    }
    toast(`Downloading ${base(path)} to this computer`);
  } catch (e) {
    toast(String(e), true);
  }
}

/** Free local space (§12, §31): the file stays; only its local bytes go. */
async function freeLocal(path, li, redraw) {
  try {
    const freed = await invoke("free_file", { path });
    if (li && !calm()) {
      li.classList.add("freeing");
      setTimeout(() => {
        li.querySelector(".sub > :first-child")?.replaceWith(stateTag("elsewhere"));
      }, 240);
    }
    toast(`Freed ${size(freed)}. ${base(path)} stays in Qurb.`);
    setTimeout(() => redraw?.(), calm() ? 0 : 700);
  } catch (e) {
    toast(onlyCopy(e), true);
  }
}

/** §13, in the words the direction gives, whatever the engine called it. */
function onlyCopy(e) {
  return /no other device is known to hold|only holder/.test(String(e))
    ? "This is the only copy currently stored in Qurb, so it can't be freed."
    : String(e);
}

/** Into this computer's Private Vault, or out of it to every device
 *  (decision 0057). Into it, says first what happens to the other devices'
 *  copies, and whether any device still keeps one. */
async function moveArea(f, intoVault, redraw) {
  const name = base(f.path);
  const move = async () => {
    try {
      await invoke("move_file_area", { path: f.path, private: intoVault });
      toast(intoVault ? `${name} is in Private Vault.` : `${name} goes to all your devices.`);
      redraw?.();
    } catch (e) {
      toast(String(e), true);
    }
  };
  if (!intoVault) return move();

  let keepers = [];
  try { keepers = await invoke("vault_keepers"); } catch (e) { /* said below as none */ }
  const box = sheet();
  box.append(el("h2", null, `Move ${name} to Private Vault?`));
  box.append(el("p", "lead",
    "It stays on this computer, and your other devices remove their copies at their next sync — " +
    "each keeps it in Recently deleted for 30 days, as with any deletion."));
  box.append(el("p", keepers.length ? "lead" : "caution", keepers.length
    ? `${keepers.join(", ")} keeps a backup of this computer's Private Vault, so it keeps this file too.`
    : "No device keeps a backup of this computer's Private Vault, so this computer will have the only copy."));
  const actions = el("div", "actions");
  const cancel = button("Cancel", "btn");
  cancel.addEventListener("click", () => box.close());
  const go = button("Move", "btn primary", "lock-keyhole");
  go.id = "move-private-go";
  go.addEventListener("click", () => { box.close(); move(); });
  actions.append(cancel, go);
  box.append(actions);
}

async function deleteFile(path, redraw) {
  try {
    await invoke("delete_file", { path });
    closePanel();
    toast(`Deleted ${base(path)} from your devices. It's in Recently deleted for 30 days.`);
    redraw?.();
    drawDeletedLink();
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------- details §19

async function openDetails(path) {
  let d;
  try {
    d = await invoke("details", { path });
  } catch (e) {
    toast(String(e), true);
    return;
  }
  const f = d.file;
  const box = panel();
  const head = el("div", "panel-head");
  const tile = el("span", "tile");
  tile.append(icon(KIND_ICON[kindOf(f.path)], "large"));
  const names = el("div");
  names.style.paddingRight = "32px";
  names.append(el("h2", null, base(f.path)));
  names.append(el("p", "meta", `${f.private ? "Private Vault" : "Qurb"}${folderOf(f.path) ? ` / ${folderOf(f.path)}` : ""}`));
  head.append(tile, names);

  const body = el("div", "panel-body");
  const s = stateOf(f.availability);
  const tone = { "only here": "attention", nowhere: "attention", here: "healthy" }[f.availability] ?? "neutral";
  const badge = el("span", `badge ${tone}`);
  badge.append(icon(wanted.has(f.path) ? "download" : s.icon), el("span", null, wanted.has(f.path) ? "Downloading" : s.words));
  body.append(badge);
  if (f.private) {
    const p = el("span", "badge neutral");
    p.style.marginLeft = "6px";
    p.append(icon("lock-keyhole"), el("span", null, "Private"));
    body.append(p);
  }

  if (f.availability === "only here") {
    body.append(el("p", "note", "This is the only copy currently stored in Qurb. It reaches your other devices the next time one is online."));
  }
  if (f.availability === "nowhere") {
    body.append(el("p", "note", "No device this computer syncs with has this file. It was freed here while another device kept it, and that device no longer has it or is no longer paired. It stays listed so you know it existed; deleting it removes it from your devices' lists."));
  }

  const holders = [
    ...(isHere(f) ? ["This computer"] : []),
    ...d.holders,
  ];
  const facts = el("dl", "facts");
  for (const [term, value] of [
    ["Type", KIND_WORD[kindOf(f.path)]],
    ["Size", size(f.size)],
    ["Location", `${f.private ? "Private Vault" : "Qurb"}${folderOf(f.path) ? ` / ${folderOf(f.path)}` : ""}`],
    ["On", holders.length ? holders.join(", ") : f.availability === "nowhere" ? "No device" : "No device has told this one yet"],
    ["Modified", `${when(f.modified)} · ${new Date(f.modified * 1000).toLocaleString()}`],
  ]) {
    facts.append(el("dt", null, term), el("dd", null, value));
  }
  body.append(facts);

  // Versions only exist for a conflict (brief §2).
  const conflict = conflictsSeen.find((c) => c.path === f.path || c.other.path === f.path || c.this?.path === f.path);
  if (conflict) {
    const review = button("Review", "btn small");
    review.addEventListener("click", () => openConflicts());
    body.append(attention({ iconName: "git-compare", title: "Has another version", says: "Two devices changed it at once.", action: review }));
  }

  const actions = el("div", "actions");
  actions.style.flexDirection = "column";
  actions.style.alignItems = "stretch";
  const add = (label, cls, iconName, act) => {
    const b = button(label, cls, iconName);
    b.style.justifyContent = "flex-start";
    b.addEventListener("click", act);
    actions.append(b);
    return b;
  };
  const redraw = () => refreshScreen();
  if (f.availability === "elsewhere") {
    if (!wanted.has(f.path)) add("Keep on this device", "btn primary", "download", () => { keepHere(f.path, rowFor(f.path)); closePanel(); });
  } else if (isHere(f)) {
    add("Open", "btn primary", "external-link", () => openFile(f.path));
    add("Show in folder", "btn", "folder-search", () => showFile(f.path));
  }
  if (f.availability === "here") {
    add("Free local space", "btn", "cloud-off", () => { closePanel(); freeLocal(f.path, rowFor(f.path), redraw); });
  }
  if (isHere(f)) add("Send to device…", "btn", "send", () => openSend([`${rootPath}/${f.path}`]));
  if (isHere(f)) {
    if (f.private) add("Move to shared", "btn", "folder", () => { closePanel(); moveArea(f, false, redraw); });
    else add("Move to Private Vault…", "btn", "lock-keyhole", () => { closePanel(); moveArea(f, true, redraw); });
  }
  add("Delete", "btn ghost quiet-danger", "trash-2", () => deleteFile(f.path, redraw));
  body.append(actions);

  // What happened to it: each row names the event, since the file is known.
  if (d.history.length) {
    body.append(el("div", "list-label", "History"));
    const list = el("ul", "rows");
    for (const r of d.history.slice(0, 5)) {
      const [iconName, words] = HAPPENED[r.kind] ?? ["info", r.kind];
      list.append(row({ iconName, name: words, sub: [r.device, when(r.at)] }));
    }
    body.append(list);
  }

  box.append(head, body);
}

// ------------------------------------------------------ recently deleted §22

const RETENTION_DAYS = 30;

async function drawDeleted() {
  const list = $("deleted-list");
  let entries;
  try {
    entries = await invoke("recently_deleted");
  } catch (e) {
    oops(list, e);
    return;
  }
  list.replaceChildren();
  if (entries.length === 0) {
    list.append(empty("trash-2", "Nothing deleted recently."));
    return;
  }
  for (const d of entries) {
    const days = Math.max(0, RETENTION_DAYS - Math.floor((Date.now() / 1000 - d.at) / 86400));
    const restore = button("Restore", "btn small", "rotate-ccw");
    const forget = twoPress(button("Delete now", "btn small ghost quiet-danger reveal"), "Delete for good?", async () => {
      try {
        await invoke("forget_deleted", { id: d.id });
        drawDeleted();
      } catch (e) {
        toast(String(e), true);
      }
    });
    const li = row({
      iconName: KIND_ICON[kindOf(d.path)],
      name: base(d.path),
      sub: [
        `${d.private ? "Private Vault · " : ""}Deleted ${when(d.at)}${d.by ? ` on ${d.by}` : ""}`,
        days === 0 ? "Expires today" : `Expires in ${count(days, "day")}`,
      ],
      trail: [forget, restore],
    });
    if (d.why) li.title = d.why;
    restore.addEventListener("click", async () => {
      restore.disabled = true;
      try {
        const at = await invoke("restore_deleted", { id: d.id });
        toast(at !== d.path
          ? `Restored as ${base(at)}: something else is at ${base(d.path)} now.`
          : d.private
            ? `${base(at)} is back in Private Vault.`
            : `${base(at)} is back, on all your devices.`);
        drawDeleted();
        drawDeletedLink();
      } catch (e) {
        restore.disabled = false;
        toast(String(e), true);
      }
    });
    list.append(li);
  }
}

/** A quiet way to Recently deleted, under the file list: always there, as
 *  on the phone, so it is where you expect it before you need it. */
async function drawDeletedLink() {
  const box = $("deleted-link");
  try {
    const entries = await invoke("recently_deleted");
    box.replaceChildren();
    // At the top of Files only, as on the phone: not inside a folder, and
    // not among a search's results.
    if (filesBrowser.dir || filesBrowser.searching) return;
    const li = row({
      iconName: "trash-2",
      name: "Recently deleted",
      sub: [entries.length ? `${count(entries.length, "file")} · kept for 30 days` : "Kept for 30 days"],
      trail: [icon("chevron-right")],
      onClick: () => showScreen("deleted"),
    });
    li.style.marginTop = "16px";
    box.append(li);
  } catch (e) {
    box.replaceChildren();
  }
}

$("files-more").addEventListener("click", () => {
  const items = [["Recently deleted", "trash-2", () => showScreen("deleted")]];
  const top = filesBrowser.dir.split("/")[0];
  if (top) items.push(["Options for “" + top + "”", "settings", () => openFolderOptions(top)]);
  items.push(["Show the Qurb folder", "folder-search", () => invoke("show_root").catch((e) => toast(String(e), true))]);
  menu($("files-more"), items);
});

// ------------------------------------------------------------ folder options

/**
 * Who has a folder (decision 0044), and whether this computer keeps it or
 * only lists it (0045). A folder is on every device unless you choose; a
 * device left out keeps what it has and gets nothing new -- said, not hidden.
 */
async function openFolderOptions(folder) {
  let sharing;
  try {
    sharing = await invoke("sharing");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  const f = sharing.folders.find((x) => x.folder === folder);
  if (!f) { toast(`“${folder}” has no options yet: put a file in it first.`, true); return; }

  const box = sheet({ onClose: () => refreshScreen() });
  box.append(el("h2", null, folder));
  box.append(el("p", "lead", "Which devices have this folder, and whether this computer keeps its files."));

  box.append(el("div", "list-label", "On these devices"));
  const picks = el("div", "group glass-frosted");
  for (const d of sharing.devices) {
    const item = el("label", "item");
    const label = el("div", "label");
    label.append(el("strong", null, d.name));
    const sw = el("span", "switch");
    const input = el("input");
    input.type = "checkbox";
    input.value = d.id;
    input.checked = f.everyone || f.members.includes(d.id);
    sw.append(input, el("span"));
    item.append(icon(deviceIcon(d.name)), label, sw);
    picks.append(item);
  }
  box.append(picks);
  box.append(el("p", "note", "A device you leave out keeps what it already has, and gets nothing new."));
  const says = el("p", "says warn hidden");
  const save = button("Save", "btn primary");
  save.addEventListener("click", async () => {
    const ticked = [...picks.querySelectorAll("input:checked")].map((i) => i.value);
    save.disabled = true;
    try {
      // Every device ticked is the same as no rule: said that way, so a device
      // paired later is included too.
      await invoke("set_sharing", { folder, members: ticked.length === sharing.devices.length ? [] : ticked });
      toast(ticked.length === sharing.devices.length ? `${folder} is on all your devices` : `Saved who has ${folder}`);
      box.close();
    } catch (e) {
      save.disabled = false;
      says.textContent = String(e);
      says.classList.remove("hidden");
    }
  });

  box.append(el("div", "list-label", "On this computer"));
  const where = el("div", "group glass-frosted");
  const item = el("div", "item");
  const label = el("div", "label");
  label.append(el("strong", null, f.remote ? "Downloaded when opened" : "Kept on this computer"));
  label.append(el("span", null, f.remote
    ? "Its files are listed here and download when you keep one."
    : "Free local space to list its files here without keeping them. Nothing is deleted."));
  const change = button(f.remote ? "Keep here" : "Free local space", "btn small", f.remote ? "download" : "cloud-off");
  change.addEventListener("click", async () => {
    change.disabled = true;
    try {
      if (f.remote) {
        const asked = await invoke("keep_locally", { folder });
        toast(asked ? `${count(asked, "file")} on the way back` : `${folder} is kept here`);
      } else {
        const r = await invoke("keep_remotely", { folder });
        const kept = r.kept.length ? ` ${count(r.kept.length, "file")} stayed: this computer has the only copy.` : "";
        toast(`Freed ${size(r.bytes)} from ${folder}.${kept}`);
      }
      box.close();
    } catch (e) {
      change.disabled = false;
      toast(onlyCopy(e), true);
    }
  });
  item.append(label, change);
  where.append(item);
  box.append(where, says);
  const actions = el("div", "actions end");
  actions.append(save);
  box.append(actions);
}

// --------------------------------------------------------------- send §17

/**
 * Send: what, to which device, what is happening, when it is complete.
 *
 * Paths of files and folders, never contents: reading them is the Rust
 * side's job, and a window that loaded a 4 GB file into a JavaScript variable
 * to hand it back would be a poor way to move it four inches.
 */
let sending = null;

function openSend(paths = [], device = null) {
  if (sending) { sending.pick(paths); return; }
  const box = sheet({ onClose: () => { sending = null; } });
  const state = { paths: [...paths], device };
  sending = { pick: (more) => { if (more.length) { state.paths = more; drawWhat(); } } };

  box.append(el("h2", null, "Send to device"));
  box.append(el("p", "lead", "To one device, and nowhere else. It isn't added to your other devices."));

  const what = el("div");
  const to = el("div");
  const says = el("p", "says");
  const actions = el("div", "actions end");
  const go = button("Send", "btn primary", "send");
  actions.append(go);
  box.append(what, to, says, actions);

  function drawWhat() {
    what.replaceChildren();
    const drop = el("div", "drop");
    const t = el("span", "tile");
    t.append(icon("upload"));
    drop.append(t);
    if (state.paths.length === 0) {
      drop.append(el("strong", null, "Drop files or folders here"));
    } else {
      const names = state.paths.map(base);
      drop.append(el("strong", null, names.length <= 3 ? names.join(", ") : `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`));
    }
    const choose = el("div", "actions");
    choose.style.marginTop = "4px";
    const files = button("Choose files…", "btn small");
    const folder = button("Choose a folder…", "btn small");
    files.addEventListener("click", () => pickFromDisk(false));
    folder.addEventListener("click", () => pickFromDisk(true));
    choose.append(files, folder);
    drop.append(choose);
    what.append(drop);
    go.disabled = state.paths.length === 0 || !state.device;
  }

  // The platform's own dialog: two buttons because no platform's dialog picks
  // files and folders at once.
  async function pickFromDisk(folder) {
    const dialog = window.__TAURI__?.dialog;
    if (!dialog) { says.textContent = "No file chooser here — drag them in instead."; return; }
    const chosen = await dialog.open({ multiple: !folder, directory: folder });
    if (!chosen) return;
    const list = Array.isArray(chosen) ? chosen : [chosen];
    state.paths = list.map((c) => (typeof c === "string" ? c : c.path));
    drawWhat();
  }

  async function drawTo() {
    to.replaceChildren(el("div", "list-label", "To"));
    let devices;
    try {
      devices = await invoke("devices");
    } catch (e) {
      to.append(el("p", "says warn", String(e)));
      return;
    }
    if (devices.length === 0) {
      const add = button("Add a device", "btn small", "plus");
      add.addEventListener("click", () => { box.close(); openAddDevice(); });
      to.append(empty("monitor-smartphone", "No devices yet.", add, true));
      return;
    }
    const choices = el("div", "choices");
    choices.style.marginTop = "0";
    for (const d of devices) {
      const c = el("button", "choice");
      const t = el("span", "tile");
      t.append(icon(deviceIcon(d.name)));
      const words = el("span");
      words.append(el("strong", null, d.name));
      words.append(el("span", null, d.route ? "Connected now" : d.last_seen ? `Last seen ${when(d.last_seen)}` : "Not connected yet"));
      c.append(t, words, icon(state.device?.fingerprint === d.fingerprint ? "circle-check" : "chevron-right"));
      if (state.device?.fingerprint === d.fingerprint) c.style.borderColor = "var(--green)";
      c.addEventListener("click", () => { state.device = d; drawTo(); drawWhat(); });
      choices.append(c);
    }
    to.append(choices);
    if (!state.device && devices.length === 1) { state.device = devices[0]; drawTo(); drawWhat(); }
  }

  go.addEventListener("click", async () => {
    const d = state.device;
    if (!d || state.paths.length === 0) return;
    go.disabled = true;
    // Sent there before: say so, and let the person choose, rather than send
    // a second copy unasked or drop it unsaid (decision 0059).
    let earlier = [];
    try { earlier = await invoke("sent_before", { paths: state.paths, to: d.fingerprint }); } catch (e) { /* send as asked */ }
    if (earlier.length) { askAgain(earlier, d); return; }
    send(d, []);
  });

  async function send(d, leaveOut) {
    go.disabled = true;
    // Storing a large folder takes a while, and the button would otherwise
    // look as though it did nothing.
    says.textContent = `Getting ready to send to ${d.name}…`;
    try {
      const r = await invoke("send_files", { paths: state.paths, to: d.fingerprint, leaveOut });
      showJourney(r, d);
    } catch (e) {
      says.textContent = String(e);
      says.classList.add("warn");
      go.disabled = false;
    }
  }

  /** Files that went to this device before, and whether to send them again. */
  function askAgain(earlier, d) {
    what.replaceChildren();
    to.replaceChildren();
    actions.replaceChildren();
    says.textContent = "";
    const one = earlier.length === 1;
    what.append(el("div", "list-label", `Sent to ${d.name} before`));
    const list = el("ul", "rows");
    for (const e of earlier) {
      list.append(row({ iconName: "send", name: base(e.path), sub: [`as ${e.sent_as}`, when(e.at)] }));
    }
    what.append(list);
    says.textContent = one
      ? `${d.name} may still have it. Sending it again puts another copy there.`
      : `${d.name} may still have them. Sending them again puts another copy there.`;
    const all = earlier.length === state.paths.length && earlier.every((e) => state.paths.includes(e.path));
    const again = button(one ? "Send it again" : "Send them again", "btn primary", "send");
    const leave = button(all ? "Don't send" : (one ? "Leave it out" : "Leave them out"), "btn");
    again.addEventListener("click", () => send(d, []));
    leave.addEventListener("click", () => {
      if (all) { box.close(); return; }
      send(d, earlier.map((e) => e.path));
    });
    actions.append(leave, again);
  }

  /** §29: what moved where, and when it landed. */
  function showJourney(r, d) {
    what.replaceChildren();
    to.replaceChildren();
    actions.replaceChildren();
    says.classList.remove("warn");
    const trip = el("div", "journey");
    const end = (name, iconName, cls) => {
      const e = el("div", `end ${cls}`);
      const t = el("span", "tile");
      t.append(icon(iconName));
      e.append(t, el("span", null, name));
      return e;
    };
    trip.append(end("This computer", "laptop", "from"), el("div", "path"), end(d.name, deviceIcon(d.name), "to"));
    what.append(trip);

    if (r.sent === 0) {
      says.textContent = "Nothing was sent.";
    }
    const what1 = r.only ? base(r.only) : count(r.sent, "file");
    // This send's files: each was stored under the name of what was picked,
    // a folder's under the folder's name.
    const picked = new Set(state.paths.map(base));
    const watch = async () => {
      if (!box.isConnected) return;
      let out = [];
      try { out = await invoke("outgoing"); } catch (e) { /* try again */ }
      const mine = out.filter((o) => o.to_id === d.id && picked.has(o.path.split("/")[0]));
      const moving = movingNow.outgoing.some((t) => t.device === d.name);
      if (r.sent > 0 && mine.length === 0) {
        trip.className = "journey landed";
        says.textContent = `Sent. ${d.name} has ${what1}.`;
        return;
      }
      trip.className = moving ? "journey moving" : "journey waiting";
      says.textContent = moving
        ? `Sending ${what1} to ${d.name}…`
        : `${what1[0].toUpperCase()}${what1.slice(1)} ${r.sent === 1 ? "is" : "are"} ready for ${d.name}. It collects ${r.sent === 1 ? "it" : "them"} the next time it's online — you can close this.`;
      setTimeout(watch, 1500);
    };
    watch();
    // What was not sent is said, with why, rather than only counted: "2 files
    // were skipped" leaves somebody guessing which.
    if (r.skipped.length > 0) {
      const skipped = el("p", "note");
      skipped.textContent = `Not sent: ${r.skipped.map((s) => `${base(s.path)} (${s.why})`).join("; ")}.`;
      box.append(skipped);
    }
    const done = button("Done", "btn primary");
    done.addEventListener("click", () => box.close());
    actions.append(done);
    drawChip();
  }

  drawWhat();
  drawTo();
}

// Dragging onto the window, from anywhere in it. Tauri reports these as
// window events rather than DOM ones, because the drag is happening to the
// *window* -- the page never sees the file, and that is the point: a path
// crosses, not the contents.
if (window.__TAURI__?.event) {
  const { listen } = window.__TAURI__.event;
  listen("tauri://drag-over", () => document.querySelector(".drop")?.classList.add("over"));
  listen("tauri://drag-leave", () => document.querySelector(".drop")?.classList.remove("over"));
  listen("tauri://drag-drop", (event) => {
    document.querySelector(".drop")?.classList.remove("over");
    const paths = event.payload?.paths ?? [];
    if (paths.length === 0 || settingUp) return;
    // Everything dropped, files and folders alike. Which device is still the
    // person's choice, so this picks and does not send.
    openSend(paths);
  });
}
