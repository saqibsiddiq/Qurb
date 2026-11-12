// Home, as agreed component by component (docs/design/desktop.md) and
// reviewed against the phone's on 2026-11-03 (docs/design/
// desktop-review-2026-11-03.md): the chosen device's circle at the top
// right, the status mark in the middle with one line under it, Send, and
// Waiting only while something is queued. Nothing else.

const HOME = { devices: [], guests: [], removed: [], summary: null, sends: [], sending: false, note: null, conflicts: 0, beats: 0 };

// ---------- The chosen device ----------

function chosenDevice() {
  let id = null;
  try { id = localStorage.getItem("qurb.chosen"); } catch (e) { /* this run only */ }
  return [...HOME.devices, ...HOME.guests].find((d) => d.fingerprint === id)
    ?? HOME.devices.find((d) => d.kind === "phone" && d.route)
    ?? HOME.devices[0]
    ?? null;
}

function choose(device) {
  try { localStorage.setItem("qurb.chosen", device.fingerprint); } catch (e) { /* this run only */ }
  drawCircle();
  liveHome();
}

// ---------- Home ----------

function drawHome(main) {
  main.append(
    h("div", { class: "home" },
      // The app's name and the chosen device's circle, as the phone's Home
      // has them at its top.
      h("div", { class: "home-top" }, h("p", { class: "home-name" }, "QurbSpace"), h("span", { id: "home-circle" })),
      h("div", { class: "home-middle" },
        h("button", { class: "mark", id: "home-mark", onclick: markClicked, "aria-label": "Sync now" },
          ring(),
          h("span", { class: "mark-in", id: "mark-in" })),
        h("p", { class: "mark-words", id: "mark-words" }, " "),
        h("p", { class: "mark-quiet", id: "mark-quiet", hidden: true }),
        h("button", { class: "send", id: "home-send", onclick: () => (HOME.mark?.again ? go("add") : chooseAndSend()) }, icon("send"), "Send"),
        h("p", { class: "drop-hint" }, icon("download"), "Drop files here to send"),
        h("div", { class: "waiting", id: "home-waiting" }))));
  icons(main);
  drawCircle();
  listsHome().then(liveHome);
}

/** The ring the mark is drawn on: a quiet track, and an arc that shows
 * syncing (a third, travelling) or progress (as much as is done). */
function ring() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("class", "ring");
  svg.innerHTML =
    '<circle class="ring-track" cx="50" cy="50" r="47.6" pathLength="100"/>' +
    '<circle class="ring-arc" id="ring-arc" cx="50" cy="50" r="47.6" pathLength="100"/>';
  return svg;
}

/** What the mark shows: its symbol, its words, and its ring. */
function markFor(s) {
  const to = chosenDevice();
  if (!s) return { icon: "circle-alert", words: "QurbSpace can't read its state", tone: "warn" };
  if (s.peers === 0) {
    // The only device gone (D22; decision 0084): what was kept, and the one
    // thing to do.
    const gone = HOME.removed[0];
    if (gone?.by_them) return { icon: "lock-keyhole", words: `${gone.name} removed this computer`, quiet: "Add it again to sync", tone: "idle", again: true };
    if (gone) return { icon: "lock-keyhole", words: `${gone.name} was removed`, quiet: Number(gone.size) > 0 ? "Its files stay here, locked, until it's added again" : null, tone: "idle", again: true };
    return { icon: "plus", words: "Add a device", tone: "idle" };
  }
  // Paused by the person (decision 0086): said first, and a click opens
  // the sheet that resumes it.
  if (s.paused_until != null) return { icon: "pause", words: "Paused", quiet: Number(s.paused_until) > 4e9 ? null : pausedWords(s.paused_until), tone: "away", resume: true };
  const moving = s.incoming[0] ?? s.outgoing[0];
  if (moving) {
    const done = Math.min(1, Number(moving.done) / Number(moving.size || 1));
    const verb = s.incoming[0] ? "Receiving" : "Sending";
    return { icon: s.incoming[0] ? "arrow-down" : "arrow-up", words: verb, tone: "busy", progress: done };
  }
  if (HOME.note && Date.now() < HOME.note.until) return { icon: "arrow-up", words: HOME.note.words, tone: "busy", progress: null, spin: true };
  if (s.problem || s.state === "needs attention") return { icon: "triangle-alert", words: "Something needs attention", tone: "warn", go: "activity" };
  // Two versions of a file wait for a choice: the mark says so, and a click
  // goes to them rather than syncing.
  if (HOME.conflicts) return { icon: "git-compare", words: HOME.conflicts === 1 ? "A file has two versions" : `${HOME.conflicts} files have two versions`, tone: "warn", go: "conflicts" };
  if (s.state === "syncing") return { icon: "refresh-cw", words: "Syncing", tone: "busy", spin: true };
  if (to && !to.route) return { icon: "cloud-off", words: `${to.name} is off`, tone: "away" };
  if (s.peers_reachable === 0) return { icon: "cloud-off", words: "Your devices are away", tone: "away" };
  return { icon: "check", words: "Everything is synced", tone: "ok" };
}

