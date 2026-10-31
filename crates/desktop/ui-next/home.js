// Home, as agreed component by component (docs/design/desktop.md): the
// chosen device's circle at the top right, the status mark in the middle
// with one line under it, Send, and Waiting only while something is queued.
// Nothing else.

const HOME = { devices: [], guests: [], summary: null, sends: [], sending: false, note: null, conflicts: 0, beats: 0 };

// ---------- The chosen device ----------

function chosenDevice() {
  let id = null;
  try { id = localStorage.getItem("qurb.chosen"); } catch (e) { /* this run only */ }
  return HOME.devices.find((d) => d.fingerprint === id)
    ?? HOME.devices.find((d) => d.kind === "phone" && d.route)
    ?? HOME.devices[0]
    ?? null;
}

function choose(device) {
  try { localStorage.setItem("qurb.chosen", device.fingerprint); } catch (e) { /* this run only */ }
  closePanel();
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
  if (HOME.mark?.go) { go(HOME.mark.go); return; }
  invoke("sync_now").catch(() => {});
  const mark = $("home-mark");
  mark.classList.add("spinning");
  $("ring-arc").style.strokeDasharray = "33 67";
  $("ring-arc").style.opacity = 1;
  setTimeout(liveHome, 1500);
}

// ---------- The circle, and its panel ----------

function drawCircle() {
  const box = $("home-circle");
  if (!box) return;
  const d = chosenDevice();
  const b = d
    ? h("button", { class: "circle-btn", onclick: togglePanel, "aria-label": `${d.name}, and your other devices` }, circle(d, "circle-lg"))
    : h("button", { class: "circle-btn", onclick: togglePanel, "aria-label": "Add a device" }, h("span", { class: "circle circle-lg circle-add" }, icon("plus")));
  put(box, b);
}

function togglePanel() {
  if ($("panel")) { closePanel(); return; }
  const chosen = chosenDevice();
  const row = (d, own) => h("div", { class: "p-row" },
    h("button", { class: "p-main", onclick: () => (own ? choose(d) : go("device", d)) },
      circle(d, "circle-md"),
      h("span", { class: "p-name" }, d.name),
      own && chosen?.fingerprint === d.fingerprint ? icon("check", "p-tick") : null),
    h("button", { class: "icon-btn", "aria-label": `${d.name}'s page`, onclick: () => go("device", d) }, icon("chevron-right")));
  const panel = h("div", { class: "panel", id: "panel", role: "dialog" },
    ...HOME.devices.map((d) => row(d, true)),
    HOME.guests.length ? h("p", { class: "p-label" }, "People who use this computer") : null,
    ...HOME.guests.map((d) => row(d, false)),
    h("button", { class: "p-add", onclick: () => go("add") },
      h("span", { class: "circle circle-md circle-add" }, icon("plus")), "Add a device"));
  $("home-circle").append(panel);
  setTimeout(() => document.addEventListener("mousedown", away, true));
}

function away(e) {
  if (!e.target.closest("#panel") && !e.target.closest(".circle-btn")) closePanel();
}

function closePanel() {
  $("panel")?.remove();
  document.removeEventListener("mousedown", away, true);
}

// ---------- Waiting ----------

function drawWaiting() {
  const box = $("home-waiting");
  if (!box) return;
  const s = HOME.summary;
  const moving = [...(s?.incoming ?? []), ...(s?.outgoing ?? [])].map((t) => ({
    name: t.path.split("/").pop(),
    progress: Math.min(1, Number(t.done) / Number(t.size || 1)),
  }));
  const ready = HOME.sends.map((o) => ({ name: o.path.split("/").pop(), to: o.to, cancel: () => cancel(o) }));
  const all = [...moving, ...ready];
  if (!all.length) { put(box); return; }
  const rows = all.slice(0, 3).map((w) => h("div", { class: "w-row" },
    h("span", { class: "w-tile" }, icon(w.to ? "send" : "arrow-down")),
    h("div", { class: "w-main" },
      h("b", {}, w.name),
      w.to ? h("small", {}, `Ready for ${w.to}`) : h("span", { class: "w-bar" }, h("i", { style: { width: `${Math.round(w.progress * 100)}%` } }))),
    w.cancel ? h("button", { class: "icon-btn", "aria-label": `Stop sending ${w.name}`, onclick: w.cancel }, icon("x")) : null));
  if (all.length > 3) rows.push(h("button", { class: "w-more", onclick: () => go("waiting") }, `${all.length - 3} more`));
  put(box, h("div", { class: "w-hd" }, "Waiting", h("span", { class: "badge" }, all.length)), ...rows);
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

/** Waiting in full: what moves now, each send not yet collected with × to
 * stop it, and what finished lately. */
async function drawWaitingAll(main) {
  main.append(h("div", { class: "pg" }, pageHead("Waiting", { words: "Home", go: () => go("home") }), h("div", { id: "waiting-all" })));
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
  const moving = [...(s?.incoming ?? []).map((t) => ({ ...t, verb: "From" })), ...(s?.outgoing ?? []).map((t) => ({ ...t, verb: "To" }))];
  const finished = done.filter((r) => r.kind === "collected" || r.kind === "cancelled" || (r.kind === "received" && (r.detail ?? "").startsWith("sent to this device"))).slice(0, 10);
  put(box, 
    moving.length ? label("Moving") : null,
    ...moving.map((t) => listRow(t.verb === "From" ? "arrow-down" : "arrow-up", base(t.path), `${t.verb} ${t.device}`, Math.min(1, Number(t.done) / Number(t.size || 1)))),
    sends.length ? label("Not collected yet") : null,
    ...sends.map((o) => row({ ic: fileIcon(o.path), name: base(o.path), sub: [`For ${o.to}`, size(o.size)],
      end: h("button", { class: "icon-btn", "aria-label": `Stop sending ${base(o.path)}`, onclick: async () => { await cancel(o); listsWaitingAll(); } }, icon("x")) })),
    !moving.length && !sends.length ? empty("circle-check", "Nothing waiting") : null,
    finished.length ? label("Done") : null,
    ...finished.map((r) => row({
      ic: tile(r.kind === "cancelled" ? "x" : "circle-check", r.kind === "cancelled" ? "" : "tile-ok"),
      name: base(r.path),
      sub: [r.kind === "collected" ? `To ${r.device}` : r.kind === "cancelled" ? "Stopped" : `From ${r.device ?? "another device"}`, ago(r.at)],
      onclick: r.kind === "received" ? () => invoke("show_received", { id: r.id }).catch((e) => toast(String(e), true)) : null,
    })));
}
