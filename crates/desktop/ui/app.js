// The window: three places, as on the phone — Home, Files, Settings — with
// devices and people reached through Home's circle (docs/design/desktop.md).
//
// Two rhythms (decision 0032), and the lesson of 2026-10-28 kept: a beat is
// skipped while the last one is still being answered, so slow answers can
// never pile up and freeze the window.

const PLACES = {
  home: { draw: drawHome, live: liveHome, lists: listsHome },
  device: { draw: drawDevicePage, lists: listsDevicePage, parent: "home" },
  "phone-folders": { draw: drawPhoneFolders, parent: "home" },
  add: { draw: drawAdd, parent: "home" },
  "pair-show": { draw: drawShowCode, parent: "home" },
  "pair-enter": { draw: drawEnterCode, parent: "home" },
  waiting: { draw: drawWaitingAll, live: listsWaitingAll, parent: "home" },
  files: { draw: drawFiles },
  received: { draw: drawReceived, parent: "files" },
  deleted: { draw: drawDeleted, parent: "files" },
  conflicts: { draw: drawConflicts, parent: "files" },
  settings: { draw: drawSettings },
  storage: { draw: drawStorage, parent: "settings" },
  key: { draw: drawKey, parent: "settings" },
  advanced: { draw: drawAdvanced, parent: "settings" },
  activity: { draw: drawActivity, parent: "settings" },
};

/** Whether the window is showing a running device: until it is, nothing
 * beats, since every command refuses until there is one. */
let running = false;

let place = "home";

function go(to, arg) {
  place = PLACES[to] ? to : "home";
  const lit = PLACES[place].parent ?? place;
  document.querySelectorAll("[data-go]").forEach((b) => b.classList.toggle("on", b.dataset.go === lit));
  closePanel();
  if (place !== "pair-show") stopPairing();
  const main = $("main");
  put(main);
  main.scrollTop = 0;
  PLACES[place].draw(main, arg);
}

let liveBusy = false;
setInterval(async () => {
  if (!running || liveBusy || document.hidden) return;
  liveBusy = true;
  try { await PLACES[place].live?.(); } finally { liveBusy = false; }
}, 2000);

let listsBusy = false;
setInterval(async () => {
  if (!running || listsBusy || document.hidden || document.querySelector(".scrim")) return;
  listsBusy = true;
  try { await PLACES[place].lists?.(); } finally { listsBusy = false; }
}, 8000);

// Files dragged anywhere over the window, in any place, go to the chosen
// device: the window says where, in one line, while they are held over it.
(function wireDrop() {
  const ev = window.__TAURI__?.event;
  if (!ev) return;
  const shade = $("dropping");
  ev.listen("tauri://drag-enter", () => {
    if (!running) return;
    const to = chosenDevice();
    $("dropping-words").textContent = to ? `Drop to send to ${to.name}` : "Add a device to send to";
    shade.classList.add("on");
  });
  ev.listen("tauri://drag-leave", () => shade.classList.remove("on"));
  ev.listen("tauri://drag-drop", (e) => {
    shade.classList.remove("on");
    if (!running) return;
    send(e.payload?.paths ?? []);
  });
})();

document.querySelectorAll("[data-go]").forEach((b) => b.addEventListener("click", () => go(b.dataset.go)));
icons();

/** Which the window is: setting a device up, unlocking it, or showing it.
 * Asked before anything is drawn, because the answer decides which. */
async function start() {
  let where;
  try { where = await invoke("situation"); } catch (e) { document.body.textContent = String(e); return; }
  rootPath = where.root;
  if (!where.set_up || where.locked) { running = false; drawSetup(where); return; }
  document.body.classList.remove("setting-up");
  put($("setup"));
  running = where.running;
  if (!where.running) {
    // A folder with a key that could not be opened: said once, rather than
    // every place failing the same way.
    put($("main"), h("div", { class: "locked" }, h("span", { class: "empty-orb warn" }, icon("circle-alert")),
      h("h1", {}, "QurbSpace can't open this folder"), h("p", { class: "note" }, where.problem ?? "")));
    return;
  }
  const at = new URLSearchParams(location.search).get("place");
  go(at || "home");
}

start();
