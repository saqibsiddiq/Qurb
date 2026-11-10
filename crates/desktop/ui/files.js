// Files: one search across the shared folder and the Vault, then this
// computer's places as cards two to a row — Shared, Vault, Received,
// Recently deleted — and a folder for each person this computer keeps files
// for. Inside, folders as cards, files as plain rows, or as a gallery where
// a folder is mostly pictures. A file's sheet has its actions as round
// buttons, as on the phone; Space looks at the file under the pointer.

/** The synced folder on this computer, from `situation`: sending a shared
 * file sends it from there. */
let rootPath = "";

/** Paths asked for with Keep here and not here yet. */
const wanted = new Set();

/** What Files is showing: an area (null at the top, "shared", "vault", or a
 * person's id), the folder inside it, and the files drawn, for Space. */
const FILES = { area: null, dir: "", query: "", people: [], shown: [], hover: -1, conflicts: [], asking: null };

function drawFiles(main, at = {}) {
  FILES.area = at.area ?? null;
  FILES.dir = at.dir ?? "";
  FILES.shown = [];
  FILES.hover = -1;
  clearInterval(FILES.asking);
  if (FILES.area === null) return drawFilesTop(main);
  if (FILES.area === "shared" || FILES.area === "vault") return drawArea(main);
  return drawPerson(main);
}

const inFiles = (area, dir = "") => go("files", { area, dir });

// ---------- The top ----------

// Files is a launcher (the owner's review of 2026-11-03, F-1 to F-5): each
// place opens in the computer's own file manager, which browses, previews
// and searches better than a window of ours would. *This computer*: Shared
// and Received, ordinary folders. *People*: each person's folder, locked
// until their phone says yes, then mounted where its placeholder is
// (mount.rs) and opened like any folder. Nothing else on the page.

function drawFilesTop(main) {
  const body = h("div", { id: "files-body" });
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Files", null), body));
  drawTopBody(body);
}

async function drawTopBody(body = $("files-body")) {
  if (!body) return;
  const people = await invoke("people").catch(() => []);
  FILES.people = people;
  if (place !== "files" || !body.isConnected) return;
  const opening = (which) => () => invoke("open_place", { which }).catch((e) => toast(String(e), true));
  put(body,
    label("This computer"),
    h("div", { class: "launch" },
      launchTile(icon("folder"), "Shared", opening("shared"), icon("external-link", "launch-end")),
      launchTile(icon("download"), "Received", opening("received"), icon("external-link", "launch-end"))),
    people.length ? label("People") : null,
    people.length ? h("div", { class: "launch" }, ...people.map(personTile)) : null);
}

function card(ic, name, sub, onclick) {
  return h("button", { class: "card", onclick }, tile(ic), h("span", { class: "card-words" }, h("b", {}, name), h("small", {}, sub)));
}

function launchTile(lead, name, onclick, end, cls = "") {
  return h("div", { class: `launch-tile ${cls}`.trim(), role: "button", tabindex: "0", onclick,
    onkeydown: (e) => { if (e.key === "Enter") onclick(); } },
    h("span", { class: "launch-lead" }, lead), h("b", {}, name), end ? h("span", { class: "launch-side" }, end) : null);
}

/** A person's folder: its lock while locked; open, green, with *Lock*. */
function personTile(p) {
  // Their phone's picture where it chose one, as it shows itself.
  const face = circle({ name: p.name, picture: p.picture }, "circle-md circle-plain");
  if (p.state === "open") {
    return launchTile(face, p.name, () => openPerson(p),
      h("button", { class: "btn btn-quiet btn-sm", onclick: async (e) => {
        e.stopPropagation();
        try { await invoke("lock_person_folder", { person: p.id }); } catch (er) { toast(String(er), true); }
        drawTopBody();
      } }, icon("lock-keyhole"), "Lock"), "launch-open");
  }
  return launchTile(face, p.name, () => askToOpen(p), icon("lock-keyhole", "launch-end"));
}

async function openPerson(p) {
  try { await invoke("open_person_folder", { person: p.id }); } catch (e) { toast(String(e), true); }
  drawTopBody();
}

/** Ask their phone, and say the one thing to do (H-3): open QurbSpace on
 * it. Opened in the file manager the moment the phone says yes; the ask
 * waits twenty minutes (H-4). */
