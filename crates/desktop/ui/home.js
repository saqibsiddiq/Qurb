// Home, Activity, conflicts and transfers.
//
// Home answers one question -- is my Qurb space okay? -- and in the healthy
// state it should be almost boring (direction §5): the state, one action, a
// line of secondary facts, attention only when something needs a decision,
// and a little that is recent.

/** The last state drawn, to notice the moment everything becomes synced. */
let lastState = null;

/** What the hero says for a given live state (§4: what is happening, does
 *  anyone need to do anything, what can they do next). */
function heroFor(s, st, conflicts) {
  if (s.peers === 0) {
    return {
      mark: "away", icon: "monitor-smartphone",
      title: "Add your first device",
      says: "Qurb keeps your files on devices you own. Connect your phone or another computer to begin.",
      action: ["Add a device", "plus", () => openAddDevice()],
    };
  }
  const send = ["Send to device", "send", () => openSend()];
  switch (s.state) {
    case "up to date":
      return {
        mark: "", icon: "check",
        title: "Everything is synced.",
        says: conflicts > 0
          ? `Your files are safe. ${conflicts === 1 ? "One thing needs" : `${conflicts} things need`} your attention.`
          : "Your files are safe. Nothing needs your attention.",
        action: send,
      };
    case "syncing": {
      const n = s.incoming.length;
      return {
        mark: "syncing", icon: "arrow-up-down",
        title: "Syncing…",
        says: n > 0
          ? `Receiving ${count(n, "file")} from ${s.incoming[0].device}.`
          : "Bringing your devices up to date.",
        action: send,
      };
    }
    case "no devices reachable": {
      const alone = st && st.only_here > 0
        ? ` ${count(st.only_here, "file")} ${st.only_here === 1 ? "is" : "are"} only on this computer until then.`
        : "";
      return {
        mark: "away", icon: "cloud-off",
        title: "Your devices are away.",
        says: `Changes here reach them the next time one is online.${alone}`,
        action: send,
      };
    }
    case "needs attention":
      return {
        mark: "attention", icon: "triangle-alert",
        title: "Something needs attention.",
        says: s.problem ?? "Qurb couldn't finish something. Activity says what.",
        action: ["Open activity", "history", () => showScreen("activity")],
      };
    default:
      return {
        mark: "syncing", icon: "refresh-cw",
        title: "Getting ready…",
        says: "Catching up with your folder.",
        action: send,
      };
  }
}

/** How many conflicts there are, asked on Home's slower beat: finding them
 *  reads every path. */
let conflictCount = 0;
let conflictsSeen = [];

async function drawHome() {
  let s;
  try {
    s = await invoke("summary");
  } catch (e) {
    setHero({ mark: "error", icon: "circle-alert", title: "Qurb can't read its state", says: String(e) });
    return;
  }
  let st = null;
  try { st = await invoke("storage"); } catch (e) { /* the facts line goes without */ }

  const hero = heroFor(s, st, conflictCount);
  setHero(hero);

  // §33: the moment everything becomes synced, a light passes once through
  // the environment. Not on opening the window to an already-synced state:
  // that is not an event.
  if (s.state === "up to date" && lastState && lastState !== "up to date") settle();
  lastState = s.state;

  const facts = [];
  if (st) facts.push(`${size(st.used)} used`);
  if (s.peers > 0) {
    facts.push(s.peers_reachable === s.peers
      ? `${count(s.peers, "device")} connected`
      : `${s.peers_reachable} of ${count(s.peers, "device")} connected`);
  }
  $("home-facts").textContent = facts.join(" · ");

  drawRecent(s.recent);
}

function setHero({ mark, icon: iconName, title, says, action }) {
  const m = $("mark");
  m.className = `mark ${mark ?? ""}`.trim();
  m.replaceChildren(icon(iconName));
  $("state").textContent = title;
  $("state-says").textContent = says ?? "";
  const b = $("home-action");
  if (action) {
    const [label, iconName2, act] = action;
    b.classList.remove("hidden");
    b.replaceChildren(icon(iconName2), el("span", null, label));
    b.onclick = act;
  } else {
    b.classList.add("hidden");
  }
}

