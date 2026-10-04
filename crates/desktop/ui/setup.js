// Setting a device up, and unlocking one (direction §47).
//
// Sparse on purpose: the recovery phrase is the highest-trust moment in the
// product, so these screens carry the visual system and nothing decorative.

/** Which onboarding step is showing. */
function step(name) {
  document.querySelectorAll("#setup .step").forEach((s) => {
    s.classList.toggle("on", s.dataset.step === name);
  });
}

$("get-started").addEventListener("click", () => step("choose"));

// Unlocking a key protected by a passphrase. The passphrase goes to the
// command and is not kept: the field is cleared whatever the answer.
async function unlock() {
  const field = $("unlock-passphrase");
  const says = $("unlock-says");
  const go = $("unlock-go");
  const passphrase = field.value;
  field.value = "";
  if (!passphrase) return;
  go.disabled = true;
  says.classList.add("hidden");
  try {
    await invoke("unlock", { passphrase });
    decide();
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
    field.focus();
  } finally {
    go.disabled = false;
  }
}
$("unlock-go").addEventListener("click", unlock);
$("unlock-passphrase").addEventListener("keydown", (e) => { if (e.key === "Enter") unlock(); });

/** The path chosen, and whether the next button may be pressed. */
let joining = false;

$("choose-new").addEventListener("click", () => { joining = false; step("folder"); lookAtFolder(); });
$("choose-join").addEventListener("click", () => { joining = true; step("folder"); lookAtFolder(); });

document.querySelectorAll("#setup [data-back]").forEach((b) => {
  b.addEventListener("click", () => step(b.dataset.back));
});

let looking = null;

/** What the folder step last found, for the storage step's free space. */
let looked = null;
$("folder-path").addEventListener("input", () => {
  clearTimeout(looking);
  looking = setTimeout(lookAtFolder, 200);
});

async function lookAtFolder() {
  const next = $("folder-next");
  const says = $("folder-says");
  const path = $("folder-path").value.trim();
  says.classList.remove("warn");
  if (!path) {
    says.textContent = "";
    next.disabled = true;
    return;
  }

  let folder;
  try {
    folder = await invoke("inspect_folder", { path });
    looked = folder;
  } catch (e) {
    says.textContent = String(e);
    says.classList.add("warn");
    next.disabled = true;
    return;
  }

  // Each of these is a reason not to continue, and each says what to do about
  // it. "Invalid" on its own is the least useful thing an interface can say.
  const refuse = (words) => {
    says.textContent = words;
    says.classList.add("warn");
    next.disabled = true;
  };
  if (folder.set_up && !joining) return refuse("Qurb is already set up in this folder. Choose another, or open that one instead.");
  if (folder.set_up && joining) return refuse("Qurb is already set up in this folder, with its own key.");
  if (!folder.writable) return refuse("This folder can't be written to.");

  const disk = folder.disk === "0" ? "" : ` · ${size(folder.free)} free on this disk`;
  // Where it is, in full, whenever that is not exactly what was typed: a
  // folder typed as "home/project/qurb" is in the home folder, and that is
  // worth seeing before Qurb is set up there rather than after.
  const at = folder.path === path ? "" : ` at ${folder.path}`;
  if (!folder.exists) {
    says.textContent = `A new folder${at}${disk}`;
  } else if (folder.existing_files === 0) {
    says.textContent = `Empty${at}${disk}`;
  } else {
    // Said plainly: everything already in there is about to appear on every
    // other device, which is a surprise worth not having.
    const n = folder.counted_all ? `${folder.existing_files}` : `Over ${folder.existing_files}`;
    says.textContent = `${n} things already here${at} — all of them will sync${disk}`;
  }
  next.disabled = false;
}

$("folder-next").addEventListener("click", () => {
  offerAllowances(looked ? Number(looked.free) : 0);
  step("storage");
});

// ------------------------------------------------------------------ the allowance
//
// Asked before the key is made (decision 0038): how much of this disk qurb may
// take. The brief's four figures and a custom one; "no limit" is not offered
// here, and stays what `qurb config <dir> limit=0` sets.

const GB = 1024 ** 3;
const PRESETS = [50, 100, 250, 500];

/**
 * An amount in the unit the question is asked in. "GB" here is the unit the
 * presets use and `qurb config limit=50G` means, 2^30 bytes, so the screen
 * speaks one unit throughout rather than offering "GB" and answering "GiB".
 */
function gb(bytes) {
  const n = Number(bytes) / GB;
  if (n >= 1024) return `${(n / 1024).toFixed(1)} TB`;
  return `${n < 10 ? n.toFixed(1) : n.toFixed(0)} GB`;
}

/** Bytes chosen, as a string, or null while nothing valid is. */
let allowance = null;