async function askToOpen(p) {
  try { await invoke("ask_person_folder", { person: p.id }); } catch (e) { toast(String(e), true); return; }
  const phone = (p.devices ?? [])[0];
  let watching = null;
  const box = sheet({ onclose: () => clearInterval(watching) });
  box.append(h("div", { class: "asking-phone" },
    h("span", { class: "asking-orb" }, icon("smartphone"), h("i", { class: "asking-spin" })),
    h("h2", {}, phone ? "Open QurbSpace on your phone" : "Open QurbSpace on their phone"),
    phone ? h("p", { class: "note" }, phone) : null),
    actions(btn("Cancel", async () => {
      box.close();
      await invoke("lock_person_folder", { person: p.id }).catch(() => {});
      drawTopBody();
    }, "btn-quiet")));
  watching = setInterval(async () => {
    const now = (await invoke("people").catch(() => [])).find((x) => x.id === p.id);
    if (now?.state === "open") {
      box.close();
      openPerson(now);
    } else if (now && now.state === "locked") {
      // Said no, or the ask lapsed.
      box.close();
      drawTopBody();
    }
  }, 2000);
}

/** Search: the shared folder and the Vault at once. */
async function drawFound(body, text) {
  let shared, vault, kept;
  try {
    [shared, vault, kept] = await Promise.all([
      invoke("find", { text, private: false }),
      invoke("find", { text, private: true }),
      // People's folders, those open now: a locked one is not searched.
      invoke("find_people", { text }).catch(() => []),
    ]);
  } catch (e) {
    put(body, oops(e));
    return;
  }
  if (text !== FILES.query) return;
  const found = [...shared, ...vault];
  const whose = (k) => FILES.people.find((p) => p.id === k.person)?.name ?? "Folder";
  FILES.shown = [...found.map(sharedItem), ...kept.map((k) => personItem(k.person, k))];
  const keptRows = kept.map((k, j) => {
    const r = row({
      ic: fileIcon(k.name), name: base(k.name),
      sub: [size(k.size), folderOf(k.name) || whose(k)],
      end: h("span", { class: "chip chip-quiet" }, whose(k)),
      onclick: () => personItem(k.person, k).open().catch((e) => toast(String(e), true)),
    });
    hoverable(r, found.length + j);
    return r;
  });
  const locked = FILES.people.filter((p) => p.state !== "open").length;
  put(body,
    label(count(found.length + kept.length, "result")),
    found.length + kept.length
      ? h("div", { class: "rows" }, ...found.map((f, i) => fileRow(f, i, true)), ...keptRows)
      : empty("search", "Nothing by that name"),
    locked ? h("p", { class: "note" }, locked === 1 ? "A locked folder isn't searched" : `${locked} locked folders aren't searched`) : null);
}

/** A file of a person's folder, as quick preview and the gallery see it. */
function personItem(person, f) {
  const own = person === "owner";
  return {
    name: base(f.name),
    here: true,
    small: () => (own ? invoke("backed_up_preview", { phone: f.device, path: f.path, small: true }) : Promise.resolve(null)),
    large: () => (own ? invoke("backed_up_preview", { phone: f.device, path: f.path, small: false }) : Promise.resolve({ kind: "none" })),
    media: own ? () => invoke("peek_kept", { device: f.device, path: f.path }) : null,
    open: () => invoke("open_person_file", { person, device: f.device ?? null, path: f.path }),
  };
}

// ---------- The shared folder and the Vault ----------

async function drawArea(main) {
  const vault = FILES.area === "vault";
  const top = vault ? "Vault" : "Shared";
  const parts = FILES.dir ? FILES.dir.split("/") : [];
  const up = parts.length ? { words: parts.length > 1 ? parts[parts.length - 2] : top, go: () => inFiles(FILES.area, parts.slice(0, -1).join("/")) }
    : { words: "Files", go: () => go("files") };
  const more = h("button", { class: "icon-btn", "aria-label": "More", onclick: () => areaSheet() }, icon("ellipsis"));
  const body = h("div", {});
  main.append(h("div", { class: "pg" }, pageHead(parts.length ? parts[parts.length - 1] : top, up, more), body));
  let d;
  try {
    d = await invoke("browse", { dir: FILES.dir, private: vault });
  } catch (e) {
    put(body, oops(e));
    return;
  }
  FILES.shown = d.files.map(sharedItem);
  const folders = d.folders.map((name) => {
    const path = FILES.dir ? `${FILES.dir}/${name}` : name;
    const c = card("folder", name, null, () => inFiles(FILES.area, path));
    c.querySelector("small").remove();
    // Who has a top folder, and whether it is kept here: its own sheet,
    // from a right click, set once and rarely looked at again.
    if (!vault && !FILES.dir) c.addEventListener("contextmenu", (e) => { e.preventDefault(); folderSheet(name); });
    return c;
  });
  put(body, 
    folders.length ? h("div", { class: "cards" }, ...folders) : null,
    d.files.length ? filesOf(d.files, (f, i) => fileRow(f, i, false), sharedItem) : null,
    !d.files.length && !d.folders.length
      ? empty(vault ? "lock-keyhole" : "folder-open", FILES.dir ? "Empty" : vault ? "Files kept only on this computer come here" : "Put files in your QurbSpace folder")
      : null);
}

