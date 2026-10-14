// Devices, adding one, and removing one.
//
// "My phone", "my laptop" -- never nodes (direction §16). A device shows its
// name, what it is, whether it is here now; identities and addresses are kept
// under Technical details for whoever wants them.

/** The fingerprint of a device that has just been paired, to materialise. */
let justPaired = null;

/** How a device is reached right now, in words, and whether it is here. */
function presence(d) {
  if (d.route === "direct") return { on: true, words: "Connected" };
  if (d.route === "relay") return { on: true, words: "Connected through the relay" };
  if (d.last_seen) return { on: false, words: `Last seen ${when(d.last_seen)}` };
  return { on: false, words: "Not connected yet" };
}

function presenceLine(d) {
  const p = presence(d);
  const line = el("span", p.on ? "presence on" : "presence");
  line.append(el("span", "dot"), el("span", null, p.words));
  return line;
}

async function drawDevices() {
  const grid = $("device-list");
  let devices, me, out;
  try {
    [devices, me, out] = await Promise.all([invoke("devices"), invoke("settings"), invoke("outgoing")]);
  } catch (e) {
    oops(grid, e);
    return;
  }
  grid.replaceChildren();

  const self = el("div", "device self glass-frosted");
  const t = el("span", "tile");
  t.append(icon("laptop"));
  const words = el("div");
  const here = el("span", "presence on");
  here.append(el("span", "dot"), el("span", null, "This computer"));
  words.append(el("strong", null, me.name), here);
  self.append(t, words);
  grid.append(self);

  // This person's own devices first, then other people's (decision 0060):
  // guests of this computer, and computers it visits.
  const groups = [
    ["own", null],
    ["guest", "Guests"],
    ["host", "Computers you visit as a guest"],
  ];
  // One card a device: one set up again is shown as what it became, and its
  // earlier self is reached from there (the owner's item 6, 2026-10-14).
  everyDevice = devices;
  for (const [relation, label] of groups) {
    const these = devices.filter((d) => (d.relation ?? "own") === relation && !d.replaced_by);
    if (these.length === 0) continue;
    if (label) {
      const heading = el("div", "list-label", label);
      heading.style.gridColumn = "1 / -1";
      grid.append(heading);
    }
    for (const d of these) drawCard(d);
  }

  function drawCard(d) {
    const card = el("button", "device glass-frosted");
    const tile = el("span", "tile");
    tile.append(icon(deviceIcon(d.name, d.kind)));
    const text = el("div");
    text.append(el("strong", null, d.name), presenceLine(d));
    const waiting = out.filter((o) => o.to_id === d.id).length;
    if (waiting) text.append(el("span", "meta", `${count(waiting, "file")} waiting for it`));
    card.append(tile, text);
    if (justPaired === d.fingerprint) { card.classList.add("arrived"); justPaired = null; }
    card.addEventListener("click", () => openDevice(d));
    grid.append(card);
  }

  if (devices.length === 0) {
    const add = button("Add a device", "btn primary", "plus");
    add.addEventListener("click", openAddDevice);
    const hint = empty("monitor-smartphone", "Add your phone or another computer, and your files move between them.", add);
    hint.style.gridColumn = "1 / -1";
    grid.append(hint);
  }
}

$("add-device").addEventListener("click", () => openAddDevice());

// ------------------------------------------------------------ one device

/** Every paired device as last drawn, those set up again included. */
let everyDevice = [];

/**
 * The device whose removal is being asked about, if one is. The list is not
 * redrawn meanwhile: a redraw every few seconds would take the question away
 * from under somebody reading it.
 */
let removing = null;

