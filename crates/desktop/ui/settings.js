// Storage, and Settings.
//
// Storage is where Qurb explains its one unusual idea: freeing space without
// losing files (direction §20). Settings is grouped lists, quieter than
// everything else (§23).

// --------------------------------------------------------------- storage

const GIB = 1024 ** 3;
let disk = 0;

async function drawStorage() {
  let st, fr;
  try {
    [st, fr] = await Promise.all([invoke("storage"), invoke("freeable")]);
  } catch (e) {
    $("freeable-says").textContent = String(e);
    return;
  }
  disk = Number(st.disk);
  const used = Number(st.used);
  const freeable = Number(fr.bytes);
  const free = Number(st.free_disk);

  $("freeable-big").textContent = freeable > 0 ? size(fr.bytes) : "Nothing to free";
  $("freeable-says").textContent = freeable > 0
    ? `can be freed safely, from ${count(fr.count, "file")}. They stay in Qurb, available from your other devices.`
    : st.only_here > 0
      ? `${count(st.only_here, "file")} ${st.only_here === 1 ? "is" : "are"} only on this computer, so ${st.only_here === 1 ? "it stays" : "they stay"} until another device has ${st.only_here === 1 ? "it" : "them"}.`
      : "Everything here is either needed here or already freed.";
  $("free-all").classList.toggle("hidden", freeable === 0);

  // One bar: what Qurb uses, the part of it that could go, and the rest of
  // the disk. Against the disk, since that is what is being spent.
  const whole = Math.max(disk, used + free, 1);
  $("meter-used").style.width = `${(100 * (used - freeable)) / whole}%`;
  $("meter-freeable").style.left = `${(100 * (used - freeable)) / whole}%`;
  $("meter-freeable").style.width = `${(100 * freeable) / whole}%`;
  const legend = $("legend");
  legend.replaceChildren();
  const key = (cls, words) => {
    const k = el("span", cls);
    k.append(el("i"), words);
    legend.append(k);
  };
  key("used", `Qurb uses ${size(st.used)}`);
  if (freeable > 0) key("freeable", `${size(fr.bytes)} can be freed`);
  if (free > 0) key("free", `${size(st.free_disk)} free on this disk`);

  const list = $("freeable-list");
  list.replaceChildren();
  if (fr.files.length === 0) {
    list.append(empty("hard-drive", "No files to free right now.", null, true));
  }
  for (const f of fr.files) {
    const go = button("Free local space", "btn small", "cloud-off");
    const li = row({
      iconName: KIND_ICON[kindOf(f.path)],
      name: base(f.path),
      sub: [stateTag(f.availability), size(f.size), folderOf(f.path) || "Qurb"],
      trail: [go],
      onClick: () => openDetails(f.path),
    });
    li.dataset.path = f.path;
    go.addEventListener("click", () => freeLocal(f.path, li, drawStorage));
    list.append(li);
  }

  $("usage").textContent = Number(st.limit) > 0
    ? `${size(st.used)} of ${size(st.limit)}${st.over ? " — over the limit" : ""}`
    : "No limit set";

  // Not touched while somebody is dragging: rewriting the control they are
  // holding is the single most irritating thing a live-updating screen does.
  if (document.activeElement !== $("limit")) {
    const limit = Number(st.limit);
    $("capped").checked = limit > 0;
    $("limit-controls").classList.toggle("hidden", limit === 0);
    $("limit").value = String(toTrack(limit > 0 ? limit / GIB : disk / GIB / 10));
    showLimitLabel();
  }
}

$("free-all").addEventListener("click", async () => {
  const b = $("free-all");
  b.disabled = true;
  let freed = 0, n = 0;
  try {
    // A page at a time, largest first, until nothing more can go.
    for (let round = 0; round < 20; round++) {
      const fr = await invoke("freeable");
      if (fr.files.length === 0) break;
      for (const f of fr.files) {
        try { freed += Number(await invoke("free_file", { path: f.path })); n++; } catch (e) { /* kept, and said below */ }
      }
    }
    toast(`Freed ${size(String(freed))} from ${count(n, "file")}. They stay in Qurb.`);
  } finally {
    b.disabled = false;
    drawStorage();
  }
});

// The track is square-law rather than linear.
//
// A 500 GB disk against an allowance somebody actually wants — ten or twenty
// gigabytes — puts the useful part of a linear slider in its first four
// percent, where it cannot be aimed at. Squaring gives the small end most of
// the track and leaves the large end coarse, which is the right way round:
// nobody needs 380 GB rather than 390.
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
  $("limit-label").textContent = `${gib} GB${share}`;
}

$("limit").addEventListener("input", showLimitLabel);