/** The area's own sheet: its folder's options, or showing it on disk. */
function areaSheet() {
  const box = sheet({ ic: FILES.area === "vault" ? "lock-keyhole" : "folder", title: FILES.area === "vault" ? "Vault" : "Shared" });
  const top = FILES.area === "shared" ? FILES.dir.split("/")[0] : "";
  rounds(box, [
    { ic: "external-link", words: "Show", act: () => invoke("show_root").catch((e) => toast(String(e), true)) },
    top ? { ic: "sliders-horizontal", words: "Who has it", act: () => folderSheet(top) } : null,
    { ic: "trash-2", words: "Deleted", act: () => go("deleted") },
  ]);
}

const isHere = (f) => f.availability === "here" || f.availability === "only here";

/** Where a file's bytes are, as an icon at its row's end: nothing when it is
 * here, as nearly every file is. */
function whereIcon(f) {
  if (wanted.has(f.path) && f.availability === "elsewhere") return h("span", { class: "where moving", title: "Downloading" }, icon("download"));
  if (f.availability !== "elsewhere") wanted.delete(f.path);
  if (f.availability === "elsewhere") return h("span", { class: "where", title: "On your other devices" }, icon("cloud"));
  if (f.availability === "nowhere") return h("span", { class: "where warn", title: "On no device" }, icon("circle-alert"));
  if (f.availability === "only here") return h("span", { class: "where", title: "Only on this computer" }, icon("laptop"));
  return null;
}

function fileRow(f, i, found) {
  const r = row({
    ic: fileIcon(f.path),
    name: base(f.path),
    sub: [size(f.size), found ? (folderOf(f.path) || (f.private ? "Vault" : "Shared")) : ago(f.modified)],
    end: [found && f.private ? h("span", { class: "chip chip-quiet" }, "Vault") : null, whereIcon(f)],
    onclick: () => fileSheet(f),
  });
  hoverable(r, i);
  r.addEventListener("contextmenu", (e) => { e.preventDefault(); fileSheet(f); });
  return r;
}

/** A row or a picture that Space looks at, while the pointer or focus is on it. */
function hoverable(el, i) {
  el.addEventListener("mouseenter", () => { FILES.hover = i; });
  el.addEventListener("focus", () => { FILES.hover = i; });
  el.addEventListener("mouseleave", () => { if (FILES.hover === i) FILES.hover = -1; });
}

/**
 * Files as rows, or as a gallery when most of them are pictures — a phone's
 * camera folder, a holiday. The gallery's pictures are the small ones a
 * camera keeps inside each photo, asked for as they scroll into view.
 */
function filesOf(files, asRow, source) {
  const name = (f) => f.name ?? base(f.path);
  const pictures = files.filter((f) => kindOf(name(f)) === "image").length;
  if (pictures < 4 || pictures < files.length / 2) {
    return h("div", { class: "rows" }, label("Files"), ...files.slice(0, 1000).map(asRow),
      files.length > 1000 ? h("p", { class: "note" }, `First 1,000 of ${files.length.toLocaleString()}`) : null);
  }
  const grid = h("div", { class: "gallery" });
  const seen = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      seen.unobserve(e.target);
      thumbs.push(e.target);
      pump();
    }
  }, { root: $("main"), rootMargin: "400px" });
  const thumbs = [];
  let loading = 0;
  // Three at a time: a gallery scrolled quickly asks for what is in view,
  // not for everything it passed.
  const pump = () => {
    while (loading < 3 && thumbs.length) {
      const el = thumbs.shift();
      if (!el.isConnected) continue;
      loading++;
      el.load().then((p) => {
        if (p?.kind === "image") put(el, h("img", { src: p.data, alt: "", loading: "lazy" }));
      }).catch(() => {}).finally(() => { loading--; pump(); });
    }
  };
  files.slice(0, 3000).forEach((f, i) => {
    const s = source(f);
    const cell = h("button", { class: "pic", title: name(f), onclick: () => peek(i) }, icon(fileIcon(name(f))));
    cell.load = () => (kindOf(name(f)) === "image" && s.here !== false ? s.small() : Promise.resolve(null));
    hoverable(cell, i);
    grid.append(cell);
    seen.observe(cell);
  });
  return h("div", {}, label(count(files.length, "file")), grid,
    files.length > 3000 ? h("p", { class: "note" }, `First 3,000 of ${files.length.toLocaleString()}`) : null);
}

/** A shared or Vault file as quick preview and the gallery see it. */
function sharedItem(f) {
  return {
    name: base(f.path),
    here: isHere(f),
    small: () => invoke("preview", { path: f.path, small: true }),
    large: () => invoke("preview", { path: f.path }),
    media: () => invoke("peek", { path: f.path }),
    open: () => invoke("open_file", { path: f.path }),
    sheet: () => fileSheet(f),
  };
}

// ---------- A file's sheet ----------

