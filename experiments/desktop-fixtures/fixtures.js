// Made-up answers, in place of the engine.
//
// Loaded before the window's own scripts, which read `window.__TAURI__` as
// they start and would otherwise fail on the first line.
//
// `?state=` picks the live state Home draws -- synced, syncing, away, alone
// (nothing paired), attention -- and `?screen=` opens a place directly, so
// each can be looked at without clicking through. `?quiet` answers with no
// conflicts and no transfers: the healthy state, the one that should look
// almost boring.

const now = Math.floor(Date.now() / 1000);
const params = new URLSearchParams(location.search);
const STATE = params.get("state") ?? "synced";
const QUIET = params.has("quiet") || STATE === "synced";

const FILES = [
  { path: "Notes.md", size: "1840", modified: now - 120, availability: "here", private: false },
  { path: "Project Plan.pdf", size: "2400000", modified: now - 300, availability: "here", private: false },
  { path: "Photos/cat.png", size: "250112", modified: now - 4000, availability: "here", private: false },
  { path: "Photos/Holiday/beach.jpg", size: "900000", modified: now - 90000, availability: "only here", private: false },
  { path: "Photos/Holiday/sunset.jpg", size: "1300000", modified: now - 90200, availability: "elsewhere", private: false },
  { path: "Work/Quarterly report.pdf", size: "4000000", modified: now - 300, availability: "here", private: false },
  { path: "Work/archive.tar.gz", size: "912000000", modified: now - 900000, availability: "elsewhere", private: false },
  { path: "Budget 2026.xlsx", size: "88000", modified: now - 7200, availability: "only here", private: false },
  { path: "Interview.m4a", size: "31000000", modified: now - 190000, availability: "elsewhere", private: false },
  { path: "Lost with an old phone.jpg", size: "2100000", modified: now - 900000, availability: "nowhere", private: false },
  { path: "Passport scan.pdf", size: "1200000", modified: now - 400000, availability: "here", private: true },
  { path: "Tax/2025 return.pdf", size: "640000", modified: now - 800000, availability: "only here", private: true },
];

// Flip this to see the setting-up screens instead of the running window.
const SETTING_UP = params.has("setup");

const WORDS = [
  "wheel", "push", "industry", "gospel", "vault", "canyon", "ribbon", "plastic",
  "orbit", "salmon", "fabric", "gentle", "meadow", "kitten", "bronze", "puzzle",
  "silent", "harvest", "copper", "lantern", "marble", "thunder", "velvet", "orchid",
];

let startedPairingAt = 0;
// The person's answer to a device asking to join: null until given.
let pairingAnswer = null;
// Whether the code on screen is a guest code (decision 0060).
let pairingGuest = false;

/** One directory, as `browse` answers it. */
function browse({ dir, private: priv }) {
  const prefix = dir ? `${dir}/` : "";
  const folders = new Set();
  const files = [];
  for (const f of FILES.filter((x) => x.private === priv && x.path.startsWith(prefix))) {
    const rest = f.path.slice(prefix.length);
    if (rest.includes("/")) folders.add(rest.split("/")[0]);
    else files.push(f);
  }
  return { folders: [...folders].sort(), files };
}

const STATES = {
  synced: { state: "up to date", peers: 2, peers_reachable: 2 },
  syncing: { state: "syncing", peers: 2, peers_reachable: 1 },
  away: { state: "no devices reachable", peers: 2, peers_reachable: 0 },
  alone: { state: "up to date", peers: 0, peers_reachable: 0 },
  attention: { state: "needs attention", peers: 2, peers_reachable: 1, problem: "Couldn't read Work/locked.docx: permission denied." },
};