function settle() {
  if (calm()) return;
  const env = $("environment");
  env.classList.remove("settled");
  void env.offsetWidth;
  env.classList.add("settled");
}

function drawRecent(recent) {
  const list = $("recent");
  list.replaceChildren();
  if (recent.length === 0) {
    list.append(empty("clock", "Nothing yet. Files you add or receive appear here.", null, true));
    return;
  }
  for (const r of recent.slice(0, 5)) {
    list.append(row({
      iconName: KIND_ICON[kindOf(r.path)],
      name: base(r.path),
      sub: [folderOf(r.path) || "Qurb", `${r.from_peer ? "Arrived" : "Saved here"} ${when(r.at)}`],
      onClick: () => openDetails(r.path),
    }));
  }
}

$("see-all").addEventListener("click", () => showScreen("activity"));

/** What needs a decision: conflicts, and only those (§5, §14). */
async function drawAttention() {
  const box = $("attention");
  let found;
  try {
    found = await invoke("conflicts");
  } catch (e) {
    box.replaceChildren();
    return;
  }
  conflictCount = found.length;
  conflictsSeen = found;
  box.replaceChildren();
  const filesBox = $("files-attention");
  filesBox.replaceChildren();
  if (found.length === 0) return;
  const title = found.length === 1
    ? `${base(found[0].path)} has two versions.`
    : `${found.length} files have two versions.`;
  for (const where of [box, filesBox]) {
    const review = button("Review", "btn small");
    review.addEventListener("click", () => openConflicts());
    where.append(attention({
      iconName: "git-compare",
      title: where === box ? `${found.length === 1 ? "1 thing needs" : `${found.length} things need`} attention` : title,
      says: where === box ? title : "Both versions are kept. Choose which you want.",
      action: review,
    }));
  }
}

// ------------------------------------------------------------ conflicts §14