async function fileSheet(f) {
  const name = base(f.path);
  const here = isHere(f);
  const where = h("span", { class: "chip chip-quiet" }, f.private ? "Vault" : "Shared", folderOf(f.path) ? h("span", {}, folderOf(f.path)) : null);
  const box = sheet({ ic: fileIcon(name), title: name, sub: h("div", { class: "chips" }, where, h("span", { class: "chip chip-quiet" }, size(f.size))) });
  const elsewhere = f.availability === "elsewhere" && !wanted.has(f.path);
  rounds(box, [
    here && peekable(name) ? { ic: "image", words: "Preview", act: () => peekOne(sharedItem(f)) } : null,
    here ? { ic: "external-link", words: "Open", act: () => invoke("open_file", { path: f.path }).catch((e) => toast(String(e), true)) } : null,
    here ? { ic: "folder-open", words: "Show", act: () => invoke("show_file", { path: f.path }).catch((e) => toast(String(e), true)) } : null,
    elsewhere ? { ic: "download", words: "Keep here", act: () => keepHere(f.path) } : null,
    here ? { ic: "send", words: "Send", act: () => send([`${rootPath}/${f.path}`]) } : null,
    f.availability === "here" ? { ic: "cloud-off", words: "Free up", act: () => freeUp(f.path) } : null,
    here ? (f.private
      ? { ic: "folder", words: "To Shared", act: () => moveArea(f, false) }
      : { ic: "lock-keyhole", words: "To Vault", act: () => moveArea(f, true) }) : null,
    { ic: "trash-2", words: "Delete", danger: true, act: () => deleteFile(f.path) },
  ]);
  // Where its bytes are, under the actions: the devices that have it.
  const holders = h("div", { class: "holders" });
  box.append(holders);
  try {
    const d = await invoke("details", { path: f.path });
    const on = [...(here ? ["This computer"] : []), ...d.holders];
    holders.append(label("On"), h("div", { class: "chips" }, ...(on.length ? on : ["No device yet"]).map((n) => h("span", { class: "chip chip-quiet" }, n))));
    if (f.availability === "only here") holders.append(h("p", { class: "note" }, "The only copy. It reaches your other devices when one is online."));
    if (f.availability === "nowhere") holders.append(h("p", { class: "note" }, "No device has it any more."));
    const conflict = FILES.conflicts.find((c) => c.path === f.path || c.other.path === f.path);
    if (conflict) holders.append(row({ ic: tile("git-compare", "tile-warn"), name: "Two versions", cls: "row-warn", onclick: () => { box.close(); go("conflicts"); }, end: icon("chevron-right", "row-go") }));
  } catch (e) { /* the actions are enough */ }
}

const redraw = () => { if (place === "files") go("files", { area: FILES.area, dir: FILES.dir }); };

async function keepHere(path) {
  try {
    await invoke("fetch", { path });
    wanted.add(path);
    toast(`Downloading ${base(path)}`);
    redraw();
  } catch (e) { toast(String(e), true); }
}

async function freeUp(path) {
  try {
    const freed = await invoke("free_file", { path });
    toast(`Freed ${size(freed)}`);
    redraw();
  } catch (e) {
    toast(/no other device is known to hold|only holder/.test(String(e)) ? "The only copy can't be freed" : String(e), true);
  }
}

async function deleteFile(path) {
  try {
    await invoke("delete_file", { path });
    toast("Deleted. In Recently deleted for 30 days");
    redraw();
  } catch (e) { toast(String(e), true); }
}

/** Into the Vault (decision 0057): the other devices' copies go, so it says
 * so first, and whether any device still keeps the Vault. */
