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

function drawFilesTop(main) {
  const field = h("input", { class: "find-in", type: "search", placeholder: "Search", spellcheck: "false", "aria-label": "Search your files" });
  const body = h("div", { id: "files-body" });
  main.append(h("div", { class: "pg" },
    pageHead("Files", null),
    h("label", { class: "find" }, icon("search"), field),
    body));
  let typing = null;
  field.addEventListener("input", () => {
    clearTimeout(typing);
    typing = setTimeout(() => { FILES.query = field.value.trim(); drawTopBody(body); }, 180);
  });
  field.value = FILES.query;
  drawTopBody(body);
}

async function drawTopBody(body) {
  if (FILES.query) return drawFound(body, FILES.query);
  const [people, conflicts, deleted] = await Promise.all([
    invoke("people").catch(() => []),
    invoke("conflicts").catch(() => []),
    invoke("recently_deleted").catch(() => []),
  ]);
  FILES.people = people;
  FILES.conflicts = conflicts;
  if (FILES.query || place !== "files") return;
  put(body, 
    conflicts.length ? row({ ic: tile("git-compare", "tile-warn"), name: conflicts.length === 1 ? `${base(conflicts[0].path)} has two versions` : `${conflicts.length} files have two versions`, cls: "row-warn", onclick: () => go("conflicts"), end: icon("chevron-right", "row-go") }) : null,
    label("This computer"),
    h("div", { class: "cards" },
      card("folder", "Shared", "On all your devices", () => inFiles("shared")),
      card("lock-keyhole", "Vault", "Only here", () => inFiles("vault")),
      card("download", "Received", "Sent to this computer", () => go("received")),
      card("trash-2", "Recently deleted", deleted.length ? count(deleted.length, "file") : "Kept 30 days", () => go("deleted"))),
    people.length ? label("People") : null,
    people.length ? h("div", { class: "cards" }, ...people.map((p) => card(
      p.state === "open" ? "folder-open" : "lock-keyhole",
      p.name,
      Number(p.size) > 0 ? size(p.size) : "Nothing kept yet",
      () => inFiles(p.id)))) : null);
}

function card(ic, name, sub, onclick) {
  return h("button", { class: "card", onclick }, tile(ic), h("span", { class: "card-words" }, h("b", {}, name), h("small", {}, sub)));
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
  main.append(h("div", { class: "pg" },
    pageHead("Received", { words: "Files", go: () => go("files") },
      h("button", { class: "btn btn-quiet btn-sm", onclick: () => invoke("open_downloads").catch((e) => toast(String(e), true)) }, icon("folder-open"), "Open folder")),
    body));
  try {
    const rows = (await invoke("activity", { path: null, limit: 300, before: null }))
      .filter((r) => r.kind === "received" && (r.detail ?? "").startsWith("sent to this device"));
    put(body, rows.length
      ? h("div", { class: "rows" }, ...rows.map((r) => row({
        ic: fileIcon(r.path), name: base(r.path), sub: [r.device ?? "Another device", ago(r.at)],
        onclick: () => invoke("show_received", { id: r.id }).catch((e) => toast(String(e), true)),
      })))
      : empty("download", "Files sent to this computer come here"));
  } catch (e) { put(body, oops(e)); }
}

async function drawDeleted(main) {
  const body = h("div", {});
  main.append(h("div", { class: "pg" }, pageHead("Recently deleted", { words: "Files", go: () => go("files") }), body));
  let entries;
  try { entries = await invoke("recently_deleted"); } catch (e) { put(body, oops(e)); return; }
  if (!entries.length) { put(body, empty("trash-2", "Nothing deleted lately")); return; }
  put(body, h("div", { class: "rows" }, ...entries.map((d) => {
    const left = Math.max(0, 30 - Math.floor((Date.now() / 1000 - d.at) / 86400));
    const forget = h("button", { class: "icon-btn", title: "Delete now", "aria-label": `Delete ${base(d.path)} now` }, icon("x"));
    // Twice: the first press asks, in red, and the second deletes for good.
    forget.addEventListener("click", async () => {
      if (!forget.classList.contains("armed")) { forget.classList.add("armed"); put(forget, "Delete"); return; }
      try { await invoke("forget_deleted", { id: d.id }); go("deleted"); } catch (e) { toast(String(e), true); }
    });
    return row({
      ic: fileIcon(d.path), name: base(d.path), title: d.why ?? undefined,
      sub: [d.of ? d.of : d.private ? "Vault" : null, named(d.by) ?? null, ago(d.at), left ? `${count(left, "day")} left` : "Goes today"],
      end: [h("button", { class: "icon-btn", title: "Restore", "aria-label": `Restore ${base(d.path)}`, onclick: async () => {
        try {
          const at = await invoke("restore_deleted", { id: d.id });
          toast(at !== d.path ? `Restored as ${base(at)}` : "Restored");
          go("deleted");
        } catch (e) { toast(String(e), true); }
      } }, icon("rotate-ccw")), forget],
    });
  })));
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
  const side = (words, v) => {
    const col = h("div", { class: "ver" }, label(words));
    if (!v) { col.append(h("p", { class: "note" }, "Gone since")); return col; }
    col.append(h("div", { class: "ver-by" }, icon(v.kind === "phone" ? "smartphone" : "laptop"), h("b", {}, named(v.by))),
      h("small", { class: "ver-sub" }, h("span", {}, ago(v.at)), h("span", {}, size(v.size))));
    const look = h("div", { class: "ver-look" });
    col.append(look);
    if (!v.here) { look.append(h("p", { class: "note" }, "Not on this computer yet")); return col; }
    invoke("preview", { path: v.path }).then((p) => {
      if (p.kind === "image") look.append(h("img", { src: p.data, alt: "" }));
      else if (p.kind === "text") look.append(h("pre", {}, p.data));
      else look.append(icon(fileIcon(v.path), "ver-ic"));
    }, () => {});
    return col;
  };
  const block = h("section", { class: "conflict" },
    h("div", { class: "conflict-hd" }, tile(fileIcon(c.path)), h("span", { class: "row-main" }, h("b", {}, base(c.path)), h("small", {}, folderOf(c.path) || "Shared"))),
    h("div", { class: "vers" }, side("This one", c.this), side("The other", c.other)));
  const keep = (words, how, needsOther) => {
    const b = btn(words, async () => {
      block.querySelectorAll(".acts button").forEach((x) => (x.disabled = true));
      try {
        const kept = await invoke("settle_conflict", { other: c.other.path, keep: how });
        block.replaceWith(row({ ic: tile("circle-check", "tile-ok"), name: how === "both" ? "Kept both" : `Kept ${base(kept)}`, sub: how === "both" ? `The other is ${base(kept)}` : "The other is in Recently deleted" }));
      } catch (e) { block.querySelectorAll(".acts button").forEach((x) => (x.disabled = false)); toast(String(e), true); }
    }, how === "this" ? "btn-main" : "btn-quiet");
    if (needsOther && !c.other.here) { b.disabled = true; b.title = "Not on this computer yet"; }
    return b;
  };
  block.append(actions(keep("Keep both", "both", true), keep("Keep the other", "other", true), keep("Keep this one", "this", false)));
  return block;
}