const ANSWERS = {
  situation: () => ({
    set_up: !SETTING_UP,
    running: !SETTING_UP,
    root: "/home/saqib/qurb",
    locked: params.has("locked"),
    problem: null,
  }),

  inspect_folder: ({ path }) => ({
    path,
    exists: !path.endsWith("new"),
    set_up: path.endsWith("taken"),
    writable: !path.startsWith("/etc"),
    existing_files: path.endsWith("full") ? 2000 : 0,
    counted_all: !path.endsWith("full"),
    disk: "494384795648",
    free: "201326592000",
  }),

  create_device: () => null,
  // Numbers in GB only, which is all a person looking at the screen needs to
  // try; the real parser, with its units, is tested in `qurb-cli`.
  read_allowance: ({ text }) => {
    const gb = parseFloat(text);
    if (!(gb > 0)) throw new Error(`\`${text.trim()}\` is not an amount of space — try 75 or 1.5 TB`);
    return String(Math.round(gb * 1024 ** 3));
  },
  // Any code is taken here; the real one fetches the key from the device
  // showing it (decision 0052).
  join_new_device: () => "Galaxy S23",
  pairing_number: () => "482 913",
  setup_pairing_number: () => "482 913",
  answer_pairing: ({ approve }) => { pairingAnswer = approve; return null; },
  enrol_device: () => null,
  reveal_phrase: () => WORDS,
  unlock: ({ passphrase }) => {
    if (passphrase !== "open") throw new Error("that passphrase does not open this key");
    return null;
  },

  settings: () => ({
    name: "Saqib's laptop",
    signal: "wss://rendezvous.example:9000",
    relay: null,
    port: 0,
    protection: "file",
    root: "/home/saqib/qurb",
    identity: "cfe05b03",
    downloads: "",
    downloads_at: { Ok: "/home/saqib/Downloads/qurb" },
    version: "window 0.1.0 · engine 0.1.0 · protocol qurb/2 · index schema 15",
    own_files_private: false,
    notifications: true,
    key_also_on: ["Galaxy S23"],
    phone_holds_key: true,
  }),
  save_settings: () => null,
  set_privacy: () => null,
  set_notifications: () => null,
  starts_at_login: () => true,
  set_starts_at_login: () => null,
  quit: () => null,

  security: () => ({
    identity: "cfe05b03 9a1e 44d2",
    protection: "file",
    events: [
      { id: 2, at: now - 900000, kind: "paired", path: null, size: null, device: "Galaxy S23", detail: "Galaxy S23" },
      { id: 1, at: now - 3600, kind: "paired", path: null, size: null, device: "Study desktop", detail: "Study desktop" },
    ],
  }),
  protect_key: () => null,

  // Pairing. The code is the shape of a real one and is not a real one; the
  // QR is generated here rather than by the Rust renderer, so it encodes the
  // fixture string and nothing else.
  start_pairing: ({ guest } = {}) => ({
    code: (guest ? "qurbg1-" : "qurb1-") + "k7fq".repeat(25),
    spoken: "kilo seven foxtrot quebec · romeo two delta · sierra nine whiskey",
    expires_at: now + 300,
    // None, unless a script has put a real one from the Rust renderer in
    // `window.__fixtureQr` -- the way to look at the actual drawing here.
    qr: window.__fixtureQr ?? null,
  }),

  // Answers "waiting" for a few seconds, then a phone asks to join with the
  // number it shows (decision 0053) until approved -- "paired" -- or
  // declined, when the code waits again. The countdown, the question and the
  // arrival can all be looked at without a second device.
  pairing_state: () => {
    const since = Math.floor(Date.now() / 1000) - startedPairingAt;
    const waiting = { state: "waiting", name: null, fingerprint: null, message: null };
    if (since < 4 || pairingAnswer === false) return waiting;
    if (pairingAnswer === true) return { state: "paired", name: "Pixel 8", fingerprint: "a9b8c7d6", message: null };
    // A guest code is answered by somebody else's phone (decision 0060).
    if (pairingGuest) return { ...waiting, state: "asking", name: "Ammi's phone", number: "418 205", kind: "phone", wants_key: false, guest: true };
    return { ...waiting, state: "asking", name: "Pixel 8", number: "232 760", kind: "phone", wants_key: false, guest: false };
  },

  // A file called tickets.pdf went to the device three days ago, so the
  // question about sending it again can be looked at (decision 0059).
  sent_before: ({ paths }) => paths
    .filter((p) => p.endsWith("tickets.pdf"))
    .map((p) => ({ path: p, sent_as: "tickets.pdf", at: now - 3 * 86400 })),
  send_files: ({ paths, leaveOut = [] }) => ({
    sent: paths.filter((p) => !leaveOut.includes(p)).length,
    bytes: "5242880",
    only: paths.length === 1 ? paths[0].split("/").pop() : null,
    skipped: [],
  }),
  stop_pairing: () => null,
  join_device: () => ({ state: "paired", name: "Pixel 8", fingerprint: "a9b8c7d6", message: null }),

  summary: () => {
    const s = STATES[STATE] ?? STATES.synced;
    return {
      state: s.state,
      root: "/home/saqib/qurb",
      identity: "cfe05b03",
      peers: s.peers,
      peers_reachable: s.peers_reachable,
      last_sync: now - 40,
      problem: s.problem ?? null,
      recent: [
        { path: "Project Plan.pdf", at: now - 300, from_peer: false },
        { path: "Photos/cat.png", at: now - 4000, from_peer: true },
        { path: "Work/Quarterly report.pdf", at: now - 5400, from_peer: true },
        { path: "Notes.md", at: now - 9000, from_peer: false },
      ],
      // One transfer moving, growing on every poll, so the bar can be looked at.
      incoming: STATE === "syncing" ? [
        {
          path: "Videos/Holiday.mp4",
          device: "Galaxy S23",
          size: "4000000000",
          done: String(Math.min(4000000000, (Math.floor(Date.now() / 1000) - now) * 45000000 + 1200000000)),
          started: now - 30,
          updated: Math.floor(Date.now() / 1000),
        },
      ] : [],
      outgoing: [],
    };
  },

  storage: () => ({
    files: "5400000000",
    chunks: "912000000",
    used: "6312000000",
    limit: "10737418240",
    disk: "494384795648",
    free_disk: "201326592000",
    over: false,
    file_count: FILES.length,
    evicted: 3,
    only_here: 2,
  }),

  browse,
  find: ({ text, private: priv }) => {
    // One deliberate failure, to see what a rejected command looks like on
    // the screen rather than only in a console.
    if (text === "boom") throw new Error("the index could not be read");
    return FILES.filter((f) => f.private === priv && f.path.toLowerCase().includes(text.toLowerCase()));
  },
  details: ({ path }) => {
    const file = FILES.find((f) => f.path === path) ?? { path, size: "2400000", modified: now - 300, availability: "here", private: false };
    return {
      file,
      holders: ["only here", "nowhere"].includes(file.availability) ? [] : ["Galaxy S23"],
      history: [
        { id: 3, at: now - 300, kind: "stored", path, size: file.size, device: null, detail: null },
        { id: 2, at: now - 90000, kind: "received", path, size: file.size, device: "Galaxy S23", detail: null },
      ],
    };
  },
  move_file_area: ({ path, private: into }) => {
    const f = FILES.find((x) => x.path === path);
    if (!f || f.private === into) return false;
    f.private = into;
    return true;
  },
  // No device keeps this computer's vault in the fixture: the warning shows.
  vault_keepers: () => [],
  free_file: ({ path }) => {
    const f = FILES.find((x) => x.path === path);
    if (!f || f.availability === "only here") throw new Error(`cannot evict ${path}: no other device is known to hold this content`);
    f.availability = "elsewhere";
    return f.size;
  },
  freeable: () => {
    const files = FILES.filter((f) => f.availability === "here").sort((a, b) => Number(b.size) - Number(a.size));
    return { count: files.length, bytes: String(files.reduce((n, f) => n + Number(f.size), 0)), files };
  },
  delete_file: ({ path }) => { FILES.splice(FILES.findIndex((f) => f.path === path), 1); return null; },
  open_file: () => null,
  show_file: () => null,
  show_root: () => null,
  keep_locally: () => 4,
  keep_remotely: () => ({ freed: 12, bytes: "480000000", kept: [] }),

  devices: () => STATE === "alone" ? [] : [
    { id: "4cef0d89", name: "Galaxy S23", fingerprint: "a1b2c3d4", paired_at: now - 900000, last_seen: now - 300,
      route: STATE === "away" ? null : "direct", address: "192.168.1.2:57199" },
    { id: "77b10e2a", name: "Study desktop", fingerprint: "e5f60718", paired_at: now - 3600, last_seen: now - 7200,
      route: STATE === "synced" ? "relay" : null, address: null, relation: "own" },
    // Another person, visiting this computer (decision 0060).
    { id: "9d01c3aa", name: "Ammi's phone", fingerprint: "c0ffee12", paired_at: now - 86400, last_seen: now - 600,
      route: null, address: null, relation: "guest" },
  ],

  activity: ({ before }) => {
    if (before) return [];
    return [
      { id: 9, at: now - 300, kind: "stored", path: "Project Plan.pdf", size: "2400000", device: null, detail: null },
      { id: 8, at: now - 4000, kind: "received", path: "Photos/cat.png", size: "250112", device: "Galaxy S23", detail: null },
      { id: 7, at: now - 5000, kind: "received", path: "Boarding pass.pdf", size: "700416", device: "Galaxy S23",
        detail: "sent to this device; saved to /home/saqib/Downloads/qurb/Boarding pass.pdf" },
      { id: 6, at: now - 6000, kind: "collected", path: "Tickets.pdf", size: null, device: "Galaxy S23", detail: null },
      { id: 5, at: now - 90000, kind: "evicted", path: "Photos/Holiday/sunset.jpg", size: "1300000", device: null,
        detail: "local copy freed; another device keeps it" },
      { id: 4, at: now - 91000, kind: "conflicted", path: "Notes.md", size: null, device: "Galaxy S23",
        detail: "the other version was kept as Notes.conflict-4cef0d89-2026-09-22-141005.md" },
      { id: 3, at: now - 95000, kind: "failed", path: "Work/locked.docx", size: null, device: null,
        detail: "permission denied" },
      { id: 2, at: now - 900000, kind: "paired", path: null, size: null, device: "Galaxy S23", detail: "Galaxy S23" },
      { id: 1, at: now - 900100, kind: "deleted", path: "Old/scratch.txt", size: null, device: null, detail: null },
    ];
  },

  outgoing: () => QUIET ? [] : [
    { path: "Tickets.pdf", size: "700416", to: "Galaxy S23", to_id: "4cef0d89" },
  ],
  cancel_send: () => null,

  // Removing a device: the question has something in each part.
  removal_plan: ({ device }) => ({
    name: device === "a1b2c3d4" ? "Galaxy S23" : "Study desktop",
    waiting: 1,
    kept: 12,
    kept_bytes: String(340 * 1024 * 1024),
    only_there: ["Photos/IMG_0041.jpg", "Photos/IMG_0042.jpg"],
    holds_ours: true,
  }),
  remove_device: () => null,

  // One conflict, whose other version is here, so every choice is offered.
  conflicts: () => QUIET ? [] : [
    {
      path: "Project Plan.pdf",
      this: { path: "Project Plan.pdf", size: "2400000", here: true, by: "Saqib's laptop", at: now - 3600 },
      other: {
        path: "Project Plan.conflict-a1b2c3d4-2026-09-28-101502.pdf",
        size: "2380000", here: true, by: "Galaxy S23", at: now - 3500,
      },
    },
    {
      path: "Notes.md",
      this: { path: "Notes.md", size: "1840", here: true, by: "Saqib's laptop", at: now - 900 },
      other: { path: "Notes.conflict-a1b2c3d4-2026-10-08-091200.md", size: "1902", here: true, by: "Galaxy S23", at: now - 840 },
    },
    {
      path: "Photos/Holiday/beach.jpg",
      this: { path: "Photos/Holiday/beach.jpg", size: "900000", here: true, by: "Saqib's laptop", at: now - 7200 },
      other: { path: "Photos/Holiday/beach.conflict-a1b2c3d4-2026-10-08-071500.jpg", size: "880000", here: true, by: "Galaxy S23", at: now - 7000 },
    },
  ],
  // What each version looks like: a note's text, and for a photo a picture
  // drawn here, since a fixture has no files.
  preview: ({ path }) => {
    if (path.endsWith(".md")) {
      return { kind: "text", data: path.includes("conflict")
        ? "# Groceries\n\n- oat milk\n- lemons\n- coffee beans\n- bread (the seeded one)\n"
        : "# Groceries\n\n- oat milk\n- lemons\n- coffee\n- eggs\n- bread\n" };
    }
    if (path.endsWith(".jpg")) {
      const c = document.createElement("canvas");
      c.width = 320; c.height = 200;
      const g = c.getContext("2d");
      const sky = g.createLinearGradient(0, 0, 0, 200);
      sky.addColorStop(0, path.includes("conflict") ? "#f2b880" : "#8fc1e3");
      sky.addColorStop(1, "#f7e9c8");
      g.fillStyle = sky; g.fillRect(0, 0, 320, 200);
      g.fillStyle = "#2f6b57"; g.fillRect(0, 150, 320, 50);
      return { kind: "image", data: c.toDataURL("image/png") };
    }
    return { kind: "none", data: "" };
  },
  settle_conflict: ({ keep }) => (keep === "both" ? "Project Plan (Galaxy S23).pdf" : "Project Plan.pdf"),

  sharing: () => ({
    folders: [
      { folder: "Photos", everyone: false, members: ["aa", "bb"], remote: false },
      { folder: "Work", everyone: true, members: [], remote: true },
    ],
    devices: [
      { id: "aa", name: "Saqib's laptop", here: true },
      { id: "bb", name: "Galaxy S23", here: false },
      { id: "cc", name: "Study desktop", here: false },
    ],
  }),
  set_sharing: () => null,

  recently_deleted: () => [
    { id: 3, path: "Tax 2025.pdf", size: "184220", at: now - 3600, by: "this computer", why: null, private: true },
    { id: 2, path: "Photos/IMG_0007.jpg", size: "2311043", at: now - 7200, by: "Galaxy S23", why: null, private: false },
    {
      id: 1, path: "Notes.md", size: "3977", at: now - 90000 * 3, by: "this computer",
      why: "the version not kept when a conflict was settled", private: false,
    },
  ],
  restore_deleted: ({ id }) => ({ 1: "Notes.md", 2: "Photos/IMG_0007.jpg", 3: "Tax 2025.pdf" })[id],
  forget_deleted: () => null,
  open_downloads: () => null,
  show_received: () => null,

  fetch: () => true,
  set_limit: () => null,
};

