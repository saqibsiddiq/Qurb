// Setting this computer up, and unlocking it (direction §47). Since the
// owner's review of 2026-11-03 (P-1 to P-5), two screens: *Start*, which
// makes the key and `~/qurb` asking nothing, then *Add your phone*. Choosing
// the folder and how much of the disk it may take are gone: the folder is
// fixed, and a computer keeps everything (F-3). *I already use QurbSpace*
// joins with the code the phone shows, the key coming from the phone.

const SETUP = { joining: false, path: "", looked: null, allowance: "0" };

function drawSetup(where) {
  // Fixed (P-1): what this computer would use, `~/qurb/Shared` unless it was
  // set up before.
  SETUP.path = where.root || "~/qurb/Shared";
  document.body.classList.add("setting-up");
  if (where.locked) return setupStep(unlockStep);
  setupStep(welcome);
}

function setupStep(step) {
  const box = h("div", { class: "setup-in" });
  put($("setup"), box);
  step(box);
  icons(box);
  box.querySelector("input")?.focus();
}

/** The app's mark (decision 0048): a ring, the stem that makes it a q, a
 * point of light inside; the website's and the phone's drawing. */
function appMark() {
  const m = h("span", { class: "appmark" });
  m.innerHTML = '<svg viewBox="0 0 32 32" aria-hidden="true"><defs><linearGradient id="qm-tile" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3c806a"/><stop offset="1" stop-color="#22503f"/></linearGradient></defs><rect x="1" y="1" width="30" height="30" rx="9" fill="url(#qm-tile)"/><circle cx="14.4" cy="14.4" r="6.2" fill="none" stroke="#fff" stroke-width="2.6"/><path d="M20.6 11 V24.5" fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round"/><circle cx="14.4" cy="14.4" r="1.9" fill="#bde7d6"/></svg>';
  return m;
}

function welcome(box) {
  const says = h("p", { class: "note" });
  const startBtn = btn("Start", () => create(startBtn, says), "btn-main btn-start");
  // D16: the app's mark, its name, one quiet line, one button.
  box.append(
    appMark(),
    h("h1", { class: "ptitle" }, "QurbSpace"),
    h("p", { class: "quiet" }, "Your files, on your own devices"),
    startBtn,
    h("button", { class: "link", onclick: () => { SETUP.joining = true; setupStep(joinStep); } }, "I already use QurbSpace"),
    says);
}

const backTo = (step) => h("button", { class: "back", onclick: () => setupStep(step) }, icon("arrow-left"), "Back");

async function create(next, says) {
  next.disabled = true;
  try {
    await invoke("create_device", { path: SETUP.path, allowance: SETUP.allowance });
    // Straight to adding the phone, the second and last step (P-2).
    await start();
    go("pair-show", { guest: false, first: true });
  } catch (e) { says.textContent = String(e); says.classList.add("warn"); next.disabled = false; }
}

function joinStep(box) {
  let byWords = false;
  const code = h("input", { class: "digits-in", inputmode: "numeric", placeholder: "000000", autocomplete: "off", "aria-label": "The code" });
  const words = h("textarea", { class: "field", rows: "4", hidden: true, placeholder: "Your 24 words", spellcheck: "false", "aria-label": "Your 24 words" });
  const says = h("p", { class: "note" }, "On your phone: its circle, Add a device, Show a code");
  const title = h("h1", {}, "The code from your phone");
  const swap = h("button", { class: "link", onclick: () => {
    byWords = !byWords;
    code.hidden = byWords;
    words.hidden = !byWords;
    title.textContent = byWords ? "Your 24 words" : "The code from your phone";
    swap.textContent = byWords ? "Use a code instead" : "Use my 24 words";
    (byWords ? words : code).focus();
  } }, "Use my 24 words");
  const next = btn("Join", async () => {
    next.disabled = true;
    says.classList.remove("warn");
    try {
      if (byWords) {
        await invoke("enrol_device", { path: SETUP.path, phrase: words.value, allowance: SETUP.allowance });
        words.value = "";
        setupStep((b) => ready(b, "Pair it with your other devices from Home"));
      } else {
        says.textContent = "Looking for it on this Wi-Fi";
        const number = await invoke("setup_pairing_number", { path: SETUP.path, code: code.value });
        put(says, "Approve on your phone. It shows ", h("b", {}, number));
        const joined = await invoke("join_new_device", { path: SETUP.path, code: code.value, allowance: SETUP.allowance });
        setupStep((b) => ready(b, `Paired with ${joined}`));
      }
    } catch (e) { says.textContent = String(e); says.classList.add("warn"); next.disabled = false; }
  }, "btn-main");
  code.addEventListener("input", () => { if (/^\d{6}$/.test(code.value.trim())) next.click(); });
  box.append(backTo(welcome), title, code, words, says, swap, actions(next));
}

function ready(box, words) {
  box.append(
    h("span", { class: "empty-orb ok" }, icon("check")),
    h("h1", {}, "Ready"),
    h("p", { class: "note" }, words),
    actions(btn("Open QurbSpace", () => start(), "btn-main")));
}

function unlockStep(box) {
  const field = h("input", { class: "field", type: "password", placeholder: "Passphrase", "aria-label": "Passphrase" });
  const says = h("p", { class: "note" }, "Nothing syncs until it's typed");
  const open = async () => {
    const passphrase = field.value;
    field.value = "";
    if (!passphrase) return;
    try { await invoke("unlock", { passphrase }); start(); } catch (e) { says.textContent = String(e); says.classList.add("warn"); field.focus(); }
  };
  field.addEventListener("keydown", (e) => { if (e.key === "Enter") open(); });
  box.append(h("span", { class: "empty-orb" }, icon("lock-keyhole")), h("h1", {}, "QurbSpace is locked"), field, says, actions(btn("Unlock", open, "btn-main")));
}