async function moveArea(f, intoVault) {
  const move = async () => {
    try {
      await invoke("move_file_area", { path: f.path, private: intoVault });
      toast(intoVault ? "In your Vault" : "On all your devices");
      redraw();
    } catch (e) { toast(String(e), true); }
  };
  if (!intoVault) return move();
  const keepers = await invoke("vault_keepers").catch(() => []);
  const box = sheet({ ic: "lock-keyhole", title: "Move to Vault?", sub: "Your other devices remove their copies" });
  box.append(
    h("p", { class: keepers.length ? "note" : "note warn" }, keepers.length ? `${keepers.join(", ")} keeps your Vault too` : "This computer will have the only copy"),
    actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Move", () => { box.close(); move(); }, "btn-main")));
}

/** Who has a top folder (decision 0044), and whether this computer keeps its
 * files or only lists them (0045). Switches apply at once. */
async function folderSheet(folder) {
  let sharing;
  try { sharing = await invoke("sharing"); } catch (e) { toast(String(e), true); return; }
  const f = sharing.folders.find((x) => x.folder === folder);
  if (!f) { toast("Put a file in it first", true); return; }
  const box = sheet({ ic: "folder", title: folder, onclose: redraw });
  let members = f.everyone ? sharing.devices.map((d) => d.id) : [...f.members];
  const save = async () => {
    await invoke("set_sharing", { folder, members: members.length === sharing.devices.length ? [] : members });
  };
  box.append(group("On", ...sharing.devices.map((d) => setting({
    ic: d.here ? "laptop" : "smartphone",
    name: d.name,
    end: toggle(members.includes(d.id), async (on) => {
      members = on ? [...members, d.id] : members.filter((m) => m !== d.id);
      await save();
    }, d.name),
  }))));
  box.append(group("This computer", setting({
    ic: f.remote ? "cloud" : "hard-drive",
    name: "Keep its files here",
    end: toggle(!f.remote, async (on) => {
      if (on) {
        const asked = await invoke("keep_locally", { folder });
        toast(asked ? `${count(asked, "file")} on the way` : "Kept here");
      } else {
        const r = await invoke("keep_remotely", { folder });
        toast(`Freed ${size(r.bytes)}`);
      }
    }, "Keep its files here"),
  })));
  box.append(h("p", { class: "note" }, "A device left out keeps what it has, and gets nothing new"));
}

// ---------- A person's folder ----------

async function drawPerson(main) {
  const person = FILES.area;
  if (!FILES.people.some((p) => p.id === person)) FILES.people = await invoke("people").catch(() => []);
  const p = FILES.people.find((x) => x.id === person) ?? { id: person, name: "Folder", relation: "guest" };
  const own = p.relation === "owner";
  const parts = FILES.dir ? FILES.dir.split("/") : [];
  const up = parts.length ? { words: parts.length > 1 ? parts[parts.length - 2] : p.name, go: () => inFiles(person, parts.slice(0, -1).join("/")) }
    : { words: "Files", go: () => go("files") };
  const end = h("span", {});
  const body = h("div", {});
  main.append(h("div", { class: "pg" }, pageHead(parts.length ? parts[parts.length - 1] : p.name, up, end), body));

  let answer;
  try { answer = await invoke("person_folder", { person, dir: FILES.dir }); } catch (e) { put(body, oops(e)); return; }
  if (place !== "files" || FILES.area !== person) return;

  if (answer.state !== "open") {
    const asking = answer.state === "asking";
    put(body, h("div", { class: "locked" },
      h("span", { class: `empty-orb${asking ? " asking" : ""}` }, icon(asking ? "smartphone" : "lock-keyhole")),
      h("p", {}, asking ? (own ? "Approve on your phone" : `Waiting for ${p.name}`) : (own ? "Opens with a yes on your phone" : `Sealed. Opens with ${p.name}'s yes`)),
      asking ? null : h("button", { class: "btn btn-main", onclick: async (e) => {
        e.currentTarget.disabled = true;
        try { await invoke("ask_person_folder", { person }); } catch (er) { toast(String(er), true); }
        inFiles(person, FILES.dir);
      } }, own ? "Ask your phone" : `Ask ${p.name}`)));
    if (asking) {
      FILES.asking = setInterval(() => {
        if (place !== "files" || FILES.area !== person) { clearInterval(FILES.asking); return; }
        invoke("person_folder", { person, dir: FILES.dir }).then((a) => { if (a.state !== "asking") inFiles(person, FILES.dir); }).catch(() => {});
      }, 1500);
    }
    return;
  }

  end.append(h("button", { class: "btn btn-quiet btn-sm", onclick: async () => {
    try { await invoke("lock_person_folder", { person }); } catch (e) { toast(String(e), true); }
    inFiles(person);
  } }, icon("lock-keyhole"), "Lock"));
  const item = (f) => personItem(person, f);
  FILES.shown = answer.files.map(item);
  put(body, 
    answer.folders.length ? h("div", { class: "cards" }, ...answer.folders.map((name) => {
      const c = card(!FILES.dir && own ? "smartphone" : "folder", name, null, () => inFiles(person, FILES.dir ? `${FILES.dir}/${name}` : name));
      c.querySelector("small").remove();
      return c;
    })) : null,
    answer.files.length ? filesOf(answer.files, (f, i) => {
      const r = row({ ic: fileIcon(f.name), name: f.name, sub: [size(f.size), ago(f.modified_at)], onclick: () => item(f).open().catch((e) => toast(String(e), true)) });
      hoverable(r, i);
      return r;
    }, item) : null,
    !answer.files.length && !answer.folders.length ? empty("folder-open", "Nothing kept here yet") : null,
    h("p", { class: "note" }, "Locks itself after ten minutes unused"));
}

// ---------- Quick preview ----------

/** Space, on a file under the pointer: the picture or the text, large, with
 * the arrow keys to the next and the one before. */
function peek(i) {
  const list = FILES.shown;
  if (!list[i]) return;
  let at = i;
  const stage = h("div", { class: "peek-stage" });
  const name = h("b", {});
  const shade = h("div", { class: "peek", role: "dialog", "aria-modal": "true" },
    h("div", { class: "peek-bar" }, name,
      h("button", { class: "btn btn-sm btn-onpeek", onclick: () => list[at].open().catch((e) => toast(String(e), true)) }, icon("external-link"), "Open"),
      h("button", { class: "icon-btn", "aria-label": "Close", onclick: () => close() }, icon("x"))),
    stage,
    list.length > 1 ? h("button", { class: "peek-nav prev", "aria-label": "Previous", onclick: () => step(-1) }, icon("arrow-left")) : null,
    list.length > 1 ? h("button", { class: "peek-nav next", "aria-label": "Next", onclick: () => step(1) }, icon("arrow-left")) : null);
  shade.addEventListener("mousedown", (e) => { if (e.target === shade || e.target === stage) close(); });
  const keys = (e) => {
    if (e.key === "Escape" || e.key === " ") { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === "ArrowRight") step(1);
    else if (e.key === "ArrowLeft") step(-1);
  };
  function close() { document.removeEventListener("keydown", keys, true); shade.remove(); }
  function step(by) { at = (at + by + list.length) % list.length; show(); }
  async function show() {
    const it = list[at];
    const mine = at;
    name.textContent = it.name;
    put(stage, h("span", { class: "peek-icon" }, icon(fileIcon(it.name))));
    if (!it.here) { stage.append(h("p", {}, "Not on this computer")); return; }
    if (!peekable(it.name)) return;
    try {
      if (played(it.name)) {
        if (!it.media) return;
        const m = await it.media();
        if (mine !== at) return;
        put(stage, m.kind === "pdf" ? h("iframe", { class: "peek-pdf", src: m.url, title: it.name })
          : h(m.kind, { src: m.url, controls: true, autoplay: true, class: `peek-${m.kind}` }));
        return;
      }
      const p = await it.large();
      if (mine !== at) return;
      if (p.kind === "image") put(stage, h("img", { src: p.data, alt: it.name }));
      else if (p.kind === "text") put(stage, h("pre", {}, p.data));
    } catch (e) { stage.append(h("p", {}, String(e))); }
  }
  document.addEventListener("keydown", keys, true);
  document.body.append(shade);
  show();
}

function peekOne(item) {
  FILES.shown = [item];
  peek(0);
}

document.addEventListener("keydown", (e) => {
  if (e.key !== " " || FILES.hover < 0 || place !== "files") return;
  if (e.target.closest?.("input, textarea") || document.querySelector(".scrim, .peek")) return;
  e.preventDefault();
  peek(FILES.hover);
});

// ---------- Received, Recently deleted, two versions ----------

async function drawReceived(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg pg-narrow" },
    pageHead("Received", { words: "Files", go: () => go("files") },
      h("button", { class: "btn btn-quiet btn-sm", onclick: () => invoke("open_downloads").catch((e) => toast(String(e), true)) }, icon("folder-open"), "Open folder")),
    body));
  try {
    const rows = (await invoke("activity", { path: null, limit: 300, before: null }))
      .filter((r) => r.kind === "received" && (r.detail ?? "").startsWith("sent to this device"));
    put(body, rows.length
      ? h("div", { class: "grp-box rows-card" }, ...rows.map((r) => row({
        ic: fileIcon(r.path), name: base(r.path), sub: [r.device ?? "Another device", ago(r.at)],
        onclick: () => invoke("show_received", { id: r.id }).catch((e) => toast(String(e), true)),
      })))
      : empty("download", "Files sent to this computer come here"));
  } catch (e) { put(body, oops(e)); }
}

