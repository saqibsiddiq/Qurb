// Setting a device up, and unlocking one (direction §47).
//
// Sparse on purpose: setting a device up hands it the key, the highest-trust
// moment in the product, so these screens carry the visual system and nothing
// decorative. There is no phrase to write down (decision 0052).

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
    // Made and started in one go: there is no phrase to write down first
    // (decision 0052). Another device is added with a code, which carries
    // the key.
    await invoke("create_device", { path: $("folder-path").value.trim(), allowance });
    $("ready-says").textContent = "This computer is your first device, and Qurb is watching your folder. Add your phone from Devices, with a code.";
    step("ready");
  } catch (e) {
    $("storage-says").textContent = String(e);
    $("storage-says").classList.add("warn");
  } finally {
    next.disabled = false;
  }
});

/** Whether the join step takes a code (the usual) or the 24 words. */
let byWords = false;

$("join-words").addEventListener("click", () => {
  byWords = !byWords;
  $("code-field").classList.toggle("hidden", byWords);
  $("phrase-field").classList.toggle("hidden", !byWords);
  $("join-title").textContent = byWords ? "Enter your recovery phrase" : "Enter the code from your other device";
  $("join-lead").textContent = byWords
    ? "The 24 words of your key. Order matters; spacing and capitals don't."
    : "On your phone, open Qurb, go to Devices, choose Add a device and then Show a code on this phone. Type that code here. It brings your key with it, so there's nothing else to type.";
  $("join-words").textContent = byWords ? "Use a code instead" : "Use my 24 words instead";
  $("join-says").classList.add("hidden");
});

$("join-next").addEventListener("click", async () => {
  const says = $("join-says");
  const b = $("join-next");
  b.disabled = true;
  try {
    const path = $("folder-path").value.trim();
    if (byWords) {
      await invoke("enrol_device", { path, phrase: $("given-phrase").value, allowance });
      // Off the screen as soon as it has been used.
      $("given-phrase").value = "";
      $("ready-says").textContent = "This computer now shares your key, and Qurb is watching your folder. Pair it with your other devices from Devices.";
    } else {
      const joined = await invoke("join_new_device", { path, code: $("given-code").value, allowance });
      $("given-code").value = "";
      $("ready-says").textContent = `This computer is now one of your devices, paired with ${joined}, and Qurb is watching your folder.`;
    }
    says.classList.add("hidden");
    step("ready");
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  } finally {
    b.disabled = false;
  }
});

$("ready-next").addEventListener("click", () => decide());
