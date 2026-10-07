// What every screen uses: talking to the engine, formatting, and the handful of
// components the design is built from (docs/design/direction.md §48) -- rows,
// states, sheets, panels, menus, toasts, empty states.
//
// Every number and every row comes from a command in `src/commands.rs`, which
// is a thin wrapper over the engine. Nothing in these scripts decides anything
// about syncing; if it looks like it is deciding something, that is a bug in
// the layering rather than a clever optimisation.

// Every command goes through here, so that a failure is remembered even where
// the page shows it only briefly or not at all: the last fifty, as
// `window.qurbFailures`. scripts/desktop-smoke.sh reads it to fail on any
// command that did; from the web inspector, it says what a screen that went
// wrong without explanation was told.
const failures = (window.qurbFailures = []);
async function invoke(command, args) {
  try {
    return await window.__TAURI__.core.invoke(command, args);
  } catch (e) {
    failures.push(`${command}: ${e}`);
    if (failures.length > 50) failures.shift();
    throw e;
  }
}

const $ = (id) => document.getElementById(id);
const el = (tag, cls, text) => {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
};

/** An icon from the sprite icons.js puts in the page. */
function icon(name, cls) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", cls ? `i ${cls}` : "i");
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}

/** A button with an icon and words. Icons support text; they do not replace it (§10). */
function button(label, cls = "btn", iconName = null) {
  const b = el("button", cls);
  if (iconName) b.append(icon(iconName));
  b.append(el("span", null, label));
  return b;
}

/** Whether the person asked for less motion (§42). */
const calm = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ------------------------------------------------------------- formatting