// Recently deleted (K-2), reached from This computer's storage: each file's
// preview where it can be had, its name and the device that deleted it; ↺
// puts it back, × deletes it for good; how long it has left only in its last
// week. Empty at the top asks first. What the owner's phone deleted from its
// Private Vault is listed too, while the owner's folder is open -- put back
// from the phone, whose Recently deleted keeps it.
// Recently deleted, as Photos keeps its own (D13, D13b): the files
// themselves in a grid, by where they were deleted; days left only in the
// last week, on the picture, in amber. Restore all and Delete all at the
// foot; Select to choose some, the title then saying how many, and the bar
// holding only Restore -- green, as Send is -- and Delete. What a phone's
// Vault deleted shows too, put back from the phone.

const DELETED = { selecting: false, chosen: new Set(), entries: [] };

async function drawDeleted(main) {
  DELETED.selecting = false;
  DELETED.chosen.clear();
  main.append(h("div", { class: "pg pg-narrow" }, h("div", { id: "deleted-page" })));
  let phone;
  try { [DELETED.entries, phone] = await Promise.all([invoke("recently_deleted"), invoke("phone_deleted").catch(() => null)]); } catch (e) { put($("deleted-page"), oops(e)); return; }
  DELETED.phone = phone ?? [];
  drawDeletedPage();
}