async function liveHome() {
  if (!$("home-mark")) return;
  try { HOME.summary = await invoke("summary"); } catch (e) { HOME.summary = null; }
  const m = markFor(HOME.summary);
  HOME.mark = m;
  const mark = $("home-mark");
  mark.dataset.tone = m.tone;
  mark.classList.toggle("spinning", Boolean(m.spin));
  put($("mark-in"), icon(m.icon));
  $("mark-words").textContent = m.words;
  $("mark-quiet").hidden = !m.quiet;
  $("mark-quiet").textContent = m.quiet ?? "";
  const sendBtn = $("home-send");
  if (sendBtn.dataset.again !== String(Boolean(m.again))) {
    sendBtn.dataset.again = String(Boolean(m.again));
    put(sendBtn, icon(m.again ? "smartphone" : "send"), m.again ? "Add it again" : "Send");
  }
  const arc = $("ring-arc");
  arc.style.strokeDasharray = m.spin ? "33 67" : m.progress != null ? `${Math.max(1, m.progress * 100)} 100` : "0 100";
  // No arc at all when nothing moves: an empty one with round ends is a dot.
  arc.style.opacity = m.spin || m.progress != null ? 1 : 0;
  drawWaiting();
}

async function listsHome() {
  if (!$("home-circle")) return;
  const [devices, sends, removed] = await Promise.all([invoke("devices").catch(() => []), invoke("outgoing").catch(() => []), invoke("removed_devices").catch(() => [])]);
  HOME.removed = removed;
  // One's own devices, each once: a phone set up again is the one it became.
  HOME.devices = devices.filter((d) => d.relation === "own" && !d.replaced_by);
  HOME.guests = devices.filter((d) => d.relation === "guest");
  HOME.sends = sends;
  // Finding two versions reads every path: every fourth beat, half a minute.
  if (HOME.beats++ % 4 === 0) HOME.conflicts = (await invoke("conflicts").catch(() => [])).length;
  drawCircle();
  drawWaiting();
}

function markClicked() {
  if (HOME.summary?.peers === 0) { go("add"); return; }
  if (HOME.mark?.go === "conflicts") { conflictsSheet(); return; }
  if (HOME.mark?.resume) { pauseSheet(HOME.summary.paused_until); return; }
  if (HOME.mark?.go) { go(HOME.mark.go); return; }
  invoke("sync_now").catch(() => {});
  const mark = $("home-mark");
  mark.classList.add("spinning");
  $("ring-arc").style.strokeDasharray = "33 67";
  $("ring-arc").style.opacity = 1;
  setTimeout(liveHome, 1500);
}

// ---------- The circle ----------

// As the phone's (the owner's review of 2026-11-03, H-2): a click opens the
// chosen device's page, and choosing another is *Switch device* there, or a
// right-click on the circle, the desktop's long press. The dropdown that did
// both at once, a click choosing and the › beside it opening, is gone.

function drawCircle() {
  const box = $("home-circle");
  if (!box) return;
  const d = chosenDevice();
  const b = d
    ? h("button", { class: "circle-btn", onclick: () => go("device", d), oncontextmenu: (e) => { e.preventDefault(); switchDevice(); }, "aria-label": `${d.name}'s page` }, circle(d, "circle-lg"))
    : h("button", { class: "circle-btn", onclick: () => go("add"), "aria-label": "Add a device" }, h("span", { class: "circle circle-lg circle-add" }, icon("plus")));
  put(box, b);
}