function offerAllowances(free) {
  const box = $("allowances");
  box.replaceChildren();
  allowance = null;
  $("allowance-custom").classList.add("hidden");
  $("custom-allowance").value = "";

  const pick = (b, bytes) => {
    box.querySelectorAll("button").forEach((x) => x.classList.toggle("on", x === b));
    allowance = bytes;
    $("storage-next").disabled = allowance === null;
  };

  for (const amount of PRESETS) {
    const b = el("button", "btn", `${amount} GB`);
    // Known free space only: a disk that could not be looked at does not
    // disable anything, rather than disabling everything.
    if (free > 0 && amount * GB > free) {
      b.disabled = true;
      b.title = `More than the ${gb(free)} free on this disk`;
    }
    b.addEventListener("click", () => {
      $("allowance-custom").classList.add("hidden");
      pick(b, String(amount * GB));
      tellAboutAllowance(free);
    });
    box.append(b);
  }

  const custom = el("button", "btn", "Another amount");
  custom.addEventListener("click", () => {
    pick(custom, null);
    $("allowance-custom").classList.remove("hidden");
    $("custom-allowance").focus();
    readCustom(free);
  });
  box.append(custom);

  // The largest preset that fits, chosen to begin with, so that Continue
  // works at once for somebody content with a sensible figure.
  const fits = [...box.querySelectorAll("button")].filter((b) => !b.disabled && b !== custom);
  if (fits.length) fits[Math.min(1, fits.length - 1)].click();
  tellAboutAllowance(free);
}

function tellAboutAllowance(free) {
  const says = $("storage-says");
  const disk = free > 0 ? `${gb(free)} free on this disk.` : "";
  const over = free > 0 && PRESETS.some((amount) => amount * GB > free)
    ? " Larger amounts are more than it has free." : "";
  says.textContent = disk + over;
  says.classList.remove("warn");
}

let reading = null;
$("custom-allowance").addEventListener("input", () => {
  clearTimeout(reading);
  reading = setTimeout(() => readCustom(looked ? Number(looked.free) : 0), 200);
});

async function readCustom(free) {
  const says = $("storage-says");
  const text = $("custom-allowance").value;
  allowance = null;
  $("storage-next").disabled = true;
  if (!text.trim()) { tellAboutAllowance(free); return; }
  try {
    const bytes = await invoke("read_allowance", { text });
    if (free > 0 && Number(bytes) > free) {
      says.textContent = `${gb(bytes)} is more than the ${gb(free)} free on this disk.`;
      says.classList.add("warn");
      return;
    }
    allowance = bytes;
    says.textContent = `${gb(bytes)}, of ${gb(free)} free.`;
    says.classList.remove("warn");
    $("storage-next").disabled = false;
  } catch (e) {
    says.textContent = String(e);
    says.classList.add("warn");
  }
}

$("storage-next").addEventListener("click", async () => {
  if (allowance === null) return;
  if (joining) { step("join"); return; }

  const next = $("storage-next");
  next.disabled = true;
  try {
    await invoke("create_device", { path: $("folder-path").value.trim(), allowance });
    await showPhrase();
    step("phrase");
  } catch (e) {
    $("storage-says").textContent = String(e);
    $("storage-says").classList.add("warn");
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
  // Three, chosen at random each time, so that pressing "show the words
  // again" and coming back is not a way to learn the answer to the same
  // question.
  const positions = new Set();
  while (positions.size < 3) positions.add(1 + Math.floor(Math.random() * 24));
  asked = [...positions].sort((a, b) => a - b);

  const box = $("asks");
  box.replaceChildren();
  for (const position of asked) {
    const field = el("label");
    field.append(el("span", null, `Word ${position}`));
    const input = el("input", "input");
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
    $("verify-says").textContent = "Those aren't the words at those places. Check your paper, and the numbers.";
    $("verify-says").classList.remove("hidden");
    return;
  }

  // Confirmed, so the words come off the screen. The session has already
  // dropped its copy; this drops the only other one.
  $("words").replaceChildren();
  $("asks").replaceChildren();
  $("ready-says").textContent = "This computer is your first device, and Qurb is watching your folder.";
  step("ready");
});

$("join-next").addEventListener("click", async () => {
  const says = $("join-says");
  const b = $("join-next");
  b.disabled = true;
  try {
    await invoke("enrol_device", {
      path: $("folder-path").value.trim(),
      phrase: $("given-phrase").value,
      allowance,
    });
    // Off the screen as soon as it has been used.
    $("given-phrase").value = "";
    says.classList.add("hidden");
    $("ready-says").textContent = "This computer now shares your key, and Qurb is watching your folder.";
    step("ready");
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  } finally {
    b.disabled = false;
  }
});

$("ready-next").addEventListener("click", () => decide());
