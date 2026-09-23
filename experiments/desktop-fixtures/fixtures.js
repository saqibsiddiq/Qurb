// Made-up answers, in place of the engine.
//
// Loaded before `app.js`, which reads `window.__TAURI__` at the top of the
// module and would otherwise fail on the first line.

const now = Math.floor(Date.now() / 1000);

const FILES = [
  { path: "notes.txt", size: "1840", updated_at: now - 120, availability: "here" },
  { path: "photos/cat.png", size: "250112", updated_at: now - 4000, availability: "here" },
  { path: "photos/holiday/beach.jpg", size: "900000", updated_at: now - 90000, availability: "only here" },
  { path: "photos/holiday/sunset.jpg", size: "1300000", updated_at: now - 90200, availability: "not here" },
  { path: "work/report.pdf", size: "4000000", updated_at: now - 300, availability: "here" },
  { path: "work/archive.tar.gz", size: "912000000", updated_at: now - 900000, availability: "not here" },
];

// Flip this to see the setting-up screens instead of the running window.
const SETTING_UP = new URLSearchParams(location.search).has("setup");

const WORDS = [
  "wheel", "push", "industry", "gospel", "vault", "canyon", "ribbon", "plastic",
  "orbit", "salmon", "fabric", "gentle", "meadow", "kitten", "bronze", "puzzle",
  "silent", "harvest", "copper", "lantern", "marble", "thunder", "velvet", "orchid",
];

let startedPairingAt = 0;

const ANSWERS = {
  situation: () => ({
    set_up: !SETTING_UP,
    running: !SETTING_UP,
    root: "/home/saqib/Sync",
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
  shown_phrase: () => WORDS,
  // Any answer is accepted here; the real one checks against the phrase the
  // session is holding, which a fixture has no way to be.
  confirm_phrase: () => true,
  enrol_device: () => null,
  reveal_phrase: () => WORDS,

  settings: () => ({
    name: "laptop",
    signal: "wss://rendezvous.example:9000",
    relay: null,
    port: 0,
    protection: "file",
    root: "/home/saqib/Sync",
    identity: "cfe05b03",
  }),

  save_settings: () => null,

  // Pairing. The code is the shape of a real one and is not a real one; the
  // QR is generated here rather than by the Rust renderer, so it encodes the
  // fixture string and nothing else.
  start_pairing: () => ({
    code: "qurb1-" + "k7fq".repeat(25),
    spoken: "kilo seven foxtrot quebec · romeo two delta · sierra nine whiskey",
    expires_at: now + 300,
    qr: null,
  }),

  // Answers "waiting" for a few seconds and then "paired", so the countdown
  // and the arrival can both be looked at without a second device.
  pairing_state: () => {
    const since = Math.floor(Date.now() / 1000) - startedPairingAt;
    if (since < 6) return { state: "waiting", name: null, fingerprint: null, message: null };
    return { state: "paired", name: "phone", fingerprint: "a1b2c3d4", message: null };
  },

  stop_pairing: () => null,
  join_device: () => ({ state: "paired", name: "phone", fingerprint: "a1b2c3d4", message: null }),
  summary: () => ({
    state: "syncing",
    root: "/home/saqib/Sync",
    identity: "cfe05b03",
    peers: 2,
    peers_reachable: 1,
    last_sync: now - 40,
    problem: null,
    recent: [
      { path: "work/report.pdf", at: now - 300, from_peer: false },
      { path: "photos/cat.png", at: now - 4000, from_peer: true },
    ],
  }),

  storage: () => ({
    files: "5400000000",
    chunks: "912000000",
    used: "6312000000",
    limit: "10737418240",
    disk: "494384795648",
    over: false,
    file_count: 6,
    evicted: 2,
    only_here: 1,
  }),

  files: ({ offset = 0, limit = 100 }) => FILES.slice(offset, offset + limit),

  // One deliberate failure, to see what a rejected command looks like on the
  // screen rather than only in a console.
  search: ({ text }) => {
    if (text === "boom") throw new Error("the index could not be read");
    return FILES.filter((f) => f.path.toLowerCase().includes(text.toLowerCase()));
  },

  devices: () => [
    { id: "4cef0d89", name: "phone", fingerprint: "a1b2c3d4", paired_at: now - 900000, last_seen: now - 300 },
    { id: "77b10e2a", name: "spare laptop", fingerprint: "e5f60718", paired_at: now - 3600, last_seen: null },
  ],

  activity: ({ before }) => {
    if (before) return [];
    return [
      { id: 9, at: now - 300, kind: "stored", path: "work/report.pdf", size: "4000000", device: null, detail: null },
      { id: 8, at: now - 4000, kind: "received", path: "photos/cat.png", size: "250112", device: "phone", detail: null },
      { id: 7, at: now - 5000, kind: "sent", path: "tickets.pdf", size: "700416", device: "phone", detail: null },
      { id: 6, at: now - 6000, kind: "collected", path: "tickets.pdf", size: null, device: "phone", detail: null },
      { id: 5, at: now - 90000, kind: "evicted", path: "photos/holiday/sunset.jpg", size: "1300000", device: null,
        detail: "dropped to stay under the storage limit; `qurb fetch` brings it back" },
      { id: 4, at: now - 91000, kind: "conflicted", path: "work/plan.md", size: null, device: "phone",
        detail: "the other version was kept as work/plan.conflict-4cef0d89-2026-09-22-141005.md" },
      { id: 3, at: now - 95000, kind: "failed", path: "work/locked.docx", size: null, device: null,
        detail: "permission denied" },
      { id: 2, at: now - 900000, kind: "paired", path: null, size: null, device: "phone", detail: "phone" },
      { id: 1, at: now - 900100, kind: "deleted", path: "old/scratch.txt", size: null, device: null, detail: null },
    ];
  },

  outgoing: () => [
    { path: "tickets.pdf", size: "700416", to: "phone" },
  ],

  fetch: () => true,
  set_limit: () => null,
};

window.__TAURI__ = {
  core: {
    invoke: async (name, args = {}) => {
      if (name === "start_pairing") startedPairingAt = Math.floor(Date.now() / 1000);
      const answer = ANSWERS[name];
      if (!answer) throw new Error(`no fixture for ${name}`);
      // A promise, like the real thing, so anything that depends on the call
      // not being synchronous behaves the same here.
      await new Promise((r) => setTimeout(r, 15));
      return answer(args);
    },
  },
};
