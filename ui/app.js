// The randomizer's page: what a seed moves, and the build beside it, laid out as
// Motion's is. The mod manager draws the title row (‹ Tools, the name, the
// version, ×); everything under it is this page.
import { request, wear, listen } from "./host.js";
import { workerCaller, until, outputs, Stopped } from "./core.js";
import { pieceOf } from "./pieces.js";
import * as L from "./logic.js";

const app = document.getElementById("app");
const OUTPUT = "Randomizer";
/** The game's own files, under its install folder. */
const IN_GAME = "Game/";
const MAX_PART = 64 * 1024 * 1024;

const store = {
  get(k, d) {
    try {
      const v = localStorage.getItem("randomizer." + k);
      return v == null ? d : JSON.parse(v);
    } catch {
      return d;
    }
  },
  set(k, v) {
    try {
      localStorage.setItem("randomizer." + k, JSON.stringify(v));
    } catch {
      /* no storage here: the setting lasts for this visit */
    }
  },
};

const S = {
  build: "idle", // idle | run | done | fail
  settings: { ...L.defaults(), ...store.get("settings", {}) },
  seedText: String(store.get("seed", L.newSeed())),
  last: store.get("last", null), // { at, ms, seed, files, paths }
  result: null, // { seed, slots, files, ms, spoiler }
  fail: null, // { message, ms }
  stop: false,
};

function h(tag, attrs = {}, ...kids) {
  const [name, ...cls] = tag.split(".");
  const el = document.createElement(name || "div");
  if (cls.length) el.className = cls.join(" ");
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "style") el.style.cssText = v;
    else if (k === "value") el.value = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids.flat()) if (c != null && c !== false) el.append(c instanceof Node ? c : String(c));
  return el;
}

const hostAsk = async (op, args) => {
  try {
    return await request(op, args);
  } catch {
    return null;
  }
};
const busy = (label, progress) => hostAsk("busy", label ? { label, progress } : {});

// ------------------------------------------------------------------ page
let statusEl, sideEl;

function status() {
  if (S.build === "run") return "Building";
  if (S.build === "fail") return "Build failed";
  if (S.build === "done") return `Seed ${S.result.seed} built`;
  return S.last ? `Seed ${S.last.seed} built ${L.ago(S.last.at).toLowerCase()}` : "No seed built";
}

function setStatus() {
  statusEl.replaceChildren(h("span.fadein", {}, status()));
}

const saveSettings = () => store.set("settings", S.settings);

function slider(label, value, set, tag) {
  const out = h("output", {}, `${value}%`);
  const input = h("input", {
    type: "range",
    min: "0",
    max: "100",
    step: "5",
    value: String(value),
    "aria-label": label,
    style: `--at:${value}%`,
    oninput: (e) => {
      const v = Number(e.target.value);
      e.target.style.setProperty("--at", `${v}%`);
      out.textContent = `${v}%`;
      set(v);
      saveSettings();
    },
  });
  return h("div.sr", {}, h("span.n", {}, label, tag), input, out);
}

function groups() {
  const st = S.settings;
  const seg = (options, now, set) => {
    const el = h(
      "div.seg",
      { role: "group", "aria-label": "Shadow of the Erdtree items" },
      options.map(([k, label]) =>
        h(
          "button" + (k === now ? ".on" : ""),
          {
            "aria-pressed": String(k === now),
            onclick: (e) => {
              el.querySelectorAll("button").forEach((b) => {
                b.classList.toggle("on", b === e.currentTarget);
                b.setAttribute("aria-pressed", String(b === e.currentTarget));
              });
              set(k);
              saveSettings();
            },
          },
          label,
        ),
      ),
    );
    return el;
  };
  return h(
    "div.groups",
    {},
    h(
      "section",
      {},
      h("div.sh", {}, h("span", {}, "Items"), h("span.x")),
      h(
        "div.card.rows",
        {},
        L.CATEGORIES.map(([k, label]) => slider(label, st.shuffle[k] ?? 100, (v) => (st.shuffle[k] = v))),
        h(
          "div.sr.pick",
          {},
          h("span.n", {}, "Shadow of the Erdtree"),
          seg(
            [
              ["mixed", "Mixed in"],
              ["separate", "Kept apart"],
            ],
            st.dlc,
            (v) => (st.dlc = v),
          ),
        ),
      ),
    ),
    h(
      "section",
      {},
      h("div.sh", {}, h("span", {}, "Enemies"), h("span.x")),
      h(
        "div.card.rows",
        {},
        slider("Bosses", st.bosses, (v) => (st.bosses = v), h("span.tag.warn", { title: "Not yet played through in the game" }, "Untested")),
        slider("Ordinary", st.ordinary ?? 0, (v) => (st.ordinary = v), h("span.tag.warn", { title: "Not yet played through in the game" }, "Untested")),
        slider("Boss mix", st.mix ?? 0, (v) => (st.mix = v), h("span.tag.warn", { title: "The share of bosses that trade places with an ordinary enemy. Needs Bosses above 0. Not yet played through in the game" }, "Untested")),
      ),
    ),
  );
}

