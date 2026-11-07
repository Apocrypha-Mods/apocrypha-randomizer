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
  settings: L.settled(store.get("settings", {})),
  seedText: String(store.get("seed", L.newSeed())),
  last: store.get("last", null), // { at, ms, seed, files, paths }
  result: null, // { seed, slots, files, ms, spoiler }
  fail: null, // { message, ms }
  stop: false,
  stack: null, // what the other mods hold, as last asked
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
  for (const c of kids.flat(Infinity)) if (c != null && c !== false) el.append(c instanceof Node ? c : String(c));
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
  if (S.last && L.modsChanged(S.last.mods, S.stack?.mods)) return "Your mods have changed since this seed: build again";
  const named = (seed) => (seed == null ? "Hidden seed" : `Seed ${seed}`);
  if (S.build === "done") return `${named(S.result.seed)} built`;
  return S.last ? `${named(S.last.seed)} built ${L.ago(S.last.at).toLowerCase()}` : "No seed built";
}

function setStatus() {
  statusEl.replaceChildren(h("span.fadein", {}, status()));
}

const saveSettings = () => store.set("settings", S.settings);

/** A setting's row, with an ⓘ that opens a line or two about it under the row. */
function explained(row, label, about) {
  if (!about) return h("div.item", {}, row);
  const help = h("p.help", { hidden: true }, about);
  const info = h(
    "button.info",
    {
      type: "button",
      "aria-label": `About ${label}`,
      "aria-expanded": "false",
      onclick: () => {
        help.hidden = !help.hidden;
        info.setAttribute("aria-expanded", String(!help.hidden));
        info.classList.toggle("on", !help.hidden);
      },
    },
    "i",
  );
  row.querySelector(".n").append(info);
  return h("div.item", {}, row, help);
}

function slider(label, value, set, tag, about) {
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
  return explained(h("div.sr", {}, h("span.n", {}, label, tag), input, out), label, about);
}

