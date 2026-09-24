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

// ------------------------------------------------------------- which of the two

// The window opens on one of two things: setting a device up, or showing one.
// Asked before anything is drawn, because the answer decides which.
async function decide() {
  let where;
  try {
    where = await invoke("situation");
  } catch (e) {
    // Nothing can be shown and nothing can be set up. Saying so beats an empty
    // window that looks like it is still loading.
    document.body.textContent = String(e);
    return;
  }

  const settingUp = !where.set_up;
  $("setup").classList.toggle("hidden", !settingUp);
  $("tabs").classList.toggle("hidden", settingUp);
  document.querySelector("main").classList.toggle("hidden", settingUp);

  if (settingUp) {
    $("folder-path").value = where.root;
    step("welcome");
    return;
  }

  if (!where.running) {
    // A folder with a key that could not be opened. The commands will refuse,
    // so say why once rather than showing five screens of the same failure.
    $("state").textContent = "cannot open this folder";
    $("where").textContent = where.problem ?? "";
    return;
  }

  refreshScreen();
}

// ------------------------------------------------------------------ setting up

/** Which onboarding step is showing. */
function step(name) {
  document.querySelectorAll("#setup .step").forEach((s) => {
    s.classList.toggle("on", s.dataset.step === name);
  });
}

/** The path chosen, and whether the next button may be pressed. */
let joining = false;

$("choose-new").addEventListener("click", () => { joining = false; step("folder"); lookAtFolder(); });
$("choose-join").addEventListener("click", () => { joining = true; step("folder"); lookAtFolder(); });

document.querySelectorAll("#setup [data-back]").forEach((b) => {
  b.addEventListener("click", () => step(b.dataset.back));
});

let looking = null;
$("folder-path").addEventListener("input", () => {
  clearTimeout(looking);
  looking = setTimeout(lookAtFolder, 200);
});

async function lookAtFolder() {
  const next = $("folder-next");
  const says = $("folder-says");
  const path = $("folder-path").value.trim();
  if (!path) {
    says.textContent = "";
    next.disabled = true;
    return;
  }

  let folder;
  try {
    folder = await invoke("inspect_folder", { path });
  } catch (e) {
    says.textContent = String(e);
    next.disabled = true;
    return;
  }

  // Each of these is a reason not to continue, and each says what to do about
  // it. "Invalid" on its own is the least useful thing an interface can say.
  if (folder.set_up && !joining) {
    says.textContent = "there is already a device here — choose somewhere else, or open it instead";
    next.disabled = true;
    return;
  }
  if (folder.set_up && joining) {
    says.textContent = "there is already a device here, with its own key";
    next.disabled = true;
    return;
  }
  if (!folder.writable) {
    says.textContent = "this cannot be written to";
    next.disabled = true;
    return;
  }

  const disk = folder.disk === "0" ? "" : ` · ${size(folder.free)} free of ${size(folder.disk)}`;
  if (!folder.exists) {
    says.textContent = `will be created${disk}`;
  } else if (folder.existing_files === 0) {
    says.textContent = `empty${disk}`;
  } else {
    // Said plainly: everything already in there is about to appear on every
    // other device, which is a surprise worth not having.
    const count = folder.counted_all ? `${folder.existing_files}` : `over ${folder.existing_files}`;
    says.textContent = `${count} things already here — all of them will sync${disk}`;
  }
  next.disabled = false;
}

$("folder-next").addEventListener("click", async () => {
  const path = $("folder-path").value.trim();
  if (joining) { step("join"); return; }

  const next = $("folder-next");
  next.disabled = true;
  try {
    await invoke("create_device", { path });
    await showPhrase();
    step("phrase");
  } catch (e) {
    $("folder-says").textContent = String(e);
  } finally {
    next.disabled = false;
  }
});

async function showPhrase() {
  const words = await invoke("shown_phrase");
  const list = $("words");
  list.replaceChildren();
  for (const word of words) list.append(el("li", null, word));
  // Nothing keeps a copy: the list in the document is the only one here, and
  // confirmation is checked against the copy the session holds.
}

$("phrase-next").addEventListener("click", () => {
  askForWords();
  step("verify");
});

/** Which three positions are being asked about this time. */
let asked = [];