// The build button, its fill showing how far: Motion's.
function bb(text, primary) {
  const fill = h("span.fill", { "aria-hidden": "true" }, h("i"));
  const lbl = h("span.lbl", {}, text);
  const btn = h("button.btn" + (primary ? ".pri" : ""), { onclick: () => build() }, lbl);
  const wrap = h("div.bbw", {}, btn, fill);
  wrap.set = ({ text: t, frac, show }) => {
    if (t != null) lbl.textContent = t;
    if (frac != null) fill.style.transform = `scaleX(${frac})`;
    if (show != null) fill.classList.toggle("on", show);
  };
  wrap.btn = btn;
  return wrap;
}

function seedField() {
  const input = h("input.inp", {
    value: S.seedText,
    inputmode: "numeric",
    maxlength: "15",
    "aria-label": "Seed",
    oninput: (e) => {
      S.seedText = e.target.value;
      store.set("seed", S.seedText);
    },
  });
  const another = h(
    "button.btn.sm",
    {
      onclick: () => {
        S.seedText = String(L.newSeed());
        input.value = S.seedText;
        store.set("seed", S.seedText);
      },
    },
    "New seed",
  );
  return h("label.fld", {}, h("span", {}, "Seed"), h("div.seedrow", {}, input, another));
}

let bar = null;
function side() {
  const kv = (pairs) => h("div.kv", {}, pairs.flatMap(([k, v]) => [h("span.k", {}, k), h("span.val", {}, v)]));
  let kids;
  if (S.build === "done" && S.result) {
    const r = S.result;
    kids = [
      h("div.res.risein", {}, h("span.big", {}, `${r.slots.toLocaleString()} items`), h("span.sec", {}, `Built in ${L.seconds(r.ms)}`)),
      kv([
        ["Output", OUTPUT],
        ["Seed", String(r.seed)],
        ["Status", h("span.accent.fadein", {}, "Apply to place")],
      ]),
      h("button.btn.sm", { onclick: saveSpoiler }, "Save spoiler log…"),
      h("div.pair", {}, h("button.btn.sm", { onclick: () => hostAsk("apply.review") }, "Review changes"), (bar = bb("Build again"))),
    ];
    bar.btn.classList.add("sm");
  } else if (S.build === "fail" && S.fail) {
    kids = [
      h("div.risein", { style: "display:grid;gap:8px" }, h("span", {}, h("span.tag.bad", {}, "Build failed")), h("span", {}, S.fail.message)),
      seedField(),
      (bar = bb("Build again", true)),
    ];
  } else {
    kids = [
      seedField(),
      kv(
        S.last
          ? [
              ["Output", OUTPUT],
              ["Last build", `${L.ago(S.last.at)} · ${L.seconds(S.last.ms)}`],
            ]
          : [
              ["Output", OUTPUT],
              ["Status", h("span.tag", {}, "New mod")],
            ],
      ),
      (bar = bb("Build", true)),
    ];
  }
  sideEl.replaceChildren(h("div.sh", {}, h("span", {}, "Build"), h("span.x")), h("div.card", {}, kids));
}

function page() {
  sideEl = h("section.side");
  app.replaceChildren(
    h("div.top", {}, (statusEl = h("span.status"))),
    h("div", { id: "live", class: "vh", "aria-live": "polite" }),
    h("div.mw", {}, groups(), sideEl),
  );
  setStatus();
  side();
}

async function saveSpoiler() {
  const r = S.result;
  const bytes = new TextEncoder().encode(r.spoiler).buffer;
  await hostAsk("disk.save", { name: `Randomizer seed ${r.seed} spoiler.txt`, bytes });
}