/** Bytes as a person would say them. Input is a string: see commands.rs. */
function size(bytes) {
  let n = Number(bytes);
  if (!isFinite(n)) return "–";
  const units = ["B", "KB", "MB", "GB", "TB"];
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

function count(n, noun) {
  return `${n.toLocaleString()} ${noun}${n === 1 ? "" : "s"}`;
}

/** Seconds as a person would say a wait. */
function duration(seconds) {
  if (!isFinite(seconds) || seconds < 0) return "";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)} min`;
  return `${(seconds / 3600).toFixed(1)} h`;
}

const base = (path) => (path ?? "").split("/").pop();
const folderOf = (path) => (path ?? "").split("/").slice(0, -1).join("/");

// -------------------------------------------------------------- file states

const KINDS = {
  image: ["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "bmp", "svg", "tif", "tiff", "raw", "dng"],
  video: ["mp4", "mov", "mkv", "webm", "avi", "m4v"],
  audio: ["mp3", "m4a", "flac", "wav", "ogg", "opus", "aac"],
  archive: ["zip", "tar", "gz", "tgz", "xz", "bz2", "7z", "rar", "zst"],
  code: ["js", "ts", "rs", "py", "go", "c", "h", "cpp", "java", "kt", "swift", "sh", "json", "toml", "yaml", "yml", "html", "css"],
  text: ["txt", "md", "pdf", "doc", "docx", "odt", "rtf", "csv", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "epub"],
};

/** What kind of file a name is, for its icon and its details. */
function kindOf(path) {
  const ext = base(path).includes(".") ? base(path).split(".").pop().toLowerCase() : "";
  for (const [kind, exts] of Object.entries(KINDS)) if (exts.includes(ext)) return kind;
  return "file";
}

const KIND_ICON = {
  image: "file-image", video: "file-video", audio: "file-audio",
  archive: "file-archive", code: "file-code", text: "file-text", file: "file",
};
const KIND_WORD = {
  image: "Image", video: "Video", audio: "Audio", archive: "Archive",
  code: "Code", text: "Document", file: "File",
};

/**
 * The words and icon for where a file's bytes are (§11). Always both: a
 * state is never colour alone. "Only copy here" is the one worth a colour of
 * its own, because losing this device would lose the file.
 */
function stateOf(availability) {
  switch (availability) {
    case "here": return { cls: "here", icon: "hard-drive", words: "On this device" };
    case "elsewhere": return { cls: "elsewhere", icon: "cloud", words: "Available elsewhere" };
    case "only here": return { cls: "only", icon: "triangle-alert", words: "Only copy here" };
    // Decision 0055: listed, and no device this one syncs with has it.
    case "nowhere": return { cls: "nowhere", icon: "circle-alert", words: "On no device" };
    default: return { cls: "here", icon: "check", words: availability };
  }
}

function stateTag(availability) {
  const s = stateOf(availability);
  const tag = el("span", `state ${s.cls}`);
  tag.append(icon(s.icon), el("span", null, s.words));
  return tag;
}

// ------------------------------------------------------------------- rows

/**
 * A row: a tile with an icon, a name, a line under it, and actions at the
 * end that arrive on hover (§18, §38).
 */
function row({ iconName, name, sub = [], trail = [], onClick = null, cls = "" }) {
  const li = el("li", `row ${cls}`.trim());
  const tile = el("span", "tile");
  tile.append(icon(iconName));
  const main = el("div", "main");
  main.append(el("span", "name", name));
  const line = el("div", "sub");
  sub.filter(Boolean).forEach((part, i) => {
    const node = typeof part === "string" ? el("span", null, part) : part;
    if (i > 0) node.classList.add("dot");
    line.append(node);
  });
  if (line.childElementCount) main.append(line);
  const end = el("div", "trail");
  trail.filter(Boolean).forEach((t) => end.append(t));
  li.append(tile, main, end);
  if (onClick) {
    li.classList.add("clickable");
    li.tabIndex = 0;
    li.addEventListener("click", (event) => {
      if (event.target.closest("button")) return;
      onClick(event);
    });
    li.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && event.target === li) onClick(event);
    });
  }
  return li;
}

/** An outline icon and one line, with the action to take (brief §1). */
function empty(iconName, words, action = null, small = false) {
  const box = el("div", small ? "empty small" : "empty");
  const orb = el("span", "orb");
  orb.append(icon(iconName));
  box.append(orb, el("p", null, words));
  if (action) box.append(action);
  return box;
}

/** A failure where the data would have been, rather than silently blank. */
function oops(container, error) {
  container.replaceChildren(empty("circle-alert", String(error), null, true));
}

/** An attention item (§14): something needs a decision, and its one action. */
function attention({ iconName = "triangle-alert", title, says, action = null, error = false }) {
  const box = el("div", error ? "attention error" : "attention");
  const tile = el("span", "tile");
  tile.append(icon(iconName));
  const text = el("div");
  text.append(el("strong", null, title));
  if (says) text.append(el("p", null, says));
  box.append(tile, text);
  box.append(action ?? el("span"));
  return box;
}

// --------------------------------------------------------- sheets, panels

/**
 * An elevated sheet over a dimmed, slightly blurred window (§36). Returns the
 * sheet, and `close()` on it. Escape and a click outside close it too.
 */
function sheet({ wide = false, onClose = null } = {}) {
  const scrim = el("div", "scrim");
  const box = el("div", wide ? "sheet wide glass-elevated" : "sheet glass-elevated");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  const close = el("button", "icon-btn close");
  close.setAttribute("aria-label", "Close");
  close.append(icon("x"));
  box.append(close);
  scrim.append(box);
  document.body.append(scrim);

  let closed = false;
  box.close = () => {
    if (closed) return;
    closed = true;
    document.removeEventListener("keydown", onKey);
    scrim.classList.add("closing");
    setTimeout(() => scrim.remove(), calm() ? 0 : 280);
    onClose?.();
  };
  const onKey = (event) => { if (event.key === "Escape") box.close(); };
  document.addEventListener("keydown", onKey);
  close.addEventListener("click", () => box.close());
  scrim.addEventListener("mousedown", (event) => { if (event.target === scrim) box.close(); });
  return box;
}

/** The one panel open over the stage, if any: details of a file or device. */
let openPanel = null;

/** An elevated panel sliding in from the right of the stage (§19). */
function panel(cls = "") {
  closePanel();
  const box = el("aside", `panel glass-elevated ${cls}`.trim());
  const close = el("button", "icon-btn close");
  close.setAttribute("aria-label", "Close");
  close.style.cssText = "position:absolute;top:14px;right:14px;z-index:1";
  close.append(icon("x"));
  close.addEventListener("click", closePanel);
  box.append(close);
  $("stage").append(box);
  openPanel = box;
  return box;
}

function closePanel() {
  if (!openPanel) return;
  const box = openPanel;
  openPanel = null;
  box.classList.add("closing");
  setTimeout(() => box.remove(), calm() ? 0 : 280);
}

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !document.querySelector(".scrim")) closePanel();
});

/** A contextual menu under `anchor`: [label, icon, action, danger?] items. */
function menu(anchor, items) {
  document.querySelector(".menu")?.remove();
  const box = el("div", "menu glass-elevated");
  box.setAttribute("role", "menu");
  for (const item of items) {
    if (item === "-") { box.append(el("hr")); continue; }
    const [label, iconName, action, danger] = item;
    const b = el("button", danger ? "danger" : "");
    b.setAttribute("role", "menuitem");
    b.append(icon(iconName), el("span", null, label));
    b.addEventListener("click", () => { box.remove(); action(); });
    box.append(b);
  }
  document.body.append(box);
  const r = anchor.getBoundingClientRect();
  const width = box.offsetWidth;
  box.style.left = `${Math.min(window.innerWidth - width - 12, Math.max(12, r.right - width))}px`;
  const below = r.bottom + 6;
  box.style.top = below + box.offsetHeight > window.innerHeight - 12
    ? `${Math.max(12, r.top - box.offsetHeight - 6)}px` : `${below}px`;
  box.querySelector("button")?.focus();
  const away = (event) => {
    if (!box.contains(event.target)) { box.remove(); document.removeEventListener("mousedown", away, true); }
  };
  setTimeout(() => document.addEventListener("mousedown", away, true));
  box.addEventListener("keydown", (event) => { if (event.key === "Escape") box.remove(); });
}

/** A short confirmation at the foot of the window. */
function toast(words, warn = false) {
  document.querySelector(".toast")?.remove();
  const box = el("div", warn ? "toast warn glass-elevated" : "toast glass-elevated");
  box.setAttribute("role", "status");
  box.append(icon(warn ? "circle-alert" : "circle-check"), el("span", null, words));
  document.body.append(box);
  setTimeout(() => box.remove(), warn ? 6000 : 3200);
}

/**
 * A button that asks before it acts: the first press says what it will do,
 * the second, within a few seconds, does it. In the page rather than a
 * dialog, for actions that cannot be undone but are too small for a sheet.
 */
function twoPress(b, asking, act) {
  const said = b.querySelector("span")?.textContent ?? b.textContent;
  let armed = 0;
  b.addEventListener("click", async (event) => {
    event.stopPropagation();
    if (Date.now() > armed) {
      armed = Date.now() + 4000;
      (b.querySelector("span") ?? b).textContent = asking;
      setTimeout(() => {
        if (Date.now() > armed) (b.querySelector("span") ?? b).textContent = said;
      }, 4100);
      return;
    }
    armed = 0;
    b.disabled = true;
    try { await act(); } finally { b.disabled = false; (b.querySelector("span") ?? b).textContent = said; }
  });
  return b;
}

// ------------------------------------------------------------------ devices

/**
 * Which kind of device a name suggests, for its icon. A guess, from the name
 * the device chose: the index records who a device is, not what it is. A
 * wrong guess costs an icon, never a decision.
 */
function deviceIcon(name) {
  return /phone|galaxy|pixel|android|iphone|sm-[a-z]\d|oneplus|xiaomi|redmi/i.test(name ?? "")
    ? "smartphone" : "laptop";
}