async function openDevice(d) {
  const box = panel();
  const head = el("div", "panel-head");
  const tile = el("span", "tile");
  tile.append(icon(deviceIcon(d.name, d.kind), "large"));
  const names = el("div");
  names.style.paddingRight = "32px";
  names.append(el("h2", null, d.name), presenceLine(d));
  head.append(tile, names);

  const body = el("div", "panel-body");
  const facts = el("dl", "facts");
  const p = presence(d);
  // One of this person's phones: its folders, chosen here (decision 0063).
  const ownPhone = d.relation === "own" && d.kind === "phone";
  let folders = null;
  if (ownPhone) {
    try { folders = await invoke("phone_folders", { phone: d.fingerprint }); } catch (e) { /* not shown */ }
  }
  const chosen = folders ? folders.folders.filter((f) => f.chosen) : [];
  // What it was before it was set up again, if it was.
  const earlier = d.earlier ? everyDevice.find((e) => e.id === d.earlier) : null;
  const who = {
    guest: "A guest: another person's device, with its own key. It sees only what you send it.",
    host: "Another person's computer, which you visit as a guest. It sees only what you send it.",
  }[d.relation];
  for (const [term, value] of [
    ["Status", p.words],
    ["Paired", when(d.paired_at)],
    ...(who ? [["Who", who]] : []),
    // Their private folder here: sealed on their phone, so nothing on this
    // computer can open it -- not its name, not a byte (decision 0060).
    ...(d.relation === "guest"
      ? [["Their folder", Number(d.kept ?? 0) > 0
          ? `${size(d.kept)} kept here, sealed: you cannot open it`
          : "Nothing kept here yet"]]
      : []),
    // A phone's WhatsApp, kept here in its Private Vault: out of sight, and
    // brought back only through Qurb on a phone (decision 0062).
    ...(d.whatsapp
      ? [["WhatsApp", `${count(d.whatsapp.files, "file")}, ${size(d.whatsapp.bytes)}, kept here for it, out of sight`]]
      : []),
    ...(chosen.length
      ? [["Folders", `${count(chosen.length, "folder")} backed up here: ` +
          `${count(chosen.reduce((n, f) => n + f.kept_files, 0), "file")} so far`]]
      : []),
    ...(earlier
      ? [["Set up again", `${when(d.paired_at)}. ` + (earlier.held
          ? `From before, ${count(earlier.held.files, "file")}, ${size(earlier.held.bytes)}, are still kept here.`
          : "Nothing from before is kept here.")]]
      : []),
  ]) facts.append(el("dt", null, term), el("dd", null, value));
  body.append(facts);

  let out = [];
  try { out = (await invoke("outgoing")).filter((o) => o.to_id === d.id); } catch (e) { /* none shown */ }
  if (out.length) {
    body.append(el("div", "list-label", "Waiting for it to collect"));
    const list = el("ul", "rows");
    for (const o of out) list.append(row({ iconName: KIND_ICON[kindOf(o.path)], name: base(o.path), sub: [size(o.size)] }));
    body.append(list);
  }

  const actions = el("div", "actions");
  actions.style.flexDirection = "column";
  actions.style.alignItems = "stretch";
  const send = button("Send files…", "btn primary", "send");
  send.style.justifyContent = "flex-start";
  send.addEventListener("click", () => openSend([], d));
  // A guest's folder, sealed here: opened only with their approval on their
  // phone (decision 0060).
  if (d.relation === "guest" && Number(d.kept ?? 0) > 0) {
    const look = button("Open their folder…", "btn", "lock-keyhole");
    look.style.justifyContent = "flex-start";
    look.addEventListener("click", () => openGuestFolder(d));
    actions.append(look);
  }
  // Removing it, from here rather than anywhere more prominent: it is rare,
  // and it is the one thing on this screen that cannot be undone without the
  // other device in hand.
  const remove = button(d.relation === "guest" ? "Remove this guest…" : "Remove this device…", "btn ghost quiet-danger", "x");
  remove.style.justifyContent = "flex-start";
  remove.addEventListener("click", () => askToRemove(d, body));
  // The main thing first, then what is particular to this device, then
  // removing, which is rare.
  actions.append(send);
  if (ownPhone) {
    const back = button("Choose folders to back up…", "btn", "hard-drive");
    back.style.justifyContent = "flex-start";
    back.addEventListener("click", () => openPhoneFolders(d));
    actions.append(back);
  }
  if (earlier) {
    const before = button("Remove the earlier one…", "btn ghost", "history");
    before.style.justifyContent = "flex-start";
    before.addEventListener("click", () => askToRemove(earlier, body, `the earlier ${earlier.name}, from before it was set up again`));
    actions.append(before);
  }
  actions.append(remove);
  body.append(actions);

  // For whoever wants them, folded away (§16: no identifiers by default).
  const tech = el("details");
  tech.style.marginTop = "24px";
  tech.append(el("summary", "meta", "Technical details"));
  const more = el("dl", "facts");
  for (const [term, value] of [
    ["Identity", d.fingerprint],
    ["Path", d.route === "relay" ? "Relayed" : d.route === "direct" ? "Direct" : "Not connected"],
    ...(d.address ? [["Address", d.address]] : []),
    ["Transport", "QUIC, end-to-end encrypted with this device's pinned key"],
  ]) more.append(el("dt", null, term), el("dd", "mono", value));
  tech.append(more);
  body.append(tech);

  box.append(head, body);
}

