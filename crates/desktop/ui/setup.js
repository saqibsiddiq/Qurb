// Setting this computer up, and unlocking it (direction §47). One thing at
// a time over the whole window, the sidebar not there yet: a folder, how
// much of the disk it may take, and a code from the phone if it joins.
// Nothing decorative: this is the moment the key is made or handed over.

const SETUP = { joining: false, path: "", looked: null, allowance: null };

function drawSetup(where) {
  SETUP.path = where.root ?? "";
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

function welcome(box) {
  box.append(
    h("span", { class: "empty-orb" }, icon("folder")),
    h("h1", {}, "QurbSpace"),
    h("p", { class: "note" }, "Your files, near you on every device"),
    h("div", { class: "choices" },
      h("button", { class: "choice", onclick: () => { SETUP.joining = false; setupStep(folderStep); } },
        h("span", { class: "choice-c" }, icon("plus")), h("b", {}, "Start here"), h("small", {}, "This is my first device")),
      h("button", { class: "choice", onclick: () => { SETUP.joining = true; setupStep(folderStep); } },
        h("span", { class: "choice-c" }, icon("smartphone")), h("b", {}, "Join my phone"), h("small", {}, "With a code from it"))));
}

const backTo = (step) => h("button", { class: "back", onclick: () => setupStep(step) }, icon("arrow-left"), "Back");

function folderStep(box) {
  const input = h("input", { class: "field", value: SETUP.path, spellcheck: "false", "aria-label": "Folder" });
  const says = h("p", { class: "note" });
  const next = btn("Continue", () => { SETUP.path = input.value.trim(); setupStep(storageStep); }, "btn-main");
  let wait = null;
  const look = async () => {
    const path = input.value.trim();
    next.disabled = true;
    says.classList.remove("warn");
    if (!path) { says.textContent = ""; return; }
    let f;
    try { f = await invoke("inspect_folder", { path }); } catch (e) { says.textContent = String(e); says.classList.add("warn"); return; }
    if (path !== input.value.trim()) return;
    SETUP.looked = f;
    const refuse = (w) => { says.textContent = w; says.classList.add("warn"); };
    if (f.set_up) return refuse("Already set up, with its own key");
    if (!f.writable) return refuse("Can't be written to");
    const at = f.path === path ? "" : `${f.path}, `;
    says.textContent = !f.exists ? `${at}a new folder` : f.existing_files === 0 ? `${at}empty`
      : `${at}${f.counted_all ? f.existing_files : `over ${f.existing_files}`} things here already, all of them will sync`;
    next.disabled = false;
  };
  input.addEventListener("input", () => { clearTimeout(wait); wait = setTimeout(look, 200); });
  input.addEventListener("keydown", (e) => { if (e.key === "Enter" && !next.disabled) next.click(); });
  box.append(backTo(welcome), h("h1", {}, "Your QurbSpace folder"), input, says, actions(next));
  look();
}

const GB = 1024 ** 3;

function storageStep(box) {
  const free = Number(SETUP.looked?.free ?? 0);
  const says = h("p", { class: "note" }, free ? `${size(free)} free on this disk` : "");
  const next = btn("Continue", () => (SETUP.joining ? setupStep(joinStep) : create(next, says)), "btn-main");
  const custom = h("input", { class: "field", placeholder: "75 or 1.5 TB", hidden: true, "aria-label": "Another amount" });
  const chips = h("div", { class: "amounts" });
  const pick = (b, bytes) => {
    chips.querySelectorAll("button").forEach((x) => x.classList.toggle("on", x === b));
    SETUP.allowance = bytes;
    next.disabled = bytes === null;
  };
  for (const n of [50, 100, 250, 500]) {
    const b = h("button", { class: "amount", disabled: free > 0 && n * GB > free }, `${n} GB`);
    b.addEventListener("click", () => { custom.hidden = true; pick(b, String(n * GB)); });
    chips.append(b);
  }
  const other = h("button", { class: "amount" }, "Other");
  other.addEventListener("click", () => { pick(other, null); custom.hidden = false; custom.focus(); });
  chips.append(other);
  let wait = null;
  custom.addEventListener("input", () => {
    clearTimeout(wait);
    wait = setTimeout(async () => {
      SETUP.allowance = null;
      next.disabled = true;
      says.classList.remove("warn");
      try {
        const bytes = await invoke("read_allowance", { text: custom.value });
        if (free && Number(bytes) > free) { says.textContent = `More than the ${size(free)} free`; says.classList.add("warn"); return; }
        SETUP.allowance = bytes;
        says.textContent = size(bytes);
        next.disabled = false;
      } catch (e) { says.textContent = String(e); says.classList.add("warn"); }
    }, 200);
  });
  box.append(backTo(folderStep), h("h1", {}, "How much space it may take"), chips, custom, says, actions(next));
  // The largest that fits of the first two, chosen to begin with.
  const fits = [...chips.querySelectorAll(".amount:not(:disabled)")].filter((b) => b !== other);
  if (fits.length) fits[Math.min(1, fits.length - 1)].click();
  else pick(other, null);
}

async function create(next, says) {
  next.disabled = true;
  try {
    await invoke("create_device", { path: SETUP.path, allowance: SETUP.allowance });
    setupStep((box) => ready(box, "Add your phone from Home, with a code"));
  } catch (e) { says.textContent = String(e); says.classList.add("warn"); next.disabled = false; }
}

function joinStep(box) {
  let byWords = false;
  const code = h("input", { class: "digits-in", inputmode: "numeric", placeholder: "000000", autocomplete: "off", "aria-label": "The code" });
  const words = h("textarea", { class: "field", rows: "4", hidden: true, placeholder: "Your 24 words", spellcheck: "false", "aria-label": "Your 24 words" });
  const says = h("p", { class: "note" }, "On your phone: Add a device, then Show a code");
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
  box.append(backTo(storageStep), title, code, words, says, swap, actions(next));
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