function drawDeletedPage() {
  const page = $("deleted-page");
  if (!page) return;
  const { entries, phone, chosen } = DELETED;
  const selecting = DELETED.selecting;
  const back = { words: "This computer", go: () => go("storage") };
  const head = selecting
    ? h("div", { class: "pg-hd" },
      h("button", { class: "back", onclick: back.go }, icon("arrow-left"), back.words),
      h("div", { class: "pg-title" },
        h("button", { class: "icon-btn sel-x", "aria-label": "Stop choosing", onclick: () => { DELETED.selecting = false; chosen.clear(); drawDeletedPage(); } }, icon("x")),
        h("h1", {}, `${chosen.size} selected`),
        btn("All", () => { entries.forEach((d) => chosen.add(d.id)); drawDeletedPage(); }, "btn-quiet btn-sm")))
    : pageHead("Recently deleted", back,
      entries.length ? btn("Select", () => { DELETED.selecting = true; drawDeletedPage(); }, "btn-quiet btn-sm") : null);
  const dayLeft = (at) => Math.max(1, 30 - Math.floor((Date.now() / 1000 - at) / 86400));
  const cell = (path, at, id) => {
    const th = h("span", { class: "rd-th" });
    previewInto(th, path);
    if (kindOf(path) === "video") th.append(h("span", { class: "wa-play" }));
    if (dayLeft(at) <= 7) th.append(h("span", { class: "rd-days" }, dayLeft(at) === 1 ? "1 day" : `${dayLeft(at)} days`));
    if (selecting && id != null) th.append(h("span", { class: `rd-tick${chosen.has(id) ? "" : " off"}` }, chosen.has(id) ? icon("check") : null));
    const on = id != null && chosen.has(id);
    return h(id != null ? "button" : "div", {
      class: `rd${on ? " sel" : ""}`, title: id != null ? base(path) : `${base(path)}: put back on the phone`,
      onclick: id != null ? () => {
        if (!DELETED.selecting) { DELETED.selecting = true; }
        chosen.has(id) ? chosen.delete(id) : chosen.add(id);
        drawDeletedPage();
      } : null,
    }, th, h("small", {}, base(path)));
  };
  const byPhone = new Map();
  for (const d of phone) {
    if (!byPhone.has(d.by)) byPhone.set(d.by, []);
    byPhone.get(d.by).push(d);
  }
  const sections = [];
  if (entries.length) sections.push(label("This computer"), h("div", { class: "rdg" }, ...entries.map((d) => cell(d.path, d.at, d.id))));
  for (const [by, list] of byPhone) {
    sections.push(h("div", { class: "label label-who" }, h("span", { class: "circle circle-xs" }, (by || "?")[0].toUpperCase()), `${by}'s Vault`),
      h("div", { class: "rdg" }, ...list.map((d) => cell(d.path, d.at, null))));
  }
  const bar = !entries.length ? null : selecting
    ? h("div", { class: "selbar" },
      h("button", { class: "btn btn-main btn-sm", disabled: !chosen.size, onclick: () => restoreDeleted([...chosen]) }, icon("rotate-ccw"), "Restore"),
      h("button", { class: "btn btn-danger-solid btn-sm", disabled: !chosen.size, onclick: () => forgetDeleted([...chosen]) }, icon("trash-2"), "Delete"))
    : h("div", { class: "selbar" },
      btn("Restore all", () => restoreDeleted(entries.map((d) => d.id)), "btn-main btn-sm"),
      btn("Delete all", () => askEmpty(), "btn-quiet btn-sm btn-red-words"));
  put(page, head,
    h("p", { class: "quiet-line" }, "Kept for 30 days, then deleted for good"),
    ...(sections.length ? sections : [empty("trash-2", "Nothing deleted lately")]),
    bar);
  icons(page);
}

async function restoreDeleted(ids) {
  let renamed = 0;
  for (const id of ids) {
    const d = DELETED.entries.find((x) => x.id === id);
    try {
      const at = await invoke("restore_deleted", { id });
      if (d && at !== d.path) renamed++;
    } catch (e) { toast(String(e), true); }
  }
  toast(renamed ? `Restored, ${renamed} under a new name` : "Restored");
  go("deleted");
}

function forgetDeleted(ids) {
  const box = sheet({ ic: "trash-2", title: ids.length === 1 ? "Delete for good?" : `Delete ${ids.length} for good?`, sub: "They can't be restored" });
  box.append(actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Delete", async () => {
    box.close();
    for (const id of ids) {
      try { await invoke("forget_deleted", { id }); } catch (e) { toast(String(e), true); }
    }
    go("deleted");
  }, "btn-danger-solid")));
}

