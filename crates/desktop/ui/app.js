// The window's behaviour.
//
// Every number and every row here comes from a command in `src/commands.rs`,
// which is a thin wrapper over the engine. Nothing in this file decides
// anything about syncing; if it looks like it is deciding something, that is a
// bug in the layering rather than a clever optimisation.
//
// Two rhythms, because two kinds of thing change at different rates:
//
//   - the daemon's live state (syncing, devices reachable) is polled often and
//     cheaply, because it changes many times a second and only the latest value
//     matters;
//   - lists are fetched when their screen is opened, and refreshed on a slower
//     beat, because they change rarely and are expensive to redraw under
//     somebody's cursor.

const invoke = window.__TAURI__.core.invoke;

const $ = (id) => document.getElementById(id);
const el = (tag, cls, text) => {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
};

/** Bytes as a person would say them. Input is a string: see commands.rs. */
function size(bytes) {
  let n = Number(bytes);
  if (!isFinite(n)) return "–";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let u = 0;
  while (n >= 1024 && u < units.length - 1) { n /= 1024; u++; }
  return `${u === 0 ? n : n.toFixed(n < 10 ? 1 : 0)} ${units[u]}`;
}

/** Unix seconds as a relative time. Matches the wording `qurb status` uses. */
function when(seconds) {
  if (seconds == null) return "";
  const ago = Math.max(0, Math.floor(Date.now() / 1000) - seconds);
  if (ago <= 90) return "just now";
  if (ago <= 5400) return plural(Math.floor(ago / 60), "minute");
  if (ago <= 172800) return plural(Math.floor(ago / 3600), "hour");
  return plural(Math.floor(ago / 86400), "day");
}

