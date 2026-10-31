// What every place uses: talking to the engine, drawing, and saying sizes
// and times the way people say them. The engine is the one the window has
// today (src/commands.rs): the same commands, the same answers, so that this
// interface can replace the old one without the engine changing.

async function invoke(command, args) {
  return window.__TAURI__.core.invoke(command, args);
}

const $ = (id) => document.getElementById(id);

/** An element from a tag, attributes and children. */
function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null && c !== false) el.append(c.nodeType ? c : String(c));
  return el;
}

/** One of the Android app's icons (icons.js), sized by its CSS. */
function icon(name, cls = "") {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 256 256");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", `ic ${cls}`.trim());
  for (const d of ICONS[name] ?? []) {
    const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
    p.setAttribute("d", d);
    svg.append(p);
  }
  return svg;
}

/** Fill every <i data-icon> placeholder in `root`. */
function icons(root = document) {
  root.querySelectorAll("i[data-icon]").forEach((i) => i.replaceWith(icon(i.dataset.icon)));
}

/** Bytes, as a person reads them: 4.7 GB, 812 MB, 12 KB. */
function size(bytes) {
  const n = Number(bytes) || 0;
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
  return i === 0 ? `${n} bytes` : `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/** How long ago a unix time was: just now, 4 min ago, 3 h ago, 2 days ago. */
function ago(at) {
  if (!at) return "never";
  const s = Math.max(0, Math.floor(Date.now() / 1000) - at);
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  const d = Math.round(s / 86400);
  return d === 1 ? "yesterday" : `${d} days ago`;
}

const count = (n, one, many = `${one}s`) => `${Number(n).toLocaleString()} ${Number(n) === 1 ? one : many}`;

/** A device as the circle the Android app draws it: its first letter. */
function circle(device, cls = "") {
  const online = Boolean(device.route);
  return h("span", { class: `circle ${cls}`.trim(), title: device.name },
    device.picture ? h("img", { src: device.picture, alt: "" }) : (device.name || "?").trim()[0].toUpperCase(),
    h("i", { class: `circle-dot ${online ? "on" : "off"}`, "aria-label": online ? "online" : "offline" }));
}

const base = (path) => path.split("/").pop();
const folderOf = (path) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");

/** What a file is, from its name: enough to choose its icon and whether it
 * can be looked at here. */
const KINDS = {
  image: ["jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "avif", "svg"],
  video: ["mp4", "mkv", "mov", "webm", "avi", "m4v", "3gp"],
  audio: ["mp3", "m4a", "aac", "ogg", "opus", "flac", "wav"],
  text: ["txt", "md", "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp", "epub"],
  archive: ["zip", "tar", "gz", "tgz", "xz", "7z", "rar", "bz2", "zst"],
  code: ["js", "ts", "rs", "py", "kt", "java", "c", "h", "cpp", "go", "json", "toml", "yaml", "yml", "html", "css", "sh"],
};
function kindOf(name) {
  const ext = name.includes(".") ? name.split(".").pop().toLowerCase() : "";
  return Object.keys(KINDS).find((k) => KINDS[k].includes(ext)) ?? "file";
}
const KIND_ICON = { image: "file-image", video: "file-video", audio: "file-audio", text: "file-text", archive: "file-archive", code: "file-code", file: "file" };
const fileIcon = (name) => KIND_ICON[kindOf(name)];
/** Whether the window can show it itself: pictures, and text it can read. */
const peekable = (name) => kindOf(name) === "image" || /\.(txt|md|csv|json|toml|ya?ml|log|rs|js|py|sh|html|css)$/i.test(name);

/** A tile: an icon in a soft rounded square, as on the phone. */
const tile = (name, cls = "") => h("span", { class: `tile ${cls}`.trim() }, icon(name));

/** A label over a group, as on the phone. */
const label = (words) => h("p", { class: "label" }, words);

/** A place's heading, with a way back to where it was reached from. */
function pageHead(title, back, ...end) {
  return h("div", { class: "pg-hd" },
    back ? h("button", { class: "back", onclick: back.go }, icon("arrow-left"), back.words) : null,
    h("div", { class: "pg-title" }, h("h1", {}, title), h("div", { class: "pg-end" }, ...end)));
}

/** Nothing here: an icon and a few words, and what to do, if anything. */
function empty(ic, words, action) {
  return h("div", { class: "empty" }, h("span", { class: "empty-orb" }, icon(ic)), h("p", {}, words), action ?? null);
}

/** A row of a list: a tile, a name, a quieter line, and what is at its end. */
function row({ ic, name, sub, end, onclick, cls = "", title }) {
  const subs = (Array.isArray(sub) ? sub : [sub]).filter((s) => s != null && s !== "");
  return h(onclick ? "button" : "div", { class: `row ${cls}`.trim(), onclick, title },
    typeof ic === "string" ? tile(ic) : ic,
    h("span", { class: "row-main" },
      h("b", {}, name),
      subs.length ? h("small", {}, ...subs.map((s) => h("span", {}, s))) : null),
    ...(Array.isArray(end) ? end : [end]));
}

/** A switch that applies at once, and puts itself back if that fails. */
function toggle(on, apply, labelled) {
  const input = h("input", { type: "checkbox", "aria-label": labelled });
  input.checked = on;
  input.addEventListener("change", async () => {
    try { await apply(input.checked); } catch (e) { input.checked = !input.checked; toast(String(e), true); }
  });
  return h("label", { class: "switch", onclick: (e) => e.stopPropagation() }, input, h("span"));
}

/** One setting: its icon, its name, what it is now, and a switch or an arrow. */
function setting({ ic, name, value, onclick, end, danger }) {
  return h(onclick ? "button" : "div", { class: `set${danger ? " set-danger" : ""}`, onclick },
    icon(ic, "set-ic"),
    h("span", { class: "set-name" }, name),
    value != null ? h("span", { class: "set-value" }, value) : null,
    end ?? (onclick && !danger ? icon("chevron-right", "set-go") : null));
}

const group = (words, ...rows) => h("section", { class: "grp" }, words ? label(words) : null, h("div", { class: "grp-box" }, ...rows));

/**
 * A sheet over the window, as the phone's slides up from its edge: what it
 * is about, then whatever is put in it. Closed by Escape, by a click beside
 * it, or by `close()`.
 */
function sheet({ ic, title, sub, wide, onclose } = {}) {
  const box = h("div", { class: `sheet${wide ? " sheet-wide" : ""}`, role: "dialog", "aria-modal": "true" });
  const scrim = h("div", { class: "scrim", onmousedown: (e) => { if (e.target === scrim) close(); } }, box);
  const esc = (e) => { if (e.key === "Escape") { e.stopPropagation(); close(); } };
  function close() {
    if (!scrim.isConnected) return;
    document.removeEventListener("keydown", esc, true);
    scrim.classList.add("gone");
    setTimeout(() => scrim.remove(), 180);
    onclose?.();
  }
  if (title) {
    box.append(h("div", { class: "sh-hd" },
      ic ? (typeof ic === "string" ? tile(ic, "tile-lg") : ic) : null,
      h("div", { class: "sh-words" }, h("h2", {}, title), sub ? (typeof sub === "string" ? h("p", {}, sub) : sub) : null)));
  }
  document.addEventListener("keydown", esc, true);
  document.body.append(scrim);
  box.close = close;
  return box;
}

/** A sheet's actions as the phone draws a file's: round buttons with a word
 * under each, the destructive one red. Each closes the sheet first. */
function rounds(box, items) {
  const grid = h("div", { class: "rounds" });
  for (const it of items.filter(Boolean)) {
    grid.append(h("button", { class: `round${it.danger ? " round-danger" : ""}`, onclick: () => { box.close(); it.act(); } },
      h("span", { class: "round-c" }, icon(it.ic)), h("span", {}, it.words)));
  }
  box.append(grid);
  return grid;
}

/** Buttons at a sheet's foot: the quiet one, then the main one. */
function actions(...buttons) {
  return h("div", { class: "acts" }, ...buttons.filter(Boolean));
}
const btn = (words, onclick, cls = "") => h("button", { class: `btn ${cls}`.trim(), onclick }, words);

function toast(words, warn = false) {
  const t = h("div", { class: `toast${warn ? " toast-warn" : ""}`, role: "status" }, words);
  document.body.append(t);
  setTimeout(() => t.classList.add("gone"), 2800);
  setTimeout(() => t.remove(), 3300);
}

/** A failure drawn where the thing would have been. */
const oops = (e) => h("p", { class: "oops" }, String(e));

/** Replace what is in `el`, leaving out the parts that are not there:
 * `replaceChildren` itself would write a null out as the word "null". */
function put(el, ...kids) {
  el.replaceChildren(...kids.flat().filter((k) => k != null && k !== false));
}

/** A name as the start of a line: "this computer", from the engine, as
 * "This computer". */
const named = (words) => (words ? words[0].toUpperCase() + words.slice(1) : words);