/** The phone's chooser, as a small window: a card for each device, the
 * chosen one ticked, one that is off with how long since it was seen, the
 * people who use this computer under their own label, and Add a device. */
function switchDevice(then) {
  const chosen = chosenDevice();
  const box = sheet({ title: "Switch device" });
  const card = (d) => h("button", {
    class: `dev-card${chosen?.fingerprint === d.fingerprint ? " on" : ""}`,
    onclick: () => { box.close(); choose(d); then?.(d); },
  }, circle(d, "circle-md"), h("b", {}, d.name),
  chosen?.fingerprint === d.fingerprint ? icon("check", "dev-tick")
    : !d.route && d.last_seen ? h("small", {}, ago(d.last_seen)) : null);
  box.append(
    ...HOME.devices.map(card),
    HOME.guests.length ? label("People") : null,
    ...HOME.guests.map(card),
    h("button", { class: "dev-card dev-add", onclick: () => { box.close(); go("add"); } },
      h("span", { class: "circle circle-md circle-add" }, icon("plus")), h("b", {}, "Add a device")));
  icons(box);
}

/** Kept for places that still close it on the way out: there is no panel. */
function closePanel() {}

// ---------- Waiting ----------

// One row for each device files are going to or coming from, as on the
// phone (the owner's review of 2026-11-03, H-1): which way, the device's
// circle and name, a stack of the files' previews with a ring over the one
// moving, a number, a size, and a chip only when it waits on something --
// "Open the app" for a phone, which takes what it was sent only while
// QurbSpace is open on it (decision 0082), "Off" for a computer.

/** Everything waiting, grouped by device and direction. */
function waitingGroups(s, sends) {
  const groups = new Map();
  const all = [...HOME.devices, ...HOME.guests];
  const at = (coming, name, id) => {
    const key = `${coming ? "in" : "out"}:${id ?? name}`;
    if (!groups.has(key)) {
      const device = all.find((d) => d.id === id || d.name === name) ?? null;
      groups.set(key, { coming, name, device, files: [], size: 0, done: 0, moving: false, sends: [] });
    }
    return groups.get(key);
  };
  for (const t of s?.incoming ?? []) {
    const g = at(true, t.device);
    g.files.push(t.path); g.size += Number(t.size); g.done += Number(t.done); g.moving = true;
  }
  for (const t of s?.outgoing ?? []) {
    const g = at(false, t.device);
    g.files.push(t.path); g.size += Number(t.size); g.done += Number(t.done); g.moving = true;
  }
  for (const o of sends) {
    const g = at(false, o.to, o.to_id);
    if (!g.files.includes(o.path)) { g.files.push(o.path); g.size += Number(o.size); }
    g.sends.push(o);
  }
  for (const g of groups.values()) {
    const d = g.device;
    g.waitingOn = g.moving || !d || d.route ? null : d.kind === "phone" ? "Open the app" : "Off";
  }
  return [...groups.values()];
}

/** The colour of a document's tag, by what it is, as the mockups draw it. */
const DOC_TAG = { pdf: "#c4412f", doc: "#2b5fb8", docx: "#2b5fb8", odt: "#2b5fb8", xls: "#1f7a4a", xlsx: "#1f7a4a", ods: "#1f7a4a", csv: "#1f7a4a", ppt: "#c2661f", pptx: "#c2661f" };

/** A document as the mockups draw one: a page of lines, its type on a tag. */
function docFace(path) {
  const ext = path.includes(".") ? path.split(".").pop().toLowerCase() : "";
  return h("span", { class: "doc-face" }, h("i"), h("i"), h("i"), h("i", { style: { width: "55%" } }),
    ext && ext.length <= 4 ? h("em", { style: { background: DOC_TAG[ext] ?? "#6b7280" } }, ext.toUpperCase()) : null);
}

/** A file's preview into `el` when it can be had here: a picture's own, a
 * document's page, its kind's icon otherwise. Put first, so that what is
 * laid over it -- a ring, a tick -- stays over it. */