function groups() {
  const st = S.settings;
  const seg = (label, options, now, set) => {
    const el = h(
      "div.seg",
      { role: "group", "aria-label": label },
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
  const pick = (label, title, options, now, set, tag) =>
    explained(h("div.sr.pick", {}, h("span.n", {}, label, tag), seg(label, options, now, set)), label, title);
  const untested = (why) => h("span.tag.warn", { title: `${why}Not yet played through in the game` }, "Untested");
  const mixRow = slider("Boss mix", st.mix ?? 10, (v) => (st.mix = v), null, "Of the bosses that move, how many trade places with an ordinary enemy instead: the boss goes out into the world, and the enemy takes its arena with a boss's health bar.");
  if (!st.mixOn) mixRow.style.display = "none";
  const section = (name, ...kids) =>
    h("section", {}, h("div.sh", {}, h("span", {}, name), h("span.x")), h("p.sub", {}, L.SECTIONS[name] ?? ""), h("div.card.rows", {}, kids));
  return h(
    "div.groups",
    {},
    section(
      "Fog gates",
      pick(
        "Fog gate randomizer",
        "Boss fogs and the entrances of caves, catacombs, tunnels and hero's graves lead somewhere else: walk into Margit's fog and you may stand before a catacomb's boss, or go down into Stormfoot Catacombs and come out in a cave in Caelid. A dungeon's entrance leads back the way you came; a boss fog may not once the boss is dead. The seed is made so the game can be finished through them, with the item shuffle below or, with every item slider at 0, on its own. Legacy dungeons' gates that need something (Leyndell's, the Haligtree's), waygates, the start and the end stay as they are.",
        [
          [false, "Off"],
          [true, "On"],
        ],
        !!st.fogGates,
        (v) => (st.fogGates = v),
        untested(""),
      ),
    ),
    section(
      "Start",
      pick(
        "DLC Start",
        "Skip the base game: a new character is taken from the Chapel of Anticipation straight to the Gravesite Plain in the Land of Shadow, with Torrent's whistle and the Spirit Calling Bell, and the seed is made so the DLC can be finished from there, its last boss the goal. The base game is still there to walk to, but nothing you need is put in it. You arrive at the class's own level, and can level up at any grace. The four choices below only work with DLC Start on.",
        [
          [false, "Off"],
          [true, "On"],
        ],
        !!st.dlcStart,
        (v) => (st.dlcStart = v),
        untested(""),
      ),
      explained(
        h(
          "div.sr.pick",
          {},
          h("span.n", {}, "DLC Start: runes for level", untested("")),
          h("input.inp.num", {
            type: "number",
            min: "0",
            max: "200",
            step: "1",
            value: String(L.levelOf(st.dlcStartLevel)),
            "aria-label": "DLC Start: runes for level",
            onchange: (e) => {
              st.dlcStartLevel = L.levelOf(e.target.value);
              e.target.value = String(st.dlcStartLevel);
              saveSettings();
            },
          }),
        ),
        "DLC Start: runes for level",
        "Lord's Runes enough to take a level 1 character to this level, 0 to 200, to spend at a grace. 0 gives none. 100 is 40 Lord's Runes, 150 is 143.",
      ),
      pick(
        "DLC Start: flasks",
        "Start with all 30 Golden Seeds and 12 Sacred Tears, to raise your flasks at a grace, the three talisman pouches, the eight memory stones and the Flask of Wondrous Physick.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.dlcStartFlasks,
        (v) => (st.dlcStartFlasks = v),
        untested(""),
      ),
      pick(
        "DLC Start: smithing",
        "Start with every Smithing-Stone and Somberstone Miner's Bell Bearing, to give the Twin Maiden Husks so they sell every stone, and the Whetstone Knife with every whetblade.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.dlcStartSmithing,
        (v) => (st.dlcStartSmithing = v),
        untested(""),
      ),
      pick(
        "DLC Start: base-game equipment",
        "Every Site of Grace's menu gets \"Base-game equipment\": a shop selling, for nothing, every weapon, armour piece, talisman, spell, Ash of War and spirit ash the base game has, so a DLC-only run can still use them. The entry's name is in English in every language.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.dlcStartShop,
        (v) => (st.dlcStartShop = v),
        untested(""),
      ),
    ),
    section(
      "Items",
      L.CATEGORIES.map(([k, label]) => slider(label, st.shuffle[k] ?? 100, (v) => (st.shuffle[k] = v), null, L.SHUFFLE_HELP[k])),
      pick(
        "Shadow of the Erdtree",
        "Whether base-game items can land in the DLC, and DLC items in the base game",
        [
          ["mixed", "Mixed in"],
          ["separate", "Kept apart"],
        ],
        st.dlc,
        (v) => (st.dlc = v),
      ),
      pick(
        "Whetblades in the shuffle",
        "Whether the Whetstone Knife and the whetblades, which let you put Ashes of War and affinities on weapons, move with the rest or stay where the game has them",
        [
          [false, "Yes"],
          [true, "No"],
        ],
        !!st.keepWhetblades,
        (v) => (st.keepWhetblades = v),
      ),
      pick(
        "Map fragments in the shuffle",
        "Whether map fragments move with the rest or stay at their steles, where the game has them",
        [
          [false, "Yes"],
          [true, "No"],
        ],
        !!st.keepMaps,
        (v) => (st.keepMaps = v),
      ),
      pick(
        "Lamenter's Gaol keys in the shuffle",
        "Whether the Gaol Upper and Lower Level Keys, which open Lamenter's Gaol's locked doors in the DLC, move with the rest. If they move, anything else may be found in the gaol, and the seed makes sure you can find both keys before you need what is behind those doors.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.gaolKeys,
        (v) => (st.gaolKeys = v),
        untested(""),
      ),
      (() => {
        const num = (label, get, set) =>
          h("input.inp.num", {
            type: "number",
            min: "1",
            max: "20",
            step: "1",
            value: String(get()),
            "aria-label": label,
            onchange: (e) => {
              set(e.target.value);
              const s = L.shardsOf(st.kindlingShards, st.kindlingNeeded);
              st.kindlingShards = s.kindlingShards;
              st.kindlingNeeded = s.kindlingNeeded;
              e.target.value = String(get());
              saveSettings();
            },
          });
        const now = () => L.shardsOf(st.kindlingShards, st.kindlingNeeded);
        return explained(
          h(
            "div.sr.pick",
            {},
            h("span.n", {}, "Messmer's Kindling shards", untested("")),
            h(
              "div.seedrow",
              {},
              num("Shards needed", () => now().kindlingNeeded, (v) => (st.kindlingNeeded = v)),
              h("span.sec", {}, "of"),
              num("Shards in the world", () => now().kindlingShards, (v) => (st.kindlingShards = v)),
            ),
          ),
          "Messmer's Kindling shards",
          "Split Messmer's Kindling, which burns the Sealing Tree in the DLC, into shards spread through the world: 3 of 5 means five shards are hidden and any three burn the tree. 1 of 1 is the Kindling as the game has it. Each shard takes the place of a rune in a DLC treasure.",
        );
      })(),
      pick(
        "Start with the whole map",
        "Every part of the world map is drawn from the start, landmarks included, as if you held every map fragment. Sites of grace still appear only once you touch them.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.wholeMap,
        (v) => (st.wholeMap = v),
        untested(""),
      ),
      pick(
        "Great Runes for Leyndell",
        "How many of Godrick's, Radahn's, Rykard's, Mohg's and Malenia's Great Runes, restored or not, and Rennala's Great Rune of the Unborn open the capital's gate. The game asks for 2.",
        L.LEYNDELL_RUNES,
        st.leyndellRunes ?? 2,
        (v) => (st.leyndellRunes = v),
      ),
    ),
    section(
      "Lean towards late",
      L.LEANS.map(([k, label]) => slider(label, st.bias[k] ?? 0, (v) => (st.bias[k] = v), null, L.leanHelp(k, label))),
    ),
    section(
      "Other places",
      L.PLACES.map(([k, label, about]) => slider(label, st.shuffle[k] ?? 100, (v) => (st.shuffle[k] = v), null, about)),
      pick(
        "Usable starting gear",
        "Whether each class can use what it starts with: weapons its stats can wield, two-handed if need be, a bow only with arrows, and spells it has the staff or seal, the stats and the memory for. Fully random may hand a Wretch a greatsword it cannot lift.",
        [
          [false, "Fully random"],
          [true, "Usable"],
        ],
        !!st.usableStart,
        (v) => (st.usableStart = v),
      ),
      pick(
        "Starting gear weight",
        "With usable starting gear: whether each class's weapons, armour and talismans are also light enough for a medium roll, at most 70% of the equip load its endurance gives, so no class starts fat-rolling or heavy-rolling. The game's own Vagabond starts at a heavy roll. Does nothing with fully random gear.",
        [
          [false, "Any weight"],
          [true, "Medium roll"],
        ],
        !!st.lightStart,
        (v) => (st.lightStart = v),
      ),
      pick(
        "Every crafting recipe",
        "Know every crafting recipe from the start, so you can make arrows, pots and the rest without hunting the cookbooks first. You still need the crafting kit.",
        [
          [false, "From cookbooks"],
          [true, "All known"],
        ],
        !!st.allRecipes,
        (v) => (st.allRecipes = v),
        untested(""),
      ),
      pick(
        "One stone per upgrade",
        "Each regular upgrade costs one Smithing Stone instead of two, four or six, so a weapon found late can catch up. Somber upgrades already cost one.",
        [
          [false, "As the game"],
          [true, "One stone"],
        ],
        !!st.cheapUpgrades,
        (v) => (st.cheapUpgrades = v),
        untested(""),
      ),
      pick(
        "No stat requirements",
        "Any weapon or spell can be used at any stats: no strength, dexterity, intelligence, faith or arcane asked. Spells still need their staff or seal and the memory slots.",
        [
          [false, "As the game"],
          [true, "None"],
        ],
        !!st.noRequirements,
        (v) => (st.noRequirements = v),
        untested(""),
      ),
      pick(
        "Auto-upgrade",
        "Weapons arrive at the highest level you have raised one of their path to. Loads our own module into the game.",
        [
          [false, "Off"],
          [true, "On"],
        ],
        !!st.autoUpgrade,
        (v) => (st.autoUpgrade = v),
      ),
    ),
    section(
      "Enemies",
      slider("Bosses", st.bosses, (v) => (st.bosses = v), null, "How many boss fights get a different boss. 100%: every arena has someone new; 0%: every boss stays."),
      slider("Ordinary", st.ordinary ?? 0, (v) => (st.ordinary = v), untested(""), "How many ordinary enemies are replaced by others of about their size, so no dragon is put in a tunnel."),
      pick(
        "Mix bosses and ordinary",
        "Whether some bosses may trade places with ordinary enemies: the boss goes out into the world, and the ordinary enemy takes its arena. Needs Bosses above 0.",
        [
          [false, "Off"],
          [true, "On"],
        ],
        !!st.mixOn,
        (v) => {
          st.mixOn = v;
          mixRow.style.display = v ? "" : "none";
        },
      ),
      mixRow,
      explained(
        h(
          "div.sr.pick",
          {},
          h("span.n", {}, "Oops all", untested("")),
          h(
            "select.inp",
            {
              "aria-label": "Oops all",
              onchange: (e) => {
                st.oopsAll = e.target.value;
                saveSettings();
              },
            },
            L.OOPS.map(([k, label]) => h("option", { value: k, selected: k === (st.oopsAll || "") }, label)),
          ),
        ),
        "Oops all",
        "Every enemy becomes the one chosen, bosses too: a world of Margits. Each copy has the strength of the place it stands in. The sliders above then do nothing, and the DLC keeps its own enemies unless the one chosen is from the DLC. Malenia, Godrick and bosses who bring helpers cannot be chosen.",
      ),
      explained(
        h(
          "div.sr.pick",
          {},
          h("span.n", {}, "Onslaught", untested("")),
          h("input.inp.num", {
            type: "number",
            min: "1",
            max: "10",
            step: "1",
            value: String(L.onslaughtOf(st.onslaught)),
            "aria-label": "Onslaught: how many of each enemy",
            onchange: (e) => {
              st.onslaught = L.onslaughtOf(e.target.value);
              e.target.value = String(st.onslaught);
              saveSettings();
            },
          }),
        ),
        "Onslaught",
        "How many of each ordinary enemy stand where one stood: 1 is the game as it is, 3 is three soldiers at every soldier's post. Only enemies no script names are copied, so bosses and ambushes stay as they are. Above 3 the game may slow down or crash.",
      ),
      pick(
        "Scaling",
        "Whether a moved enemy takes the health and strength of the place it lands on, or keeps its own",
        [
          [true, "Of the place"],
          [false, "Its own"],
        ],
        st.scale !== false,
        (v) => (st.scale = v),
      ),
      pick(
        "Boss runes",
        "Whether a moved boss pays the runes of the arena it lands in, or its own. Of the boss: a Godskin Apostle in Margit's arena pays a Godskin Apostle's runes, and a crab there pays a crab's.",
        [
          [false, "Of the arena"],
          [true, "Of the boss"],
        ],
        !!st.bossRunes,
        (v) => (st.bossRunes = v),
        untested(""),
      ),
      pick(
        "Night bosses at any hour",
        "Night's Cavalry ride by day as well as by night, and Bell Bearing Hunters wait at their shacks at any hour. While a hunter is there, its shack's merchant stays away, as at night, until the hunter is beaten. Deathbirds and Death Rite Birds come out by day too. That part is a best guess not yet seen in play: it may instead stop them rising again after they fall.",
        [
          [false, "No"],
          [true, "Yes"],
        ],
        !!st.nightBosses,
        (v) => (st.nightBosses = v),
        untested(""),
      ),
      pick(
        "New models per map",
        "The most models ordinary enemies may add to one map. The game's limit is not known, so higher may crash it.",
        L.MODEL_LIMITS,
        st.models ?? 20,
        (v) => (st.models = v),
      ),
    ),
    section(
      "Quality of life",
      pick(
        "Malenia's heal",
        "Whether Malenia takes back health each time she hits you, as in the game. Off, her hits heal her nothing; the rest of her fight is the same.",
        [
          [false, "As the game"],
          [true, "Off"],
        ],
        !!st.noMaleniaHeal,
        (v) => (st.noMaleniaHeal = v),
        untested(""),
      ),
      pick(
        "Gestures",
        "Whether the gestures found on bodies and given by events change places: the Polite Bow may be where Rapture was. O Mother stays where it is, as it opens a statue in the Shadow Keep, and gestures that people teach you in talk stay theirs.",
        [
          [false, "As the game"],
          [true, "Shuffled"],
        ],
        !!st.randomGestures,
        (v) => (st.randomGestures = v),
        untested(""),
      ),
      pick(
        "Gargoyles' poison clouds",
        "The Valiant Gargoyles' breath leaves clouds of poison that hurt while you stand in them. Off, the clouds still show but do nothing; the breath itself still poisons.",
        [
          [false, "As the game"],
          [true, "Off"],
        ],
        !!st.noGargoylePoison,
        (v) => (st.noGargoylePoison = v),
        untested(""),
      ),
      pick(
        "Torrent in the Abyssal Woods",
        "Torrent can be called in the DLC's Abyssal Woods, where the game sends him away. The woods were built to be walked, so riding may take you past things meant to be found slowly.",
        [
          [false, "As the game"],
          [true, "Allowed"],
        ],
        !!st.torrentWoods,
        (v) => (st.torrentWoods = v),
        untested(""),
      ),
      pick(
        "Boss music",
        "Each boss arena plays another boss's theme, its second phase as that theme has it. A theme may stay silent somewhere the game does not load its sounds.",
        [
          [false, "As the game"],
          [true, "Shuffled"],
        ],
        !!st.bossMusic,
        (v) => (st.bossMusic = v),
        untested(""),
      ),
      pick(
        "Area music",
        "The music of the areas, dungeons, Stormveil, the Roundtable Hold and the Land of Shadow changes places among them: each piece plays everywhere another one played.",
        [
          [false, "As the game"],
          [true, "Shuffled"],
        ],
        !!st.areaMusic,
        (v) => (st.areaMusic = v),
        untested(""),
      ),
      pick(
        "Enemy colours",
        "Each kind of ordinary enemy is drawn in one of twelve colours, the same for every enemy of that kind in a seed. Bosses, NPCs and invaders keep their own look.",
        [
          [false, "As the game"],
          [true, "Coloured"],
        ],
        !!st.enemyColours,
        (v) => (st.enemyColours = v),
        untested(""),
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
  // A hidden seed is drawn as the build starts and never shown, and no spoiler log is kept.
  const hide = h(
    "button.btn.sm" + (S.settings.hiddenSeed ? ".on" : ""),
    {
      "aria-pressed": String(!!S.settings.hiddenSeed),
      title: "Draw a seed you are not told, and keep no spoiler log, so nobody knows where anything went",
      onclick: () => {
        S.settings.hiddenSeed = !S.settings.hiddenSeed;
        saveSettings();
        side();
      },
    },
    "Hidden",
  );
  if (S.settings.hiddenSeed) {
    const note = h("input.inp", { value: "", disabled: true, placeholder: "Drawn when you build", "aria-label": "Seed" });
    return h("label.fld", {}, h("span", {}, "Seed"), h("div.seedrow", {}, note, hide), h("p.help", {}, "No spoiler log is kept and the seed is not shown, so nobody can look up where anything went."));
  }
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
  return h("label.fld", {}, h("span", {}, "Seed"), h("div.seedrow", {}, input, another, hide));
}

/** Which mods the next seed stands on: all those on now, as Apply will place them. */
function stackLine() {
  const mods = S.stack?.mods || [];
  if (!mods.length) return null;
  return h("div.kv", {}, h("span.k", {}, "Built on"), h("span.val", { title: mods.join("\n") }, mods.length === 1 ? mods[0] : `${mods.length} mods`));
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
        ["Seed", L.seedName(r.seed)],
        ...(r.mods.length ? [["Built on", h("span", { title: r.mods.join("\n") }, r.mods.length === 1 ? r.mods[0] : `${r.mods.length} mods`)]] : []),
        ["Status", h("span.accent.fadein", {}, "Apply to place")],
      ]),
      r.seed == null ? null : h("button.btn.sm", { onclick: saveSpoiler }, "Save spoiler log…"),
      h("div.pair", {}, h("button.btn.sm", { onclick: () => hostAsk("apply.review") }, "Review changes"), (bar = bb("Build again"))),
    ];
    bar.btn.classList.add("sm");
  } else if (S.build === "fail" && S.fail) {
    kids = [
      h("div.risein", { style: "display:grid;gap:8px" }, h("span", {}, h("span.tag.bad", {}, "Build failed")), h("span", {}, S.fail.message)),
      seedField(),
      stackLine(),
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
      stackLine(),
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
  // What another mod holds, the module's name for it: `mod:<path>`, or `mod#<n>` for the
  // n-th mod's tables, each of which the mod manager will merge at Apply.
  if (path.startsWith("mod:")) return request("files.stack.read", { name: OUTPUT, path: path.slice(4) });
  if (path.startsWith("mod#")) return request("files.stack.read", { name: OUTPUT, path: "regulation.bin", copy: Number(path.slice(4)) });
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
  const hidden = !!S.settings.hiddenSeed;
  const seed = hidden ? L.hiddenSeed() : L.parseSeed(S.seedText);
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
    // Read now, as mods may have been turned on or off since the page opened.
    S.stack = L.stackOf(await request("files.stack", { name: OUTPUT }));
    const run = await until(
      call,
      L.request(seed, S.settings, S.stack),
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
    if (S.settings.autoUpgrade) {
      const native = await fetch(new URL(`./${L.NATIVE}`, import.meta.url));
      if (!native.ok) throw new Error("This add-on's module for auto-upgrade is missing.");
      files.push({ path: L.NATIVE, bytes: await native.arrayBuffer() });
    }
    // A file the seed before moved and this one does not goes, or it would stay in the mod.
    const paths = files.map((f) => f.path);
    const gone = L.stale(S.last?.paths || [], paths);
    if (gone.length) await request("output.remove", { name: OUTPUT, paths: gone });
    // Written whole, in one request: what is placed is never half a seed.
    await request("output.write", { name: OUTPUT, files, last: true }, files.map((f) => f.bytes));
    const ms = performance.now() - t0;
    // A hidden seed's number is forgotten here, as the module kept no spoiler log of it.
    const shown = hidden ? null : seed;
    S.result = { seed: shown, slots: run.json.slots, ms, spoiler: hidden ? "" : run.json.spoiler, mods: S.stack.mods };
    S.last = { at: Date.now(), ms, seed: shown, paths, mods: S.stack.mods };
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
S.stack = L.stackOf(await hostAsk("files.stack", { name: OUTPUT }));
page();
window.ready = true;
