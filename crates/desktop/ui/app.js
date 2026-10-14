// Which of the two the window is -- setting a device up, or showing one -- and
// moving between places once it is showing one.
//
// Two rhythms, because two kinds of thing change at different rates:
//
//   - the daemon's live state (syncing, devices reachable, what is moving) is
//     polled often and cheaply, because it changes many times a second and
//     only the latest value matters;
//   - lists are fetched when their place is opened, and refreshed on a slower
//     beat, because they change rarely and are expensive to redraw under
//     somebody's cursor.

// The window opens on one of two things: setting a device up, or showing one.
// Asked before anything is drawn, because the answer decides which.
//
// True until the answer is in, so the timers at the bottom draw nothing for a
// device that may not exist: each of their commands refuses until it does.
let settingUp = true;

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

  // Locked is shown in the setting-up frame too: nothing else can answer
  // until the key is open (decision 0046).
  settingUp = !where.set_up || where.locked;
  rootPath = where.root;
  $("setup").classList.toggle("hidden", !settingUp);
  $("app").classList.toggle("hidden", settingUp);

  if (where.locked) {
    step("unlock");
    $("unlock-passphrase").focus();
    return;
  }
  if (settingUp) {
    $("folder-path").value = where.root;
    step("welcome");
    return;
  }

  if (!where.running) {
    // A folder with a key that could not be opened. The commands will refuse,
    // so say why once rather than showing every place with the same failure.
    setHero({ mark: "error", icon: "circle-alert", title: "Qurb can't open this folder", says: where.problem ?? "" });
    return;
  }

  showScreen("home");
  drawAttention();
  drawChip();
}

// ---------------------------------------------------------------- navigation

let screen = "home";

/** Places that are not in the sidebar light up the one they belong to. */
function showScreen(name) {
  screen = name;
  closePanel();
  const section = $(name);
  const lit = section?.dataset.parent ?? name;
  document.querySelectorAll("#tabs .nav").forEach((b) =>
    b.classList.toggle("on", b.dataset.screen === lit));
  document.querySelectorAll(".screen").forEach((s) => s.classList.toggle("on", s.id === name));
  $("scroll").scrollTop = 0;
  refreshScreen();
}

document.querySelectorAll("#tabs .nav").forEach((b) => {
  b.addEventListener("click", () => showScreen(b.dataset.screen));
});
document.querySelectorAll("[data-go]").forEach((b) => {
  b.addEventListener("click", () => showScreen(b.dataset.go));
});

function refreshScreen() {
  if (settingUp) return;
  if (screen === "home") { drawHome(); drawHomeLists(); }
  if (screen === "files") { filesBrowser.draw(); drawDeletedLink(); }
  if (screen === "vault") vaultBrowser.draw();
  if (screen === "devices") drawDevices();
  if (screen === "storage") drawStorage();
  if (screen === "settings") drawSettings();
  if (screen === "activity") drawActivity();
  if (screen === "deleted") drawDeleted();
}

decide();

// The live state, often. A poll rather than a subscription because the value
// is one small struct and the window is in the same process as the daemon
// that publishes it: the cost of asking is a channel read.
setInterval(() => {
  if (settingUp) return;
  if (screen === "home") drawHome();
  drawChip();
}, 1500);

// Lists, rarely, and only the one being looked at. Redrawing a list somebody
// is reading is a cost, not a feature. Not while a menu, sheet or panel is
// open over it, which would redraw what is being pointed at.
let slowTicks = 0;
setInterval(() => {
  if (settingUp) return;
  // Conflicts every third beat: finding them reads every path.
  if (++slowTicks % 3 === 0) drawAttention();
  if (document.querySelector(".menu, .scrim") || openPanel) return;
  if (screen === "home") drawHomeLists();
  if (screen === "storage") drawStorage();
  if ((screen === "files" || screen === "vault") && wanted.size) refreshScreen();
  if (screen === "devices" && !watching && !removing) drawDevices();
}, 5000);