function askEmpty() {
  const box = sheet({ ic: "trash-2", title: "Delete all for good?", sub: "They can't be restored" });
  box.append(actions(btn("Cancel", () => box.close(), "btn-quiet"), btn("Delete all", async () => {
    try { await invoke("empty_deleted"); box.close(); toast("Deleted"); go("deleted"); } catch (e) { toast(String(e), true); }
  }, "btn-danger-solid")));
}

/** Two versions, as the phone's sheet from Home's mark (K-1): straight to
 * the file when there is one, a short list to choose from when several. */
async function conflictsSheet() {
  const found = await invoke("conflicts").catch(() => []);
  if (!found.length) { toast("Every file has one version"); return; }
  const one = (c) => {
    const box = sheet({ wide: true, title: base(c.path), sub: "Changed on two devices at once" });
    box.append(conflictBlock(c));
    icons(box);
  };
  if (found.length === 1) return one(found[0]);
  const box = sheet({ ic: "git-compare", title: `${found.length} files have two versions` });
  box.append(h("div", { class: "rows" }, ...found.map((c) => row({ ic: fileIcon(c.path), name: base(c.path), sub: folderOf(c.path) || "Shared", onclick: () => { box.close(); one(c); } }))));
}

/** Two versions of a file (brief §14): both kept, side by side, and a choice. */
async function drawConflicts(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg" }, pageHead("Two versions", { words: "Files", go: () => go("files") }), body));
  let found;
  try { found = await invoke("conflicts"); } catch (e) { put(body, oops(e)); return; }
  if (!found.length) { put(body, empty("circle-check", "Every file has one version")); return; }
  put(body, h("p", { class: "note" }, "Changed on two devices at once. Nothing is lost whichever you keep"),
    ...found.map(conflictBlock));
}

function conflictBlock(c) {
  // Each version's text, once both are read, with the lines the other does
  // not have marked: this computer's in red, the other's in green (D15).
  const texts = {};
  const marked = () => {
    if (texts.this == null || texts.other == null) return;
    for (const [mine, theirs, cls] of [["this", "other", "gone"], ["other", "this", "new"]]) {
      const others = new Set(texts[theirs].text.split("\n"));
      put(texts[mine].pre, ...texts[mine].text.split("\n").flatMap((line, i) => [
        i ? "\n" : "", others.has(line) ? line : h("span", { class: `ver-${cls}` }, line)]));
    }
  };
  const side = (which, v) => {
    const col = h("div", { class: "ver" });
    if (!v) { col.append(h("p", { class: "note" }, "Gone since")); return col; }
    // The preview first, then whose and how long ago, under it (K-1).
    const look = h("div", { class: "ver-look" });
    col.append(look, h("div", { class: "ver-by" }, icon(v.kind === "phone" ? "smartphone" : "laptop"), h("b", {}, which === "this" ? "This computer" : named(v.by)), h("small", {}, ago(v.at))));
    if (!v.here) { look.append(h("p", { class: "note" }, "Not on this computer yet")); return col; }
    invoke("preview", { path: v.path }).then((p) => {
      if (p.kind === "image") look.append(h("img", { src: p.data, alt: "" }));
      else if (p.kind === "text") { const pre = h("pre", {}, p.data); look.append(pre); texts[which] = { pre, text: p.data }; marked(); }
      else look.append(icon(fileIcon(v.path), "ver-ic"));
    }, () => {});
    return col;
  };
  const block = h("section", { class: "conflict" },
    h("div", { class: "conflict-hd" }, tile(fileIcon(c.path)), h("span", { class: "row-main" }, h("b", {}, base(c.path)), h("small", {}, folderOf(c.path) || "Shared"))),
    h("div", { class: "vers" }, side("this", c.this), side("other", c.other)));
  const keep = (words, how, needsOther) => {
    const b = btn(words, async () => {
      block.querySelectorAll(".acts button").forEach((x) => (x.disabled = true));
      try {
        const kept = await invoke("settle_conflict", { other: c.other.path, keep: how });
        block.replaceWith(row({ ic: tile("circle-check", "tile-ok"), name: how === "both" ? "Kept both" : `Kept ${base(kept)}`, sub: how === "both" ? `The other is ${base(kept)}` : "The other is in Recently deleted" }));
      } catch (e) { block.querySelectorAll(".acts button").forEach((x) => (x.disabled = false)); toast(String(e), true); }
    }, "btn-quiet");
    if (needsOther && !c.other.here) { b.disabled = true; b.title = "Not on this computer yet"; }
    return b;
  };
  // Named after the devices, not "this one" and "the other" (K-1).
  const other = c.other ? named(c.other.by) : "the other";
  const both = keep("Keep both", "both", true);
  both.classList.replace("btn-quiet", "btn-soft");
  block.append(h("div", { class: "keeps" },
    keep("Keep this computer's", "this", false), keep(`Keep ${other}'s`, "other", true), both));
  return block;
}