// ------------------------------------------------------------------ build
/** A file, or a piece of one, from the game's own copy: never what a mod holds. */
async function readGame(path) {
  const asked = pieceOf(path);
  const file = IN_GAME + (asked ? asked.file : path);
  const part = (at, length) => request("files.part", { path: file, at, length, own: true });
  if (asked) {
    if (asked.length <= MAX_PART) return (await part(asked.at, asked.length)).bytes;
    const out = new Uint8Array(asked.length);
    for (let done = 0; done < asked.length; done += MAX_PART) {
      const n = Math.min(MAX_PART, asked.length - done);
      out.set(new Uint8Array((await part(asked.at + done, n)).bytes), done);
    }
    return out.buffer;
  }
  const first = await part(0, MAX_PART);
  if (first.size <= MAX_PART) return first.bytes;
  const out = new Uint8Array(first.size);
  out.set(new Uint8Array(first.bytes));
  for (let done = MAX_PART; done < first.size; done += MAX_PART) {
    out.set(new Uint8Array((await part(done, MAX_PART)).bytes), done);
  }
  return out.buffer;
}

let module = null;
async function moduleCaller() {
  module ||= await workerCaller(new URL("./randomizer.wasm", import.meta.url));
  return async (req, data) => {
    const a = await module(req, data);
    if (a.json.error) throw new Error(a.json.error);
    return a;
  };
}

async function build() {
  if (S.build === "run") return;
  const seed = L.parseSeed(S.seedText);
  if (seed === null) {
    S.build = "fail";
    S.fail = { message: "A seed is a number of up to 15 digits." };
    side();
    setStatus();
    return;
  }
  const t0 = performance.now();
  S.build = "run";
  S.stop = false;
  setStatus();
  const b = bar;
  b.classList.add("run");
  b.btn.disabled = true;
  b.btn.classList.remove("pri");
  const u = { read: 0, toRead: 0, writing: false };
  const text = () => {
    if (u.writing) return "Writing files";
    if (!u.toRead) return "Starting";
    return u.read < u.toRead ? `Reading ${u.read.toLocaleString()} of ${u.toRead.toLocaleString()}` : "Building";
  };
  const show = () => {
    const f = L.progress(u);
    b.set({ frac: f, show: true, text: text() });
    busy("Building seed", f);
  };
  const tick = setInterval(show, 250);
  show();
  try {
    const call = await moduleCaller();
    const run = await until(
      call,
      L.request(seed, S.settings),
      readGame,
      (done, of) => {
        u.read = done;
        u.toRead = of;
      },
      () => S.stop,
    );
    if (S.stop) throw new Stopped();
    u.writing = true;
    const files = outputs(run).map((f) => ({ path: f.path, bytes: f.bytes.buffer }));
    // A file the seed before moved and this one does not goes, or it would stay in the mod.
    const paths = files.map((f) => f.path);
    const gone = L.stale(S.last?.paths || [], paths);
    if (gone.length) await request("output.remove", { name: OUTPUT, paths: gone });
    // Written whole, in one request: what is placed is never half a seed.
    await request("output.write", { name: OUTPUT, files }, files.map((f) => f.bytes));
    const ms = performance.now() - t0;
    S.result = { seed, slots: run.json.slots, ms, spoiler: run.json.spoiler };
    S.last = { at: Date.now(), ms, seed, paths };
    store.set("last", S.last);
    S.build = "done";
  } catch (e) {
    if (e instanceof Stopped) {
      S.build = "idle";
    } else {
      S.build = "fail";
      S.fail = { message: e.message || String(e) };
    }
  } finally {
    clearInterval(tick);
    busy();
    side();
    setStatus();
  }
}

// ------------------------------------------------------------------ start
async function fonts() {
  try {
    for (const f of (await request("fonts")) || []) {
      const face = new FontFace(f.family, f.bytes, { weight: f.weight, style: f.style });
      document.fonts.add(await face.load());
    }
  } catch {
    /* the fallback face is fine */
  }
}

const hello = await request("hello");
wear(hello.theme);
await fonts();
listen((event, data) => {
  if (event === "theme") wear(data);
  if (event === "stop") S.stop = true;
});
page();
window.ready = true;