$("capped").addEventListener("change", async (event) => {
  $("limit-controls").classList.toggle("hidden", !event.target.checked);
  // Unchecking means no limit, and is worth applying at once: somebody who has
  // just turned a limit off is asking for the cap to stop, not asking to press
  // a second button.
  if (!event.target.checked) await saveLimit("0");
});

$("apply").addEventListener("click", () => saveLimit(String(chosenGib() * GIB)));

async function saveLimit(bytes) {
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

// ------------------------------------------------------------- security

const PROTECTION = {
  file: "In a file only you can read. Enough against other people using this computer; " +
    "not against anyone who can read its disk.",
  keystore: "In the system keystore, locked while you're logged out. Qurb opens it without " +
    "asking while you're logged in.",
  passphrase: "Behind a passphrase only you know. Qurb asks for it when it starts, and nothing " +
    "syncs until you type it — the only option that protects the key from someone with the disk and your login.",
};

/** Which change is being made in the passphrase form, if one is. */
let protecting = null;

async function drawSecurity() {
  let s;
  try {
    s = await invoke("security");
  } catch (e) {
    $("protection-says").textContent = String(e);
    return;
  }
  $("set-identity").textContent = s.identity;
  $("protection-says").textContent = PROTECTION[s.protection] ?? s.protection;

  const buttons = $("protection-buttons");
  buttons.replaceChildren();
  const offer = (label, action) => {
    const b = el("button", "btn small", label);
    b.addEventListener("click", action);
    buttons.append(b);
  };
  const was = s.protection;
  if (was !== "passphrase") offer("Protect with a passphrase…", () => protectForm("passphrase", was));
  if (was === "passphrase") offer("Change passphrase…", () => protectForm("passphrase", was));
  if (was !== "keystore") offer("Use the keystore", () => protectForm("keystore", was));

  const events = $("security-events");
  events.replaceChildren();
  if (s.events.length === 0) events.append(el("li", "meta", "Nothing yet"));
  for (const r of s.events.slice(0, 6)) {
    events.append(row({
      iconName: r.kind === "removed" ? "x" : "monitor-smartphone",
      name: r.device ?? r.detail ?? "",
      sub: [r.kind === "removed" ? "Removed" : "Paired", when(r.at)],
    }));
  }
}

/** The fields a change of protection needs, and nothing else. */
function protectForm(to, was) {
  protecting = { to, was };
  const form = $("protect-form");
  form.classList.remove("hidden");
  $("protect-current-wrap").classList.toggle("hidden", was !== "passphrase");
  const wantsNew = to === "passphrase";
  $("protect-new-wrap").classList.toggle("hidden", !wantsNew);
  $("protect-again-wrap").classList.toggle("hidden", !wantsNew);
  $("protect-says").classList.add("hidden");
  for (const id of ["protect-current", "protect-new", "protect-again"]) $(id).value = "";
  $("protect-go").textContent = wantsNew ? "Set the passphrase" : "Use the keystore";
  (was === "passphrase" ? $("protect-current") : wantsNew ? $("protect-new") : $("protect-go")).focus();
}

$("protect-cancel").addEventListener("click", () => {
  protecting = null;
  $("protect-form").classList.add("hidden");
});

$("protect-go").addEventListener("click", async () => {
  if (!protecting) return;
  const says = $("protect-says");
  const current = $("protect-current").value || null;
  const fresh = $("protect-new").value;
  const again = $("protect-again").value;
  if (protecting.to === "passphrase" && fresh !== again) {
    says.textContent = "The two new passphrases aren't the same.";
    says.classList.remove("hidden");
    return;
  }
  try {
    await invoke("protect_key", {
      to: protecting.to,
      current,
      new: protecting.to === "passphrase" ? fresh : null,
    });
    protecting = null;
    $("protect-form").classList.add("hidden");
    toast("Key protection changed");
    drawSecurity();
  } catch (e) {
    says.textContent = String(e);
    says.classList.remove("hidden");
  } finally {
    for (const id of ["protect-current", "protect-new", "protect-again"]) $(id).value = "";
  }
});

// -------------------------------------------------------------- settings

async function drawSettings() {
  drawSecurity();
  let s;
  try {
    s = await invoke("settings");
  } catch (e) {
    $("settings-says").textContent = String(e);
    return;
  }

  // A computer has nothing like a phone's Google backup: what keeps its key
  // safe is another device that holds it, best a phone (decision 0053).
  const others = s.key_also_on ?? [];
  $("key-safe").textContent = others.length === 0
    ? "Only on this computer. Pair your phone: it can give the key to a new computer with a code, and keeps it in its Google backup."
    : `On this computer and on ${others.join(", ")}. Any of them can give it to a new device with a code${s.phone_holds_key ? "; your phone also keeps it in its Google backup" : ". Pair a phone too: it keeps the key in its Google backup"}.`;
  $("key-safe").classList.toggle("warn", others.length === 0);

  // Not rewritten under somebody who is in the middle of typing.
  const editing = document.activeElement?.closest?.("#settings .item");
  if (!editing) {
    $("set-name").value = s.name;
    $("set-signal").value = s.signal;
    $("set-relay").value = s.relay ?? "";
    $("set-port").value = String(s.port);
    $("set-downloads").value = s.downloads;
  }
  $("set-root").textContent = s.root;
  $("set-version").textContent = s.version;
  $("set-private").checked = s.own_files_private;
  $("set-notify").checked = s.notifications;
  $("motion-says").textContent = calm() ? "Reduced" : "Full";
  showTheme();

  // Where that actually is, since the setting can be empty or "off". Serde
  // sends a Result as {Ok} or {Err}.
  const at = s.downloads_at;
  $("downloads-at").textContent =
    "Err" in at ? `Refused: ${at.Err}` :
    at.Ok === null ? "Kept inside your Qurb folder." :
    `Now: ${at.Ok}`;
  $("open-downloads").classList.toggle("hidden", !("Ok" in at) || at.Ok === null);

  try {
    const devices = await invoke("devices");
    $("paired-says").textContent = devices.length === 0 ? "None yet" : devices.map((d) => d.name).join(", ");
  } catch (e) { /* the row says nothing */ }

  $("at-login").checked = await invoke("starts_at_login");
}

/** A switch that applies at once, and puts itself back if that fails. */
/** The theme choice (decision 0056): System, Light or Dark, applied at once. */
function showTheme() {
  const chosen = window.qurbTheme.chosen();
  for (const b of $("theme-choice").querySelectorAll("button")) {
    b.setAttribute("aria-checked", String(b.dataset.theme === chosen));
  }
}
for (const b of $("theme-choice").querySelectorAll("button")) {
  b.addEventListener("click", () => { window.qurbTheme.choose(b.dataset.theme); showTheme(); });
}

function applyAtOnce(id, apply, onWords, offWords) {
  $(id).addEventListener("change", async (event) => {
    const on = event.target.checked;
    try {
      await apply(on);
      toast(on ? onWords : offWords);
    } catch (e) {
      event.target.checked = !on;
      toast(String(e), true);
    }
  });
}

applyAtOnce("at-login", (on) => invoke("set_starts_at_login", { on }),
  "Qurb will start when you log in", "Qurb won't start by itself");
applyAtOnce("set-private", (on) => invoke("set_privacy", { private: on }),
  "Files you add here now go to your Private Vault", "Files you add here now go to all your devices");
applyAtOnce("set-notify", (on) => invoke("set_notifications", { on }),
  "Notifications on", "Notifications off");

$("quit").addEventListener("click", () => invoke("quit"));

$("settings-save").addEventListener("click", async () => {
  const says = $("settings-says");
  try {
    await invoke("save_settings", {
      name: $("set-name").value,
      signal: $("set-signal").value,
      relay: $("set-relay").value,
      port: Number($("set-port").value) || 0,
      downloads: $("set-downloads").value,
    });
    toast("Settings saved");
    // So "Now: …" shows what was just saved rather than what was there.
    document.activeElement?.blur?.();
    drawSettings();
  } catch (e) {
    says.textContent = String(e);
    says.classList.add("warn");
  }
});

// The name and where received files go are the ones people change: saved as
// soon as the field is left, rather than waiting for a button at the foot.
for (const id of ["set-name", "set-downloads"]) {
  $(id).addEventListener("change", () => $("settings-save").click());
}

$("open-downloads").addEventListener("click", async () => {
  try { await invoke("open_downloads"); } catch (e) { toast(String(e), true); }
});

$("show-phrase").addEventListener("click", async () => {
  const wrap = $("revealed-wrap");
  const list = $("revealed");
  const says = $("phrase-says");

  // A second press hides them again, so they are not left on a screen somebody
  // walks away from.
  if (!wrap.classList.contains("hidden")) {
    list.replaceChildren();
    wrap.classList.add("hidden");
    $("show-phrase").textContent = "Show";
    return;
  }

  try {
    const words = await invoke("reveal_phrase");
    list.replaceChildren();
    for (const word of words) list.append(el("li", null, word));
    wrap.classList.remove("hidden");
    says.classList.add("hidden");
    $("show-phrase").textContent = "Hide";
  } catch (e) {
    wrap.classList.remove("hidden");
    says.textContent = String(e);
    says.classList.remove("hidden");
  }
});