/**
 * A phone's folders, for choosing which it backs up here (decision 0063).
 * The phone lists them for this computer once it has "All files access";
 * ticked ones reach it the next time it syncs, and it keeps them where they
 * lie, files deleted on it staying here.
 */
async function openPhoneFolders(d) {
  const box = sheet({ wide: true, onClose: () => refreshScreen() });
  box.append(el("h2", null, `Back up ${d.name}'s folders`));
  let answer;
  try {
    answer = await invoke("phone_folders", { phone: d.fingerprint });
  } catch (e) {
    oops(box, e);
    return;
  }
  if (answer.seen_at == null || answer.folders.length === 0) {
    box.append(el("p", "lead",
      `${d.name} hasn't listed its folders here yet. On the phone, open Qurb, then Settings, ` +
      "Backups, Back up your folders, and allow it to read this phone's files. Its folders " +
      "appear here after it next syncs."));
    const actions = el("div", "actions end");
    const close = button("Close", "btn primary");
    close.addEventListener("click", () => box.close());
    actions.append(close);
    box.append(actions);
    return;
  }
  box.append(el("p", "lead",
    `The folders you choose are kept here, in ${d.name}'s Private Vault: over Wi-Fi, and files ` +
    "deleted on the phone stay here."));
  box.append(el("div", "list-label", `On ${d.name}, as it said ${when(answer.seen_at)}`));
  const picks = el("div", "group glass-frosted");
  const inputs = [];
  for (const f of answer.folders) {
    const item = el("label", "item");
    const label = el("div", "label");
    const depth = f.path.split("/").length;
    if (depth > 1) label.style.paddingLeft = "24px";
    label.append(el("strong", null, depth > 1 ? base(f.path) : f.path));
    const facts = `${count(f.files, "file")}, ${size(f.size)}` + (f.kept_files ? ` · ${count(f.kept_files, "file")} kept here` : "");
    const meta = el("p", "meta", facts);
    label.append(meta);
    const sw = el("span", "switch");
    const input = el("input");
    input.type = "checkbox";
    input.value = f.path;
    input.checked = f.chosen;
    sw.append(input, el("span"));
    item.append(icon("folder"), label);
    // Before the switch, so that the switches stay in one column.
    if (f.chosen && f.kept_files) {
      const look = button("Open", "btn small ghost");
      look.addEventListener("click", (event) => {
        event.preventDefault();
        openBackup(d, f.path);
      });
      item.append(look);
    }
    item.append(sw);
    picks.append(item);
    inputs.push({ input, meta, facts });
  }
  // A folder inside a chosen one is backed up with it: shown ticked, fixed,
  // and saying so, so that the two cannot disagree.
  const cover = () => {
    for (const one of inputs) {
      const outer = inputs.find((o) => o !== one && o.input.checked && !o.input.disabled
        && one.input.value.startsWith(`${o.input.value}/`));
      one.input.disabled = Boolean(outer);
      if (outer) one.input.checked = true;
      one.meta.textContent = outer ? `Backed up with ${outer.input.value}` : one.facts;
    }
  };
  // At once, as every switch in Qurb is: there is no Save to miss. On the
  // owner's first try (2026-10-14) Save sat below a long list's edge, and the
  // folder he ticked was never chosen.
  const says = el("p", "says warn hidden");
  inputs.forEach(({ input }) => input.addEventListener("change", async () => {
    cover();
    says.classList.add("hidden");
    try {
      await invoke("choose_phone_folder", { phone: d.fingerprint, folder: input.value, chosen: input.checked });
      toast(input.checked
        ? `${input.value} is backed up from the next time ${d.name} syncs`
        : `${input.value} backs up nothing new`);
    } catch (e) {
      input.checked = !input.checked;
      cover();
      says.textContent = String(e);
      says.classList.remove("hidden");
    }
  }));
  cover();
  box.append(says, picks);
  box.append(el("p", "note", "Unticked, a folder backs up nothing new. What is kept here already stays."));
}