window.__TAURI__ = {
  // The file chooser, and the window events a real drag-and-drop arrives on.
  // Both are Tauri's rather than the page's, so a fixture has to stand in for
  // them or sending is untouchable here.
  dialog: {
    open: async () => "/home/saqib/Downloads/holiday-photos.zip",
  },

  event: {
    listen: async (name, handler) => {
      // Exposed so the page can be driven from a console: calling
      // `window.__fixtureDrop("/some/path")` does what dropping a file does.
      if (name === "tauri://drag-drop") {
        window.__fixtureDrop = (path) => handler({ payload: { paths: [path] } });
      }
      return () => {};
    },
  },

  core: {
    invoke: async (name, args = {}) => {
      if (name === "start_pairing") { startedPairingAt = Math.floor(Date.now() / 1000); pairingAnswer = null; pairingGuest = !!args?.guest; }
      const answer = ANSWERS[name];
      if (!answer) throw new Error(`no fixture for ${name}`);
      // A promise, like the real thing, so anything that depends on the call
      // not being synchronous behaves the same here.
      await new Promise((r) => setTimeout(r, 15));
      return answer(args);
    },
  },
};

// `?screen=files` opens a place once the window is up. `?setup&at=storage`
// walks the setting-up screens as far as the storage question; `&custom=75`
// then types into its custom field; `&at=phrase` goes on to the 24 words.
(() => {
  const ready = setInterval(async () => {
    if (typeof lookAtFolder !== "function" || typeof showScreen !== "function") return;
    clearInterval(ready);
    const screen = params.get("screen");
    if (screen) setTimeout(() => showScreen(screen), 300);
    const at = params.get("at");
    if (!at) return;
    document.getElementById("choose-new").click();
    document.getElementById("folder-path").value = "/home/saqib/qurb";
    await lookAtFolder();
    document.getElementById("folder-next").click();
    const custom = params.get("custom");
    if (custom !== null) {
      [...document.querySelectorAll("#allowances button")].pop().click();
      const field = document.getElementById("custom-allowance");
      field.value = custom;
      field.dispatchEvent(new Event("input"));
    }
    if (at === "phrase") setTimeout(() => document.getElementById("storage-next").click(), 200);
  }, 50);
})();