function plural(n, unit) {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/** Show a failure where the data would have been, rather than silently blank. */
function oops(list, error) {
  list.replaceChildren();
  const row = el("li");
  row.append(el("span", "name quiet", String(error)));
  list.append(row);
}

// ---------------------------------------------------------------- navigation

let screen = "home";

document.querySelectorAll("nav button").forEach((button) => {
  button.addEventListener("click", () => {
    document.querySelectorAll("nav button").forEach((b) => b.classList.toggle("on", b === button));
    screen = button.dataset.screen;
    document.querySelectorAll(".screen").forEach((s) => s.classList.toggle("on", s.id === screen));
    refreshScreen();
  });
});

// --------------------------------------------------------------------- home

async function drawHome() {
  try {
    const s = await invoke("summary");
    $("state").textContent = s.state;
    $("where").textContent = `${s.root} · this device is ${s.identity}`;

    const problem = $("problem");
    problem.textContent = s.problem ?? "";
    problem.classList.toggle("hidden", !s.problem);

    $("card-devices").textContent = s.peers === 0 ? "none paired" : `${s.peers_reachable} of ${s.peers}`;

    const recent = $("recent");
    recent.replaceChildren();
    if (s.recent.length === 0) {
      recent.append(el("li", "quiet", "nothing yet"));
    } else {
      for (const r of s.recent) {
        const row = el("li");
        row.append(el("span", "name", r.path));
        row.append(el("span", "when", `${r.from_peer ? "arrived" : "stored"} ${when(r.at)}`));
        recent.append(row);
      }
    }
  } catch (e) {
    $("state").textContent = "cannot read the daemon";
    $("where").textContent = String(e);
  }

  try {
    const st = await invoke("storage");
    $("card-files").textContent = st.file_count.toLocaleString();
    $("card-used").textContent = size(st.used);
    $("card-risk").textContent = st.only_here.toLocaleString();
    // Only worth a place on the screen when it is true. Zero files at risk is
    // the ordinary state and does not need a number pointing at it.
    $("card-risk-wrap").classList.toggle("hidden", st.only_here === 0);
    $("card-risk-wrap").classList.add("risk");
  } catch (e) {
    $("card-files").textContent = "–";
  }

  try {
    const out = await invoke("outgoing");
    const list = $("outgoing");
    list.replaceChildren();
    $("outgoing-heading").classList.toggle("hidden", out.length === 0);
    list.classList.toggle("hidden", out.length === 0);
    for (const o of out) {
      const row = el("li");
      row.append(el("span", "name", o.path));
      row.append(el("span", "size", size(o.size)));
      row.append(el("span", "when", `waiting for ${o.to}`));
      list.append(row);
    }
  } catch (e) {
    // An empty outgoing list is the ordinary case; a failure here is not worth
    // displacing the rest of the screen over.
  }
}

// -------------------------------------------------------------------- files

const PAGE = 100;
let shown = 0;
let searching = "";

function fileRow(f) {
  const row = el("li");
  row.append(el("span", "name", f.path));
  row.append(el("span", "size", size(f.size)));

  const tag = el("span", "tag", f.availability);
  if (f.availability === "here") tag.classList.add("here");
  if (f.availability === "only here") tag.classList.add("only");
  row.append(tag);

  if (f.availability === "not here") {
    const get = el("button", "act", "Fetch");
    get.addEventListener("click", async () => {
      get.disabled = true;
      get.textContent = "asked";
      try {
        await invoke("fetch", { path: f.path });
      } catch (e) {
        get.textContent = "failed";
      }
    });
    row.append(get);
  }
  return row;
}

async function drawFiles(append = false) {
  const list = $("file-list");
  if (!append) { shown = 0; list.replaceChildren(); }

  try {
    const rows = searching
      ? await invoke("search", { text: searching })
      : await invoke("files", { under: null, limit: PAGE, offset: shown });

    if (!searching) shown += rows.length;
    $("crumbs").textContent = searching ? `${rows.length} matching “${searching}”` : "";
    $("more").classList.toggle("hidden", searching !== "" || rows.length < PAGE);

    if (rows.length === 0 && !append) {
      list.append(el("li", "quiet", searching ? "nothing matching that name" : "this folder is empty"));
      return;
    }
    for (const f of rows) list.append(fileRow(f));
  } catch (e) {
    oops(list, e);
  }
}

$("more").addEventListener("click", () => drawFiles(true));

// Debounced, because every keystroke is a query and a list that redraws under
// the cursor while somebody is still typing is worse than one that waits.
let typing = null;
$("find").addEventListener("input", (event) => {
  clearTimeout(typing);
  typing = setTimeout(() => {
    searching = event.target.value.trim();
    drawFiles();
  }, 180);
});

// ------------------------------------------------------------------ devices

async function drawDevices() {
  const list = $("device-list");
  try {
    const devices = await invoke("devices");
    list.replaceChildren();
    if (devices.length === 0) {
      list.append(el("li", "quiet", "no paired devices yet"));
      return;
    }
    for (const d of devices) {
      const row = el("li");
      row.append(el("span", "name", d.name));
      row.append(el("span", "size", d.fingerprint));
      row.append(el("span", "when", d.last_seen ? `last reached ${when(d.last_seen)}` : "not reached yet"));
      list.append(row);
    }
  } catch (e) {
    oops(list, e);
  }
}

// ----------------------------------------------------------------- activity

let oldest = null;

async function drawActivity(append = false) {
  const list = $("activity-list");
  if (!append) { oldest = null; list.replaceChildren(); }

  try {
    const rows = await invoke("activity", { path: null, limit: 60, before: oldest });
    if (rows.length === 0 && !append) {
      list.append(el("li", "quiet", "nothing recorded yet"));
      $("older").classList.add("hidden");
      return;
    }
    oldest = rows.length ? rows[rows.length - 1].id : oldest;
    $("older").classList.toggle("hidden", rows.length < 60);

    for (const r of rows) {
      const row = el("li");
      row.append(el("span", "tag", r.kind));

      // Most rows are about a path. A pairing is about a device and has no
      // path at all, so the device becomes the subject rather than a note
      // beside one.
      const subject = r.path ?? r.device ?? "";
      row.append(el("span", "name", subject));
      if (r.device && r.device !== subject) row.append(el("span", "when", r.device));
      if (r.size) row.append(el("span", "size", size(r.size)));
      row.append(el("span", "when", when(r.at)));

      // `paired` stores the device name as its detail, which is already the
      // subject. Saying it twice reads as a mistake.
      if (r.detail && r.detail !== subject) row.append(el("span", "detail", r.detail));
      list.append(row);
    }
  } catch (e) {
    oops(list, e);
  }
}

$("older").addEventListener("click", () => drawActivity(true));

// ------------------------------------------------------------------ storage

const GIB = 1024 ** 3;
let disk = 0;

async function drawStorage() {
  try {
    const st = await invoke("storage");
    disk = Number(st.disk);

    const used = Number(st.used);
    const limit = Number(st.limit);
    const against = limit > 0 ? limit : disk;
    const share = against > 0 ? Math.min(100, (used / against) * 100) : 0;

    $("bar-fill").style.width = `${share}%`;
    $("bar-fill").classList.toggle("over", st.over);

    $("usage").textContent = limit > 0
      ? `${size(st.used)} of ${size(st.limit)}${st.over ? " — over the limit" : ""}`
      : `${size(st.used)} used, no limit set`;

    // Not touched while somebody is dragging: rewriting the control they are
    // holding is the single most irritating thing a live-updating screen does.
    if (document.activeElement !== $("limit")) {
      $("capped").checked = limit > 0;
      $("limit-controls").classList.toggle("hidden", limit === 0);
      $("limit").value = String(toTrack(limit > 0 ? limit / GIB : disk / GIB / 10));
      showLimitLabel();
    }

    const breakdown = $("breakdown");
    breakdown.replaceChildren();
    const rows = [
      ["Files in the folder", size(st.files)],
      ["Content with no file here", size(st.chunks)],
      ["Files whose contents were dropped", st.evicted.toLocaleString()],
      ["Files no other device has", st.only_here.toLocaleString()],
      ["This disk", size(st.disk)],
    ];
    for (const [term, value] of rows) {
      breakdown.append(el("dt", null, term));
      breakdown.append(el("dd", null, value));
    }
  } catch (e) {
    $("usage").textContent = String(e);
  }
}

// The track is square-law rather than linear.
//
// A 500 GB disk against an allowance somebody actually wants — ten or twenty
// gigabytes — puts the useful part of a linear slider in its first four
// percent, where it cannot be aimed at. Squaring gives the small end most of
// the track and leaves the large end coarse, which is the right way round:
// nobody needs 380 GiB rather than 390.
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
  $("limit-label").textContent = `${gib} GiB${share}`;
}

$("limit").addEventListener("input", showLimitLabel);

$("capped").addEventListener("change", async (event) => {
  $("limit-controls").classList.toggle("hidden", !event.target.checked);
  // Unchecking means no limit, and is worth applying at once: somebody who has
  // just turned a limit off is asking for the cap to stop, not asking to press
  // a second button.
  if (!event.target.checked) await save("0");
});

$("apply").addEventListener("click", () => save(String(chosenGib() * GIB)));

async function save(bytes) {
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

// -------------------------------------------------------------------- pulse

function refreshScreen() {
  if (screen === "home") drawHome();
  if (screen === "files") drawFiles();
  if (screen === "devices") drawDevices();
  if (screen === "activity") drawActivity();
  if (screen === "storage") drawStorage();
}

refreshScreen();

// The live state, often. A poll rather than a subscription because the value is
// one small struct and the window is in the same process as the daemon that
// publishes it: the cost of asking is a channel read.
setInterval(() => { if (screen === "home") drawHome(); }, 1500);

// Lists, rarely, and only the one being looked at. Redrawing a list somebody is
// reading is a cost, not a feature.
setInterval(() => {
  if (screen === "storage") drawStorage();
  if (screen === "devices") drawDevices();
}, 5000);