/** What this computer keeps of one of a phone's folders, each file openable. */
async function openBackup(d, folder) {
  const box = sheet({ wide: true });
  box.append(el("h2", null, folder));
  box.append(el("p", "lead", `Kept here from ${d.name}, including files deleted on the phone.`));
  const label = el("div", "list-label", "Files");
  const list = el("ul", "rows");
  box.append(label, list);
  let files;
  try {
    files = await invoke("phone_backup", { phone: d.fingerprint, folder });
  } catch (e) {
    oops(list, e);
    return;
  }
  label.textContent = count(files.length, "file");
  if (files.length === 0) {
    list.append(empty("hard-drive", "Nothing kept here yet. It comes over Wi-Fi.", null, true));
    return;
  }
  for (const f of files.slice(0, 500)) {
    list.append(row({
      iconName: KIND_ICON[kindOf(f.path)],
      name: base(f.path),
      sub: [folderOf(f.path) || folder, size(f.size), when(f.modified_at)],
      onClick: async () => {
        try {
          await invoke("open_backed_up", { phone: d.fingerprint, folder, path: f.path });
        } catch (e) {
          toast(String(e), true);
        }
      },
    }));
  }
  if (files.length > 500) box.append(el("p", "note", `The first 500 of ${count(files.length, "file")}.`));
}

/**
 * A guest's folder at this computer (decision 0060). Sealed on their phone,
 * so nothing here can open it: the window asks their phone, their person
 * approves behind a fingerprint, face or screen lock, and the folder opens
 * here until it is locked -- by hand, after ten minutes unused, or when Qurb
 * quits.
 */