function previewInto(el, path) {
  const kind = kindOf(path);
  const face = kind === "text" || kind === "code" ? docFace(path) : icon(fileIcon(path));
  el.prepend(face);
  if (kind !== "image") return;
  invoke("preview", { path, small: true }).then((p) => {
    if (p?.kind === "image" && face.isConnected) face.replaceWith(h("img", { src: p.data, alt: "" }));
  }).catch(() => {});
}

/** Up to three previews, fanned, a ring over the first while it moves. */
function previewStack(files, done) {
  const box = h("span", { class: "w-stack" });
  files.slice(0, 3).forEach((path, i) => {
    const cell = h("span", { class: "w-pic", style: { left: `${i * 8}px`, transform: `rotate(${i * 6}deg)`, zIndex: 3 - i } });
    previewInto(cell, path);
    if (i === 0 && done != null && done < 1) cell.append(ringOver(done));
    box.append(cell);
  });
  return box;
}

function ringOver(done) {
  const r = 9, c = 2 * Math.PI * r;
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "w-ring");
  svg.innerHTML = `<circle cx="12" cy="12" r="${r}" class="w-ring-track"/><circle cx="12" cy="12" r="${r}" class="w-ring-arc" stroke-dasharray="${Math.max(0.03, done) * c} ${c}"/>`;
  return svg;
}

/** ↑ or ↓, the device's circle, its name. */
function waitingWho(g) {
  return h("span", { class: "w-who" }, icon(g.coming ? "arrow-down" : "arrow-up", "w-dir"),
    g.device ? circle(g.device, "circle-xs") : null, h("b", {}, g.name));
}

/** One row of Waiting as option A (the owner, 2026-11-03; D2): one picture
 * with the other device's face on its corner, a title whose verb says the
 * way, one quiet line -- amber only when it waits on you -- and how far it
 * has got. */
function waitingRow(g, onclick) {
  const n = g.files.length;
  const what = `${n.toLocaleString()} ${n === 1 ? "file" : "files"}`;
  const done = g.moving && g.size ? Math.min(1, g.done / g.size) : null;
  const th = h("span", { class: "wa-th" });
  previewInto(th, g.files[0] ?? "");
  if (kindOf(g.files[0] ?? "") === "video") th.append(h("span", { class: "wa-play" }));
  if (done != null && done < 1) th.append(ringOver(done));
  th.append(g.device ? circle(g.device, "wa-face") : h("span", { class: "circle wa-face" }, (g.name || "?")[0].toUpperCase()));
  const off = g.device?.kind !== "phone";
  const line = g.moving
    ? h("span", {}, `${g.coming ? "From" : "To"} ${g.name}, ${size(g.size)}`)
    : g.waitingOn
      ? off ? h("span", {}, `To ${g.name}, when it's on`) : h("span", { class: "wait" }, `Open QurbSpace on ${g.name}`)
      : h("span", {}, `${g.coming ? "From" : "To"} ${g.name}${g.size ? `, ${size(g.size)}` : ""}`);
  const end = done != null
    ? h("span", { class: "wa-end" }, `${Math.round(done * 100)}%`)
    : g.waitingOn ? icon(off ? "clock" : "smartphone", off ? "wa-end-ic" : "wa-end-ic wait") : null;
  return h("button", { class: "wa", onclick },
    th,
    h("span", { class: "wa-t" },
      h("b", {}, `${g.coming ? "Receiving" : "Sending"} ${what}`),
      line,
      done != null ? h("span", { class: "wa-bar" }, h("i", { style: { width: `${Math.round(done * 100)}%` } })) : null),
    end);
}

function drawWaiting() {
  const box = $("home-waiting");
  if (!box) return;
  const groups = waitingGroups(HOME.summary, HOME.sends);
  // The mark moves up and shrinks while a queue shows (D2).
  $("home-mark")?.closest(".home")?.classList.toggle("queued", groups.length > 0);
  if (!groups.length) { put(box); return; }
  put(box,
    h("div", { class: "wl" }, h("b", {}, "Waiting"), h("button", { class: "wl-all", onclick: () => go("waiting") }, "See all")),
    h("div", { class: "wa-card" }, ...groups.map((g) => waitingRow(g, () => go("waiting")))));
}

async function cancel(o) {
  try { await invoke("cancel_send", { path: o.path, to: o.to_id }); } catch (e) { toast(String(e), true); }
  listsHome();
}