function askForWords() {
  // Three, chosen at random each time, so that pressing "show me them again"
  // and coming back is not a way to learn the answer to the same question.
  const positions = new Set();
  while (positions.size < 3) positions.add(1 + Math.floor(Math.random() * 24));
  asked = [...positions].sort((a, b) => a - b);

  const box = $("asks");
  box.replaceChildren();
  for (const position of asked) {
    const field = el("label", "ask");
    field.append(el("span", null, `word ${position}`));
    const input = el("input");
    input.type = "text";
    input.autocomplete = "off";
    input.spellcheck = false;
    input.dataset.position = String(position);
    field.append(input);
    box.append(field);
  }
  $("verify-says").classList.add("hidden");
  box.querySelector("input")?.focus();
}

$("verify-back").addEventListener("click", () => step("phrase"));

$("verify-next").addEventListener("click", async () => {
  const answers = [...$("asks").querySelectorAll("input")]
    .map((i) => [Number(i.dataset.position), i.value]);

  let ok;
  try {
    ok = await invoke("confirm_phrase", { answers });
  } catch (e) {
    $("verify-says").textContent = String(e);
    $("verify-says").classList.remove("hidden");
    return;
  }

  if (!ok) {
    $("verify-says").textContent =
      "that is not right — look at the paper again, and check the numbers";
    $("verify-says").classList.remove("hidden");
    return;
  }

  // Confirmed, so the words come off the screen. The session has already
  // dropped its copy; this drops the only other one.
  $("words").replaceChildren();
  $("asks").replaceChildren();
  $("ready-says").textContent = "This device is set up and watching your folder.";
  step("ready");
});

$("join-next").addEventListener("click", async () => {
  const says = $("join-says");
  const button = $("join-next");
  button.disabled = true;
  try {
    await invoke("enrol_device", {
      path: $("folder-path").value.trim(),
      phrase: $("given-phrase").value,
    });
    // Off the screen as soon as it has been used.
    $("given-phrase").value = "";
    says.classList.add("hidden");
    $("ready-says").textContent =
      "This device now shares a key with your others, and is watching your folder.";
    step("ready");
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  } finally {
    button.disabled = false;
  }
});

$("ready-next").addEventListener("click", () => decide());

// ---------------------------------------------------------------- navigation

let screen = "home";

function showScreen(name) {
  screen = name;
  document.querySelectorAll("nav button").forEach((b) =>
    b.classList.toggle("on", b.dataset.screen === name));
  document.querySelectorAll(".screen").forEach((s) => s.classList.toggle("on", s.id === name));
  refreshScreen();
}