async function openConflicts() {
  let found;
  try {
    found = await invoke("conflicts");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  const box = sheet({ wide: true, onClose: () => { drawAttention(); refreshScreen(); } });
  box.append(el("h2", null, found.length === 1 ? "Two versions" : `${found.length} files with two versions`));
  box.append(el("p", "lead", "Two devices changed these at the same time, so Qurb kept both. Nothing is lost whichever you choose."));
  if (found.length === 0) {
    box.append(empty("circle-check", "Nothing to review. Every file has one version."));
    return;
  }
  for (const c of found) box.append(conflictCard(c));
}

/** What a version looks like (brief §2): an image, or a text's start, read
 *  from the file here. Nothing for anything else. */
function preview(path) {
  const box = el("div", "preview");
  invoke("preview", { path }).then((p) => {
    if (p.kind === "image") {
      const img = el("img");
      img.alt = `${base(path)}, this version`;
      img.src = p.data;
      box.append(img);
    } else if (p.kind === "text") {
      box.append(el("pre", null, p.data));
    } else {
      box.remove();
    }
  }, () => box.remove());
  return box;
}

function conflictCard(c) {
  const card = el("div", "card glass-frosted");
  card.style.marginTop = "16px";
  const head = el("div");
  head.style.cssText = "display:flex;align-items:center;gap:12px";
  const tile = el("span", "tile");
  tile.style.cssText = "display:grid;place-items:center;width:40px;height:40px;border-radius:11px;background:rgba(var(--paper),.8);border:1px solid var(--hairline);color:var(--text-2)";
  tile.append(icon(KIND_ICON[kindOf(c.path)]));
  const names = el("div");
  names.append(el("strong", null, base(c.path)), el("p", "meta", folderOf(c.path) || "Qurb"));
  head.append(tile, names);
  card.append(head);

  const versions = el("div");
  versions.style.cssText = "display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:16px";
  const side = (label, v) => {
    const col = el("div");
    col.style.cssText = "padding:14px;border-radius:14px;background:rgba(var(--paper),.66);border:1px solid var(--hairline)";
    col.append(el("p", "meta", label));
    if (!v) {
      col.append(el("p", "quiet", "Since deleted or renamed"));
      return col;
    }
    const who = el("p");
    who.style.cssText = "display:flex;align-items:center;gap:8px;margin-top:6px;font-weight:500";
    who.append(icon(deviceIcon(v.by), "small"), el("span", null, v.by));
    col.append(who);
    col.append(el("p", "quiet num", `${when(v.at)} · ${size(v.size)}`));
    if (v.here) col.append(preview(v.path));
    if (!v.here) {
      const away = el("p", "state elsewhere");
      away.style.marginTop = "6px";
      away.append(icon("cloud"), el("span", null, "Not on this computer yet"));
      col.append(away);
    }
    return col;
  };
  versions.append(side("This version", c.this), side("The other version", c.other));
  card.append(versions);

  const actions = el("div", "actions");
  const choose = (label, keep, needsOther) => {
    const b = button(label, "btn small");
    b.disabled = needsOther && !c.other.here;
    if (b.disabled) b.title = "Its contents aren't on this computer yet";
    b.addEventListener("click", async () => {
      actions.querySelectorAll("button").forEach((x) => (x.disabled = true));
      try {
        const kept = await invoke("settle_conflict", { other: c.other.path, keep });
        const done = el("div", "attention");
        done.style.background = "var(--healthy-bg)";
        done.style.borderColor = "rgba(var(--healthy-ch),.16)";
        const t = el("span", "tile");
        t.style.color = "var(--healthy)";
        t.append(icon("circle-check"));
        const words = el("div");
        words.append(el("strong", null, keep === "both" ? "Kept both" : `Kept ${base(kept)}`));
        words.append(el("p", null, keep === "both"
          ? `The other version is now ${base(kept)}.`
          : "The other version is in Recently deleted for 30 days."));
        done.append(t, words, el("span"));
        card.replaceWith(done);
      } catch (e) {
        actions.querySelectorAll("button").forEach((x) => (x.disabled = false));
        card.append(el("p", "says warn", String(e)));
      }
    });
    return b;
  };
  actions.append(
    choose("Keep this version", "this", false),
    choose("Keep the other version", "other", true),
    choose("Keep both", "both", true),
  );
  card.append(actions);
  return card;
}

// -------------------------------------------------------------- activity

const HAPPENED = {
  stored: ["hard-drive", "Saved on this computer"],
  received: ["download", "Arrived"],
  deleted: ["trash-2", "Deleted"],
  sent: ["send", "Ready to send"],
  collected: ["circle-check", "Collected"],
  evicted: ["cloud-off", "Local space freed"],
  restored: ["rotate-ccw", "Restored"],
  conflicted: ["git-compare", "Changed on two devices"],
  paired: ["monitor-smartphone", "Paired"],
  failed: ["circle-alert", "Didn't finish"],
  cancelled: ["x", "Send cancelled"],
  removed: ["x", "Removed"],
};

/** One history entry as a row. */
function happenedRow(r) {
  const [iconName, words] = HAPPENED[r.kind] ?? ["info", r.kind];
  // Most rows are about a path. A pairing is about a device and has no path
  // at all, so the device becomes the subject rather than a note beside one.
  const subject = r.path ? base(r.path) : (r.device ?? "");
  const other = r.device && r.device !== subject ? r.device : null;
  const said = !other ? words
    : r.kind === "received" ? `${words} from ${other}`
    : r.kind === "collected" || r.kind === "sent" ? `${words} by ${other}`
    : r.kind === "conflicted" ? `Changed here and on ${other}`
    : `${words} · ${other}`;
  const li = row({
    iconName,
    name: subject,
    sub: [said, when(r.at), r.size ? size(r.size) : null],
    cls: r.kind === "failed" ? "failed" : "",
    onClick: r.path && r.kind !== "deleted" ? () => openDetails(r.path) : null,
  });
  if (r.kind === "failed") li.querySelector(".tile").style.color = "var(--error)";
  // `paired` stores the device name as its detail, which is already the
  // subject. Saying it twice reads as a mistake.
  if (r.detail && r.detail !== subject && r.detail !== r.device) li.title = r.detail;
  return li;
}

let oldest = null;

async function drawActivity(append = false) {
  const list = $("activity-list");
  if (!append) { oldest = null; list.replaceChildren(); }
  try {
    const rows = await invoke("activity", { path: null, limit: 60, before: oldest });
    if (rows.length === 0 && !append) {
      list.append(empty("history", "Nothing has happened yet."));
      $("older").classList.add("hidden");
      return;
    }
    oldest = rows.length ? rows[rows.length - 1].id : oldest;
    $("older").classList.toggle("hidden", rows.length < 60);
    for (const r of rows) list.append(happenedRow(r));
  } catch (e) {
    oops(list, e);
  }
}

$("older").addEventListener("click", () => drawActivity(true));

// -------------------------------------------------------------- transfers §21

/**
 * How long a send may go without the other device asking for more and still
 * be shown moving. A sender never hears that a transfer ended -- the other end
 * just stops asking -- so this is how it stops drawing a bar.
 */
const SEND_QUIET = 10;

/** What the chip last counted, and what is waiting to be collected. */
let waitingNow = [];
let movingNow = { incoming: [], outgoing: [] };

/** The chip in the top bar: there only while something moves or waits (§25). */
async function drawChip() {
  try {
    const [s, out] = await Promise.all([invoke("summary"), invoke("outgoing")]);
    const now = Date.now() / 1000;
    movingNow = {
      incoming: s.incoming,
      outgoing: s.outgoing.filter((t) => now - t.updated < SEND_QUIET),
    };
    waitingNow = out;
    const n = s.incoming.length + out.length;
    const chip = $("transfers-chip");
    chip.classList.toggle("hidden", n === 0 && !transfersOpen());
    $("transfers-count").textContent = String(n);
    $("transfers-word").textContent = n === 1 ? "transfer" : "transfers";
    chip.classList.toggle("flowing", s.incoming.length + movingNow.outgoing.length > 0);
    if (transfersOpen()) drawTransfers();
  } catch (e) {
    // Not set up, or the daemon is between states: nothing to show.
  }
}

const transfersOpen = () => openPanel?.classList.contains("transfers");

$("transfers-chip").addEventListener("click", () => {
  if (transfersOpen()) { closePanel(); return; }
  const box = panel("transfers");
  const head = el("div", "panel-head");
  const t = el("span", "tile");
  t.append(icon("arrow-up-down"));
  const words = el("div");
  words.append(el("h2", null, "Transfers"), el("p", "meta", "What you sent on purpose, and what was sent to you."));
  head.append(t, words);
  const body = el("div", "panel-body");
  body.id = "transfers-body";
  box.append(head, body);
  drawTransfers();
  drawFinished();
});

/** How far a transfer has got, as words and a bar with the Qurb light. */
function moving(t) {
  const done = Number(t.done), total = Number(t.size);
  // A rate from the whole transfer so far rather than the last moment:
  // steadier to read, and the first seconds of a connection are not typical
  // of the rest.
  const elapsed = Math.max(1, Date.now() / 1000 - t.started);
  const rate = done / elapsed;
  const percent = total > 0 ? Math.min(100, (100 * done) / total) : 0;
  let said = `${Math.round(percent)}%`;
  if (done > 0 && total > done) said += ` · about ${duration((total - done) / rate)} left`;
  const bar = el("div", "progress");
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  bar.setAttribute("aria-valuenow", String(Math.round(percent)));
  const fill = el("span");
  fill.style.width = `${percent}%`;
  bar.append(fill);
  return { said, bar };
}

/** The send a Cancel button failed on, kept across redraws for a while. */
let cancelFailed = null;

function drawTransfers() {
  const body = $("transfers-body");
  if (!body) return;
  const keepFinished = body.querySelector("#finished");
  body.replaceChildren();

  const active = [];
  for (const t of movingNow.incoming) active.push({ t, words: `Receiving from ${t.device}` });
  for (const t of movingNow.outgoing) active.push({ t, words: `Sending to ${t.device}` });

  body.append(el("div", "list-label", "Active"));
  if (active.length === 0) {
    body.append(el("p", "quiet", "No active transfers"));
  }
  for (const { t, words } of active) {
    const { said, bar } = moving(t);
    const li = row({ iconName: KIND_ICON[kindOf(t.path)], name: base(t.path), sub: [words, said] });
    li.querySelector(".main").append(bar);
    bar.style.marginTop = "8px";
    body.append(li);
  }

  if (waitingNow.length > 0) {
    body.append(el("div", "list-label", "Waiting to be collected"));
    for (const o of waitingNow) {
      const key = `${o.to_id}\u0000${o.path}`;
      const failed = cancelFailed?.key === key && Date.now() < cancelFailed.until;
      const stop = twoPress(button("Cancel", "btn small reveal"), "Stop sending?", async () => {
        try {
          await invoke("cancel_send", { path: o.path, to: o.to_id });
          toast(`Stopped sending ${base(o.path)}`);
        } catch (e) {
          // Collected in the meantime, most likely. Said where it happened.
          cancelFailed = { key, why: String(e), until: Date.now() + 10000 };
        }
        drawChip();
      });
      stop.setAttribute("aria-label", `Stop sending ${o.path} to ${o.to}`);
      body.append(row({
        iconName: KIND_ICON[kindOf(o.path)],
        name: base(o.path),
        sub: [failed ? cancelFailed.why : `Waiting for ${o.to}`, size(o.size)],
        trail: [failed ? null : stop],
      }));
    }
  }

  if (keepFinished) body.append(keepFinished);
}

/** The kinds of history that are a transfer somebody meant, rather than sync. */
function finishedLine(r) {
  const who = r.device ?? "another device";
  switch (r.kind) {
    case "received":
      return (r.detail ?? "").startsWith("sent to this device") ? `Received from ${who}` : null;
    // Not "sent": that is written when a send is queued, and a queued send is
    // under "Waiting to be collected" until it is finished.
    case "collected":
      return `Sent to ${who}`;
    case "cancelled":
      return `Taken back before ${who} collected it`;
    case "failed":
      return "Didn't finish";
    default:
      return null;
  }
}

/** Transfers somebody meant, that are over: arrived, collected, or failed. */
async function drawFinished() {
  const body = $("transfers-body");
  if (!body) return;
  const box = el("div");
  box.id = "finished";
  box.append(el("div", "list-label", "Done"));
  try {
    const rows = await invoke("activity", { path: null, limit: 200, before: null });
    let shown = 0;
    for (const r of rows) {
      const line = finishedLine(r);
      if (!line) continue;
      const trail = [];
      // A way to a file that went to Downloads. The entry is what is sent
      // back, never a path: which folder to open is looked up and checked on
      // the other side.
      if (r.kind === "received" && (r.detail ?? "").includes("; saved to ")) {
        const show = button("Show", "btn small reveal", "folder-search");
        show.setAttribute("aria-label", `Show ${r.path} in its folder`);
        show.addEventListener("click", async () => {
          try { await invoke("show_received", { id: r.id }); } catch (e) { toast(String(e), true); }
        });
        trail.push(show);
      }
      const li = row({
        iconName: r.kind === "failed" ? "circle-alert" : r.kind === "cancelled" ? "x" : "circle-check",
        name: base(r.path),
        sub: [line, when(r.at)],
        trail,
      });
      if (r.kind !== "failed" && r.kind !== "cancelled") li.querySelector(".tile").style.color = "var(--healthy)";
      box.append(li);
      if (++shown === 8) break;
    }
    if (shown === 0) box.append(el("p", "quiet", "Nothing yet"));
  } catch (e) {
    box.append(el("p", "says warn", String(e)));
  }
  body.querySelector("#finished")?.remove();
  body.append(box);
}