function openGuestFolder(d) {
  let polling = null;
  const box = sheet({
    wide: true,
    onClose: () => { clearInterval(polling); },
  });
  box.append(el("h2", null, `Folder kept for ${d.name}`));
  const lead = el("p", "lead");
  const body = el("div");
  const actions = el("div", "actions end");
  box.append(lead, body, actions);

  // What is drawn, so the watch below redraws only when that changes.
  let shown = null;

  // A look every 1.5 s, which reads nothing and does not count as using the
  // folder: an open folder left on screen still locks itself when idle.
  async function watch() {
    try {
      const { state } = await invoke("guest_folder", { guest: d.fingerprint, list: false });
      if (state !== shown) draw();
    } catch (e) { /* drawn on the next change */ }
  }

  async function draw() {
    let folder;
    try {
      folder = await invoke("guest_folder", { guest: d.fingerprint, list: true });
    } catch (e) {
      lead.textContent = String(e);
      return;
    }
    shown = folder.state;
    body.replaceChildren();
    actions.replaceChildren();
    if (folder.state === "locked") {
      lead.textContent = `Kept here sealed: this computer can't open it. To open it here, ask ${d.name}.`;
      const ask = button(`Ask ${d.name}'s phone`, "btn primary", "lock-keyhole");
      ask.addEventListener("click", async () => {
        try { await invoke("ask_guest_folder", { guest: d.fingerprint }); } catch (e) { lead.textContent = String(e); return; }
        draw();
      });
      actions.append(ask);
      return;
    }
    if (folder.state === "asking") {
      lead.textContent = `Waiting for ${d.name}. Open Qurb on their phone and approve — it asks for their fingerprint, face or screen lock.`;
      const stop = button("Stop asking", "btn");
      stop.addEventListener("click", async () => {
        try { await invoke("lock_guest_folder", { guest: d.fingerprint }); } catch (e) { /* already */ }
        draw();
      });
      actions.append(stop);
      return;
    }
    lead.textContent = "Open here. Anyone at this computer can see these files until you lock it. It locks itself after ten minutes unused.";
    const list = el("ul", "rows");
    for (const f of folder.files) {
      list.append(row({
        iconName: KIND_ICON[kindOf(f.path)],
        name: f.path,
        sub: [size(f.size), when(f.modified_at)],
        onClick: async () => {
          try { await invoke("open_guest_file", { guest: d.fingerprint, path: f.path }); } catch (e) { lead.textContent = String(e); }
        },
      }));
    }
    if (folder.files.length === 0) list.append(el("li", "meta", "Nothing in it."));
    body.append(list);
    const lock = button("Lock", "btn primary", "lock-keyhole");
    lock.addEventListener("click", async () => {
      try { await invoke("lock_guest_folder", { guest: d.fingerprint }); } catch (e) { /* already */ }
      draw();
    });
    actions.append(lock);
  }
  draw();
  polling = setInterval(watch, 1500);
}

/**
 * Ask before removing a device, saying exactly what it does (brief §35):
 * trust ends here; nothing on the device itself is touched; and whatever this
 * computer can no longer get back because of it, named.
 */
async function askToRemove(device, where, called = null) {
  let plan;
  try {
    plan = await invoke("removal_plan", { device: device.fingerprint });
  } catch (e) {
    toast(String(e), true);
    return;
  }
  removing = device.fingerprint;
  where.querySelector(".confirm")?.remove();

  const box = el("div", "confirm card glass-frosted");
  box.style.marginTop = "16px";
  // Named as the person would tell it apart: the earlier self of a device set
  // up again has the same name as the device in front of them.
  box.append(el("strong", null, `Remove ${called ?? plan.name}?`));
  const say = (text, cls = "quiet") => {
    const p = el("p", cls, text);
    p.style.marginTop = "8px";
    p.style.fontSize = "13.5px";
    box.append(p);
  };
  say(`This computer stops trusting it: it can no longer connect or sync here. It keeps ` +
    `its key and everything already on it — removing it deletes nothing there.`);
  if (plan.waiting > 0) say(`${count(plan.waiting, "file")} waiting for it to collect will be cancelled.`);
  if (plan.holds_ours) say("It keeps this computer's private files. After this, it won't.");
  if (plan.only_there.length > 0) {
    const n = plan.only_there.length;
    say(`${count(n, "file")} freed from this computer ${n === 1 ? "is" : "are"} kept only on ` +
      `${plan.name}. Once it's removed, ${n === 1 ? "it" : "they"} can't be downloaded again.`, "warn");
    const fetchFirst = button("Keep them here first", "btn small", "download");
    fetchFirst.addEventListener("click", async () => {
      for (const path of plan.only_there) {
        try { await invoke("fetch", { path }); wanted.add(path); } catch (e) { /* in qurbFailures */ }
      }
      fetchFirst.replaceWith(el("p", "quiet", "Asked for. They come back the next time it's reachable; remove it after that."));
    });
    box.append(fetchFirst);
  }
  let deleteKept = null;
  if (plan.kept > 0) {
    const label = el("label");
    label.style.cssText = "display:flex;gap:8px;align-items:center;margin-top:10px;font-size:13.5px";
    deleteKept = el("input");
    deleteKept.type = "checkbox";
    label.append(deleteKept, `Also delete the ${count(plan.kept, "file")} (${size(plan.kept_bytes)}) this computer keeps for it`);
    box.append(label);
  }
  say("Only on this computer: your other devices go on trusting it until you remove it there too.", "meta");

  const buttons = el("div", "actions end");
  const cancel = button("Cancel", "btn");
  const confirm = button("Remove device", "btn danger");
  cancel.addEventListener("click", () => { removing = null; box.remove(); });
  confirm.addEventListener("click", async () => {
    confirm.disabled = true;
    try {
      await invoke("remove_device", { device: device.fingerprint, deleteKept: deleteKept?.checked ?? false });
    } catch (e) {
      confirm.disabled = false;
      say(String(e), "warn");
      return;
    }
    removing = null;
    closePanel();
    toast(`Removed ${plan.name}`);
    drawDevices();
  });
  buttons.append(cancel, confirm);
  box.append(buttons);
  where.append(box);
  // The panel's body scrolls, never the panel: scrolling the panel itself
  // would slide the body up under its heading.
  where.scrollTo({ top: where.scrollHeight, behavior: calm() ? "auto" : "smooth" });
}