// ---------- Send ----------

async function send(paths, leaveOut = null) {
  const to = chosenDevice();
  if (!paths?.length || HOME.sending) return;
  if (!to) { toast("Add a device to send to"); return; }
  // Sent there before (decision 0059): asked, rather than a second copy sent
  // unasked or the file dropped unsaid.
  if (leaveOut === null) {
    const earlier = await invoke("sent_before", { paths, to: to.fingerprint }).catch(() => []);
    if (earlier.length) { askAgain(paths, earlier, to); return; }
  }
  HOME.sending = true;
  try {
    const r = await invoke("send_files", { paths, to: to.fingerprint, leaveOut });
    if (r.skipped?.length) toast(`Not sent: ${r.skipped.map((x) => base(x.path)).join(", ")}`, true);
    HOME.note = { words: r.only ? `Sending ${r.only}` : `Sending ${r.sent} files`, until: Date.now() + 2500 };
    if (place === "home") { liveHome(); listsHome(); }
  } catch (e) {
    toast(String(e), true);
  } finally {
    HOME.sending = false;
  }
}

async function chooseAndSend() {
  const dialog = window.__TAURI__?.dialog;
  if (!dialog) { toast("Drag files onto the window to send them"); return; }
  const chosen = await dialog.open({ multiple: true });
  if (chosen) send(Array.isArray(chosen) ? chosen : [chosen]);
}

function askAgain(paths, earlier, to) {
  const one = earlier.length === 1;
  const all = earlier.length === paths.length;
  const box = sheet({ ic: "send", title: one ? `Sent to ${to.name} before` : `${earlier.length} sent to ${to.name} before` });
  box.append(h("div", { class: "rows" }, ...earlier.map((e) => row({ ic: fileIcon(e.path), name: base(e.path), sub: ago(e.at) }))),
    actions(
      btn(all ? "Don't send" : "Leave out", () => { box.close(); if (!all) send(paths, earlier.map((e) => e.path)); }, "btn-quiet"),
      btn("Send again", () => { box.close(); send(paths, []); }, "btn-main")));
}

// ---------- Everything waiting ----------

/** Waiting in full, one section for each of Home's rows (H-1): its files'
 * previews in a grid, the moving one ringed, a send's with × to stop it;
 * then what finished lately. */
async function drawWaitingAll(main) {
  main.append(h("div", { class: "pg pg-narrow" }, pageHead("Waiting", { words: "Home", go: () => go("home") }), h("div", { id: "waiting-all" })));
  await listsWaitingAll();
}

async function listsWaitingAll() {
  const box = $("waiting-all");
  if (!box) return;
  const [s, sends] = await Promise.all([invoke("summary").catch(() => null), invoke("outgoing").catch(() => [])]);
  const groups = waitingGroups(s, sends);
  // Each of Home's rows as a section (D10): its title and quiet line as on
  // Home, then its files' previews, the moving one ringed, a send's with ×.
  const section = (g) => {
    const row = waitingRow(g, null);
    const words = row.querySelector(".wa-t");
    words.querySelector(".wa-bar")?.remove();
    const grid = h("div", { class: "wgrid" });
    const room = 6;
    g.files.slice(0, g.files.length > room + 1 ? room : room + 1).forEach((path, i) => {
      const cell = h("span", { class: "wa-th wa-cell", title: base(path) });
      previewInto(cell, path);
      if (kindOf(path) === "video") cell.append(h("span", { class: "wa-play" }));
      if (i === 0 && g.moving && g.size) cell.append(ringOver(g.done / g.size));
      const send = g.sends.find((o) => o.path === path);
      if (send) cell.append(h("button", { class: "w-x", "aria-label": `Stop sending ${base(path)}`, onclick: async () => { await cancel(send); listsWaitingAll(); } }, icon("x")));
      grid.append(cell);
    });
    if (g.files.length > room + 1) grid.append(h("span", { class: "wa-th wa-cell wa-more" }, `+${(g.files.length - room).toLocaleString()}`));
    return [h("div", { class: "wsect" }, ...words.children), grid];
  };
  put(box, ...(groups.length ? groups.flatMap(section) : [empty("circle-check", "Nothing waiting")]));
  icons(box);
}