document.querySelectorAll("nav button").forEach((button) => {
  button.addEventListener("click", () => showScreen(button.dataset.screen));
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

  await drawOutgoing();
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

// ---------------------------------------------------------------------- send

// The file waiting to be sent, as an absolute path. Held rather than read,
// because reading it is the Rust side's job and a window that loaded a 4 GB
// file into a JavaScript variable to hand it back would be a poor way to move
// it four inches.
let picked = null;

function showPicked() {
  $("chosen").textContent = picked ? picked.split("/").pop() : "";
  drawSendTo();
}

$("choose").addEventListener("click", async () => {
  // The plugin's own API, which `withGlobalTauri` exposes alongside the core
  // one. A native dialog rather than a page of our own: the file system is the
  // platform's, and every platform already has a good way to look at it.
  const dialog = window.__TAURI__?.dialog;
  if (!dialog) {
    $("send-says").textContent = "no file chooser available — drag a file in instead";
    return;
  }
  const chosen = await dialog.open({ multiple: false, directory: false });
  if (chosen) {
    picked = typeof chosen === "string" ? chosen : chosen.path;
    showPicked();
  }
});

// Dragging a file onto the window. Tauri reports these as window events rather
// than DOM ones, because the drag is happening to the *window* — the page never
// sees the file, and that is the point: a path crosses, not the contents.
const dropZone = $("drop");

if (window.__TAURI__?.event) {
  const { listen } = window.__TAURI__.event;

  listen("tauri://drag-over", () => {
    // Only meaningful on the send screen. Highlighting a zone nobody is looking
    // at is harmless; not highlighting one somebody is dragging onto is not.
    if (screen === "send") dropZone.classList.add("over");
  });

  listen("tauri://drag-leave", () => dropZone.classList.remove("over"));

  listen("tauri://drag-drop", (event) => {
    dropZone.classList.remove("over");
    const paths = event.payload?.paths ?? [];
    if (paths.length === 0) return;

    // One file. Sending several at once is a reasonable thing to want and a
    // different interaction — a queue, and something to say about partial
    // failure — so it is deliberately not pretended at here.
    picked = paths[0];
    if (paths.length > 1) {
      $("send-says").textContent = "one file at a time — taking the first";
    }
    showScreen("send");
    showPicked();
  });
}

async function drawSendTo() {
  const list = $("send-to");
  try {
    const devices = await invoke("devices");
    list.replaceChildren();

    if (devices.length === 0) {
      list.append(el("li", "quiet", "no paired devices yet — pair one first"));
      return;
    }

    for (const d of devices) {
      const row = el("li", "pickable");
      // Disabled until there is something to send, rather than hidden: the list
      // of devices is useful information on its own, and a row that appears
      // only after a file is chosen looks like it arrived from nowhere.
      row.setAttribute("aria-disabled", picked ? "false" : "true");

      const pick = el("button", "pick");
      pick.append(el("span", "name", d.name));
      pick.append(el("span", "when", d.last_seen ? `last reached ${when(d.last_seen)}` : "not reached yet"));
      pick.disabled = !picked;
      pick.addEventListener("click", () => sendTo(d));
      row.append(pick);
      list.append(row);
    }
  } catch (e) {
    oops(list, e);
  }
}

async function sendTo(device) {
  const says = $("send-says");
  if (!picked) return;

  says.textContent = `sending to ${device.name}…`;
  try {
    const name = await invoke("send_file", { path: picked, to: device.fingerprint });
    says.textContent =
      `${name} is waiting for ${device.name}. It will arrive the next time that device syncs.`;
    picked = null;
    showPicked();
    drawOutgoing();
  } catch (e) {
    says.textContent = String(e);
  }
}

/** What is still waiting to be collected, on the send screen and on home. */
async function drawOutgoing() {
  try {
    const out = await invoke("outgoing");
    for (const [heading, list] of [["sending-heading", "sending"], ["outgoing-heading", "outgoing"]]) {
      $(heading).classList.toggle("hidden", out.length === 0);
      $(list).classList.toggle("hidden", out.length === 0);
      $(list).replaceChildren();
      for (const o of out) {
        const row = el("li");
        row.append(el("span", "name", o.path));
        row.append(el("span", "size", size(o.size)));
        row.append(el("span", "when", `waiting for ${o.to}`));
        $(list).append(row);
      }
    }
  } catch (e) {
    // An empty list is the ordinary case; a failure here is not worth
    // displacing the rest of the screen over.
  }
}

// ------------------------------------------------------------------- pairing

// Which of the four panels under the device list is showing.
function pairPanel(which) {
  for (const name of ["idle", "showing", "entering", "done"]) {
    $(`pair-${name}`).classList.toggle("hidden", name !== which);
  }
}

let watching = null;

$("pair-show").addEventListener("click", async () => {
  pairPanel("showing");
  $("qr").replaceChildren();
  $("pair-code").textContent = "";
  $("pair-spoken").textContent = "";
  $("pair-says").textContent = "opening a port…";

  let invitation;
  try {
    invitation = await invoke("start_pairing");
  } catch (e) {
    $("pair-says").textContent = String(e);
    return;
  }

  // The SVG comes from our own renderer, not from anything a peer sent, and
  // the only variable in it is the code this device just made.
  //
  // Hidden entirely when there is none, rather than left as an empty white
  // panel: a blank where a code should be reads as a code that failed to load,
  // and somebody will sit waiting for it.
  $("qr").classList.toggle("hidden", !invitation.qr);
  if (invitation.qr) $("qr").innerHTML = invitation.qr;
  $("pair-code").textContent = `qurb join <dir> ${invitation.code}`;
  $("pair-spoken").textContent = invitation.spoken;

  clearInterval(watching);
  watching = setInterval(() => followPairing(invitation.expires_at), 700);
  followPairing(invitation.expires_at);
});

async function followPairing(expiresAt) {
  let state;
  try {
    state = await invoke("pairing_state");
  } catch (e) {
    $("pair-says").textContent = String(e);
    return;
  }

  if (state.state === "waiting") {
    // Counted down rather than left saying "waiting". A code that stopped
    // working five minutes ago, under a screen that still says it is waiting,
    // is worse than no screen: somebody reads it out and is told it is wrong.
    const left = Math.max(0, expiresAt - Math.floor(Date.now() / 1000));
    const minutes = Math.floor(left / 60);
    const seconds = String(left % 60).padStart(2, "0");
    $("pair-says").textContent = `waiting — this code expires in ${minutes}:${seconds}`;
    return;
  }

  clearInterval(watching);
  watching = null;

  if (state.state === "paired") {
    donePairing(state);
  } else if (state.state === "expired") {
    $("pair-says").textContent = "that code has expired — show a new one";
  } else if (state.state === "failed") {
    $("pair-says").textContent = state.message ?? "pairing failed";
  }
}

function donePairing(state) {
  $("pair-with").textContent = `${state.name} (${state.fingerprint})`;
  pairPanel("done");
  drawDevices();
}

$("pair-cancel").addEventListener("click", async () => {
  clearInterval(watching);
  watching = null;
  // Stopped at both ends: off the screen, and no longer answered. A cancelled
  // code that still worked would be the opposite of what was asked for.
  try { await invoke("stop_pairing"); } catch (e) { /* already gone */ }
  pairPanel("idle");
});

$("pair-enter").addEventListener("click", () => {
  $("pair-input").value = "";
  $("join-error").classList.add("hidden");
  pairPanel("entering");
  $("pair-input").focus();
});

$("pair-back").addEventListener("click", () => pairPanel("idle"));

$("pair-go").addEventListener("click", async () => {
  const button = $("pair-go");
  const error = $("join-error");
  button.disabled = true;
  button.textContent = "Joining…";
  try {
    const state = await invoke("join_device", { code: $("pair-input").value });
    $("pair-input").value = "";
    error.classList.add("hidden");
    donePairing(state);
  } catch (e) {
    error.textContent = String(e);
    error.classList.remove("hidden");
  } finally {
    button.disabled = false;
    button.textContent = "Join";
  }
});

$("pair-finish").addEventListener("click", () => pairPanel("idle"));

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
  if (screen === "settings") drawSettings();
  if (screen === "send") { drawSendTo(); drawOutgoing(); }
}

decide();

// ----------------------------------------------------------------- settings

async function drawSettings() {
  let s;
  try {
    s = await invoke("settings");
  } catch (e) {
    $("settings-says").textContent = String(e);
    return;
  }

  // Not rewritten under somebody who is in the middle of typing.
  const editing = document.activeElement?.closest?.(".field");
  if (!editing) {
    $("set-name").value = s.name;
    $("set-signal").value = s.signal;
    $("set-relay").value = s.relay ?? "";
    $("set-port").value = String(s.port);
  }

  const facts = $("facts");
  facts.replaceChildren();
  for (const [term, value] of [
    ["Folder", s.root],
    ["This device", s.identity || "—"],
    ["Key kept", s.protection],
  ]) {
    facts.append(el("dt", null, term));
    facts.append(el("dd", null, value));
  }
}

$("settings-save").addEventListener("click", async () => {
  const says = $("settings-says");
  try {
    await invoke("save_settings", {
      name: $("set-name").value,
      signal: $("set-signal").value,
      relay: $("set-relay").value,
      port: Number($("set-port").value) || 0,
    });
    says.textContent = "saved";
    setTimeout(() => { says.textContent = ""; }, 1600);
  } catch (e) {
    says.textContent = String(e);
  }
});

$("show-phrase").addEventListener("click", async () => {
  const list = $("revealed");
  const says = $("phrase-says");

  // A second press hides them again, so they are not left on a screen somebody
  // walks away from.
  if (!list.classList.contains("hidden")) {
    list.replaceChildren();
    list.classList.add("hidden");
    $("show-phrase").textContent = "Show the 24 words";
    return;
  }

  try {
    const words = await invoke("reveal_phrase");
    list.replaceChildren();
    for (const word of words) list.append(el("li", null, word));
    list.classList.remove("hidden");
    says.classList.add("hidden");
    $("show-phrase").textContent = "Hide them";
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  }
});

// The live state, often. A poll rather than a subscription because the value is
// one small struct and the window is in the same process as the daemon that
// publishes it: the cost of asking is a channel read.
setInterval(() => { if (screen === "home") drawHome(); }, 1500);

// Lists, rarely, and only the one being looked at. Redrawing a list somebody is
// reading is a cost, not a feature.
setInterval(() => {
  if (screen === "storage") drawStorage();
  // Not while a code is up: the list is at the top of the screen and redrawing
  // it is harmless, but `drawDevices` is also what a finished pairing calls,
  // and two of them racing would be a list drawn twice for no reason.
  if (screen === "devices" && !watching) drawDevices();
}, 5000);