// -------------------------------------------------------------- adding one

let watching = null;

/**
 * Add a device: show a code for it to scan or type, or enter the code it is
 * showing. The QR is the way most people will do it; the text is for a
 * device with no camera, and for reading out.
 */
function openAddDevice() {
  const box = sheet({
    onClose: async () => {
      clearInterval(watching);
      watching = null;
      // Stopped at both ends: off the screen, and no longer answered. A
      // closed code that still worked would be the opposite of what was asked.
      try { await invoke("stop_pairing"); } catch (e) { /* already gone */ }
      drawDevices();
    },
  });
  const body = el("div");
  box.append(body);
  choose();

  function choose() {
    clearInterval(watching);
    body.replaceChildren(el("h2", null, "Add a device"), el("p", "lead", "Your phone or another computer. It joins with a code, once."));
    const choices = el("div", "choices");
    const option = (id, iconName, title, words, act) => {
      const c = el("button", "choice");
      c.id = id;
      const t = el("span", "tile");
      t.append(icon(iconName));
      const text = el("span");
      text.append(el("strong", null, title), el("span", null, words));
      c.append(t, text, icon("chevron-right"));
      c.addEventListener("click", act);
      choices.append(c);
    };
    option("pair-show", "qr-code", "Show a code", "Scan it with your phone, or type its six digits there.", () => show(false));
    option("pair-enter", "keyboard", "Enter a code", "The six digits a device on this network is showing.", enter);
    // Another person, as a guest of this computer (decision 0060).
    option("pair-guest", "user-plus", "Add a person", "Someone else's phone, as a guest. It keeps its own key and sees only what you send it.", () => show(true));
    body.append(choices);
  }

  async function show(guest) {
    body.replaceChildren(el("h2", null, guest ? "Scan or type this on their phone" : "Scan or type this on the other device"));
    if (guest) body.append(el("p", "lead", "In Qurb on their phone: Devices → Add → Visit a computer."));
    const says = el("p", "countdown", "Opening a port…");
    body.append(says);
    let invitation;
    try {
      invitation = await invoke("start_pairing", { guest });
    } catch (e) {
      body.replaceChildren(el("h2", null, "Couldn't show a code"), el("p", "says warn", String(e)));
      const back = button("Back", "btn");
      back.addEventListener("click", choose);
      const actions = el("div", "actions end");
      actions.append(back);
      body.append(actions);
      return;
    }
    // The SVG comes from our own renderer, not from anything a peer sent, and
    // the only variable in it is the code this device just made. Hidden when
    // there is none: a blank square reads as a code still loading.
    const qr = el("div", "qr");
    qr.id = "qr";
    // With no code drawn, the countdown it captions starts on the left edge
    // under the heading, rather than centred under nothing.
    if (invitation.qr) qr.innerHTML = invitation.qr;
    else {
      qr.classList.add("hidden");
      if (!invitation.digits) says.style.textAlign = "left";
    }
    body.insertBefore(qr, says);
    // Six digits, typed on a device on the same network (decision 0066):
    // what the owner asked for in place of the long code, as WhatsApp's.
    const digitsSay = el("p", "meta digits-say", guest
      ? "Or type these on their phone, on this Wi-Fi"
      : "Or type these on your phone, on this Wi-Fi");
    const digits = el("p", "digits num", invitation.digits ?? "");
    digits.id = "pair-digits";
    if (!invitation.digits) { digitsSay.classList.add("hidden"); digits.classList.add("hidden"); }
    body.insertBefore(digitsSay, says);
    body.insertBefore(digits, says);
    const code = el("details");
    code.append(el("summary", "meta", "Type the whole code or read it out instead"));
    code.append(el("p", "meta", guest ? "On their computer:" : "On another computer:"));
    const typed = el("pre", "code", guest ? `qurb visit ${invitation.code}` : `qurb join <folder> ${invitation.code}`);
    typed.id = "pair-code";
    code.append(typed, el("p", "meta", "Or read this out:"));
    const spoken = el("pre", "code", invitation.spoken);
    spoken.id = "pair-spoken";
    code.append(spoken);
    code.style.marginTop = "12px";
    body.append(code);
    body.append(el("p", "note", "The code carries this computer's identity, which is why it travels outside the network; the six digits fetch it, for whoever knows them. It works once."));
    const actions = el("div", "actions end");
    const stop = button("Stop showing it", "btn");
    stop.addEventListener("click", async () => {
      clearInterval(watching);
      try { await invoke("stop_pairing"); } catch (e) { /* already gone */ }
      choose();
    });
    actions.append(stop);
    body.append(actions);

    // A device that used the code waits for the person here (decision 0053):
    // who it is, the number it should be showing, and a yes or no. A device
    // showing a different number has somebody else's copy of the code.
    const asking = el("div", "asking glass-frosted hidden");
    asking.id = "pair-asking";
    asking.style.cssText = "margin-top: 16px; padding: 16px; border-radius: 16px";
    body.insertBefore(asking, code);
    let asked = null;
    const ask = (state) => {
      const key = `${state.name}|${state.number}`;
      if (asked === key) return;
      asked = key;
      const what = state.guest
        ? "wants to visit this computer as a guest, with their own key, seeing only what you send them"
        : state.wants_key ? "wants to join and take this computer's key" : "wants to pair";
      const number = el("p", "number", state.number);
      number.id = "pair-number";
      number.style.cssText = "font-size: 32px; font-weight: 600; letter-spacing: 2px; margin: 8px 0; font-variant-numeric: tabular-nums";
      const decline = button("Decline", "btn");
      const approve = button("Approve", "btn primary");
      approve.id = "pair-approve";
      const answer = async (yes) => {
        decline.disabled = approve.disabled = true;
        try { await invoke("answer_pairing", { approve: yes }); } catch (e) { says.textContent = String(e); }
        asked = null;
        asking.classList.add("hidden");
      };
      decline.addEventListener("click", () => answer(false));
      approve.addEventListener("click", () => answer(true));
      const buttons = el("div", "actions end");
      buttons.append(decline, approve);
      asking.replaceChildren(
        el("strong", null, `${state.name} ${what}.`),
        el("p", "meta", "Approve only if it shows this number:"),
        number,
        el("p", "meta", "A different number means someone else has this code: decline."),
        buttons,
      );
      asking.classList.remove("hidden");
    };

    clearInterval(watching);
    const follow = async () => {
      let state;
      try {
        state = await invoke("pairing_state");
      } catch (e) {
        says.textContent = String(e);
        return;
      }
      if (state.state === "asking") {
        says.textContent = state.guest ? "Someone is asking to visit." : "A device is asking to join.";
        ask(state);
        return;
      }
      if (state.state === "waiting") {
        if (asked !== null) { asked = null; asking.classList.add("hidden"); }
        // Tried wrongly too often, the digits stop working; the QR still does.
        if (state.digits_withdrawn && !digits.classList.contains("withdrawn")) {
          digits.classList.add("withdrawn");
          digitsSay.textContent = "These were typed wrongly too often. Scan the code instead.";
        }
        // Counted down rather than left saying "waiting". A code that stopped
        // working minutes ago, under a screen that says it is waiting, is
        // worse than no screen: somebody reads it out and is told it is wrong.
        const left = Math.max(0, invitation.expires_at - Math.floor(Date.now() / 1000));
        says.textContent = `Waiting — this code works for ${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
        return;
      }
      clearInterval(watching);
      watching = null;
      if (state.state === "paired") done(state);
      else if (state.state === "expired") says.textContent = "That code has expired. Show a new one.";
      else if (state.state === "failed") says.textContent = state.message ?? "Pairing didn't work.";
    };
    watching = setInterval(follow, 700);
    follow();
  }

  function enter() {
    body.replaceChildren(el("h2", null, "Enter a code"), el("p", "lead", "The six digits the other device is showing, on the same network. The whole code works too."));
    const input = el("input", "input digits-input num");
    input.id = "pair-input";
    input.spellcheck = false;
    input.autocomplete = "off";
    input.inputMode = "numeric";
    input.placeholder = "000000";
    input.style.marginTop = "16px";
    const says = el("p", "says warn hidden");
    const actions = el("div", "actions end");
    const back = button("Back", "btn");
    back.addEventListener("click", choose);
    const go = button("Join", "btn primary");
    go.id = "pair-go";
    const join = async () => {
      if (go.disabled) return;
      go.disabled = true;
      go.querySelector("span").textContent = "Joining…";
      try {
        // Six digits are looked for on this network first (decision 0066).
        says.classList.remove("warn", "hidden");
        says.textContent = "Looking for it on this network…";
        // The other device asks its person to approve this one: show the
        // number it will be comparing (decision 0053).
        const number = await invoke("pairing_number", { code: input.value });
        says.classList.remove("warn", "hidden");
        says.textContent = `Approve it on the other device. It should show ${number}.`;
        const state = await invoke("join_device", { code: input.value });
        input.value = "";
        done(state);
      } catch (e) {
        says.textContent = String(e);
        says.classList.add("warn");
        says.classList.remove("hidden");
        go.disabled = false;
        go.querySelector("span").textContent = "Join";
      }
    };
    go.addEventListener("click", join);
    input.addEventListener("keydown", (event) => { if (event.key === "Enter") join(); });
    // Joining starts as the sixth digit is typed, as on the phone.
    input.addEventListener("input", () => { if (/^\d{6}$/.test(input.value.trim())) join(); });
    actions.append(back, go);
    body.append(input, says, actions);
    input.focus();
  }

  // §34: the new device materialises -- on this sheet, and again as its card
  // appears in the list behind it.
  function done(state) {
    justPaired = state.fingerprint;
    body.replaceChildren();
    const card = el("div", "device glass-frosted arrived");
    card.id = "pair-done";
    card.style.cssText = "margin: 8px auto 0; max-width: 280px; align-items: center; text-align: center";
    const t = el("span", "tile");
    t.append(icon(deviceIcon(state.name, state.kind)));
    const words = el("div");
    const named = el("strong", null, state.name);
    named.id = "pair-with";
    named.dataset.fingerprint = state.fingerprint;
    words.append(named, el("span", "presence on", "Connected to your Qurb space"));
    card.append(t, words);
    body.append(el("h2", null, "Device added"), card);
    const actions = el("div", "actions end");
    const finish = button("Done", "btn primary");
    finish.id = "pair-finish";
    finish.addEventListener("click", () => box.close());
    actions.append(finish);
    body.append(actions);
    drawDevices();
  }
}
