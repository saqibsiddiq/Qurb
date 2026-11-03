// Home, as agreed component by component (docs/design/desktop.md) and
// reviewed against the phone's on 2026-11-03 (docs/design/
// desktop-review-2026-11-03.md): the chosen device's circle at the top
// right, the status mark in the middle with one line under it, Send, and
// Waiting only while something is queued. Nothing else.

const HOME = { devices: [], guests: [], summary: null, sends: [], sending: false, note: null, conflicts: 0, beats: 0 };

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
      h("div", { class: "home-top" }, h("span", { id: "home-circle" })),
      h("div", { class: "home-middle" },
        h("button", { class: "mark", id: "home-mark", onclick: markClicked, "aria-label": "Sync now" },
          ring(),
          h("span", { class: "mark-in", id: "mark-in" })),
        h("p", { class: "mark-words", id: "mark-words" }, " "),
        h("button", { class: "send", onclick: chooseAndSend }, icon("send"), "Send"),
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
    '<circle class="ring-track" cx="50" cy="50" r="46" pathLength="100"/>' +
    '<circle class="ring-arc" id="ring-arc" cx="50" cy="50" r="46" pathLength="100"/>';
  return svg;
}

/** What the mark shows: its symbol, its words, and its ring. */
function markFor(s) {
  const to = chosenDevice();
  if (!s) return { icon: "circle-alert", words: "QurbSpace can't read its state", tone: "warn" };
  if (s.peers === 0) return { icon: "plus", words: "Add a device", tone: "idle" };
  const moving = s.incoming[0] ?? s.outgoing[0];
  if (moving) {
    const done = Math.min(1, Number(moving.done) / Number(moving.size || 1));
    const verb = s.incoming[0] ? "Receiving" : "Sending";
    return { icon: s.incoming[0] ? "arrow-down" : "arrow-up", words: `${verb} ${moving.path.split("/").pop()}`, tone: "busy", progress: done };
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
  const arc = $("ring-arc");
  arc.style.strokeDasharray = m.spin ? "33 67" : m.progress != null ? `${Math.max(1, m.progress * 100)} 100` : "0 100";
  // No arc at all when nothing moves: an empty one with round ends is a dot.
  arc.style.opacity = m.spin || m.progress != null ? 1 : 0;
  drawWaiting();
}

async function listsHome() {
  if (!$("home-circle")) return;
  const [devices, sends] = await Promise.all([invoke("devices").catch(() => []), invoke("outgoing").catch(() => [])]);
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

/** A file's preview into `el` when it can be had here; its kind's icon until. */
function previewInto(el, path) {
  el.append(icon(fileIcon(path)));
  if (kindOf(path) !== "image") return;
  invoke("preview", { path, small: true }).then((p) => {
    if (p?.kind === "image") put(el, h("img", { src: p.data, alt: "" }));
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

function drawWaiting() {
  const box = $("home-waiting");
  if (!box) return;
  const groups = waitingGroups(HOME.summary, HOME.sends);
  if (!groups.length) { put(box); return; }
  const rows = groups.map((g) => h("button", { class: "w-row", onclick: () => go("waiting") },
    previewStack(g.files, g.moving && g.size ? g.done / g.size : null),
    h("span", { class: "w-main" },
      waitingWho(g),
      h("span", { class: "w-chips" },
        h("span", { class: "chip" }, Number(g.files.length).toLocaleString()),
        g.size ? h("span", { class: "chip" }, size(g.size)) : null,
        g.waitingOn ? h("span", { class: "chip chip-wait" }, icon(g.device?.kind === "phone" ? "smartphone" : "laptop"), g.waitingOn) : null)),
    icon("chevron-right", "row-go")));
  put(box, h("div", { class: "w-hd" }, "Waiting", h("span", { class: "badge" }, groups.length)), h("div", { class: "w-card" }, ...rows));
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
  const [s, sends, done] = await Promise.all([
    invoke("summary").catch(() => null),
    invoke("outgoing").catch(() => []),
    invoke("activity", { path: null, limit: 200, before: null }).catch(() => []),
  ]);
  const groups = waitingGroups(s, sends);
  const finished = done.filter((r) => r.kind === "collected" || (r.kind === "received" && (r.detail ?? "").startsWith("sent to this device"))).slice(0, 12);
  const section = (g) => {
    const grid = h("div", { class: "a-grid w-grid" });
    g.files.slice(0, 23).forEach((path, i) => {
      const cell = h("span", { class: "w-pic w-pic-row", title: base(path) });
      previewInto(cell, path);
      if (i === 0 && g.moving && g.size) cell.append(ringOver(g.done / g.size));
      const send = g.sends.find((o) => o.path === path);
      if (send) cell.append(h("button", { class: "w-x", "aria-label": `Stop sending ${base(path)}`, onclick: async () => { await cancel(send); listsWaitingAll(); } }, icon("x")));
      grid.append(cell);
    });
    if (g.files.length > 23) grid.append(h("span", { class: "w-pic w-pic-row a-more" }, `+${(g.files.length - 23).toLocaleString()}`));
    return [h("div", { class: "w-sect" }, waitingWho(g),
      g.size ? h("span", { class: "chip" }, size(g.size)) : null,
      g.waitingOn ? h("span", { class: "chip chip-wait" }, g.waitingOn) : null), grid];
  };
  put(box,
    ...(groups.length ? groups.flatMap(section) : [empty("circle-check", "Nothing waiting")]),
    finished.length ? label("Done") : null,
    finished.length ? h("div", { class: "a-grid w-grid" }, ...finished.map((r) => { const c = h("span", { class: "w-pic w-pic-row", title: base(r.path ?? "") }); previewInto(c, r.path ?? ""); return c; })) : null);
  icons(box);
}
