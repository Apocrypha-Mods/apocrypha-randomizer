// What the page works out, apart from the page, for the tests (ui/logic.test.mjs).

/** The categories a seed can move, by the name `apoc-rando --shuffle` has for them. */
export const CATEGORIES = [
  ["keys", "Key items"],
  ["weapons", "Weapons"],
  ["armour", "Armour"],
  ["talismans", "Talismans"],
  ["spells", "Spells"],
  ["ashes-of-war", "Ashes of war"],
  ["spirit-ashes", "Spirit ashes"],
  ["upgrades", "Upgrades"],
  ["quest", "Quest items"],
  ["consumables", "Consumables"],
];

/** The kinds of slot with a shuffle slider of their own. Gathering alone also takes a bias. */
export const PLACES = [
  ["merchants", "Merchants", "How much of what merchants sell is put in play. A row moves only if its item's own slider allows it too."],
  ["starting-gear", "Starting gear", "The classes' weapons, armour, talismans and spells. Each moves only if its item's own slider allows it too."],
  ["keepsakes", "Keepsakes", "The gifts offered when making a character. Each moves only if its item's own slider allows it too."],
  ["gathering", "Gathering", "What plants and ores give. These trade only with each other, and each only if its item's own slider allows it too."],
  ["npc-gear", "NPC gear", "What NPCs wear and hold. Nothing a player picks up. A weapon changes only if the weapons slider allows it too, armour the armour slider."],
];

/** The line under each section's heading. */
export const SECTIONS = {
  Start: "Where a new character begins.",
  Items: "How much of each kind of item moves to a new place.",
  "Lean towards late": "Push what moves deeper into the game. At 0% it can land anywhere.",
  "Other places": "Which kinds of places join the shuffle, beside chests, bodies and bosses.",
  Enemies: "Who stands where. Rewards always stay with the place.",
  "Quality of life": "Small changes to the game itself, whatever moves.",
};

/** What each item slider moves, opened by its ⓘ. */
export const SHUFFLE_HELP = {
  keys: "Medallions, Great Runes, keys and other items that open the way. However many move, every seed is proven finishable.",
  weapons: "100%: every weapon moves. 0%: every weapon stays where the game has it, shops included.",
  armour: "Every piece of armour. 0% keeps each where the game has it.",
  talismans: "Every talisman. 0% keeps each where the game has it.",
  spells: "Sorceries and incantations.",
  "ashes-of-war": "Ashes of War, which give a weapon a new skill.",
  "spirit-ashes": "Spirit ashes for summoning help.",
  upgrades: "Smithing and Somber stones, Golden Seeds and Sacred Tears.",
  quest: "Quest items, tools, letters and cookbooks.",
  consumables: "Consumables and crafting materials.",
};

/** What each lean slider does, opened by its ⓘ. */
export function leanHelp(k, label) {
  if (k === "keys") return "How deep key items lean. High, they tend to sit far into the game, so one opens the way to the next.";
  if (k === "gathering") return "How strongly what plants and ores give leans towards later areas.";
  return `How strongly moved ${label.toLowerCase()} lean towards later areas. High, the better ones tend to be found deeper in.`;
}

/** What takes a bias: every category of item, and gathering. */
export const LEANS = [...CATEGORIES, ["gathering", "Gathering"]];

/** How many Great Runes open Leyndell, as `--leyndell-runes`: 2 is the game's own. */
export const LEYNDELL_RUNES = [0, 1, 2, 3, 4].map((n) => [n, String(n)]);

/**
 * Who stands on every spot for "Oops all": none, the seed's draw, or one of these, each
 * a name the module knows (`real_oops.rs` holds the module to them).
 */
export const OOPS = [
  ["", "Off"],
  ["random", "Random"],
  ["Margit, the Fell Omen", "Margit, the Fell Omen"],
  ["Tree Sentinel", "Tree Sentinel"],
  ["Soldier of Godrick", "Soldier of Godrick"],
  ["Crucible Knight", "Crucible Knight"],
  ["Black Knife Assassin", "Black Knife Assassin"],
  ["Godskin Apostle", "Godskin Apostle"],
  ["Mad Pumpkin Head", "Mad Pumpkin Head"],
  ["Misbegotten Warrior", "Misbegotten Warrior"],
  ["Erdtree Burial Watchdog", "Erdtree Burial Watchdog"],
  ["Bell Bearing Hunter", "Bell Bearing Hunter"],
  ["Patches", "Patches"],
  ["Starscourge Radahn", "Starscourge Radahn"],
  ["Rellana, Twin Moon Knight", "Rellana, Twin Moon Knight"],
  ["Dancer of Ranah", "Dancer of Ranah"],
  ["Bayle the Dread", "Bayle the Dread"],
];

/** Messmer's Kindling's shards as typed: 1 to 20, and 1 to that many needed; 1 shard is the Kindling as it is. */
export function shardsOf(count, needed) {
  const whole = (v, lo, hi, or) => {
    const n = Number(v);
    return Number.isInteger(n) && n >= lo && n <= hi ? n : or;
  };
  const c = whole(count, 1, 20, 1);
  return { kindlingShards: c, kindlingNeeded: whole(needed, 1, c, c) };
}

/** Onslaught as typed: a whole number 1 to 10, anything else as the game (1). */
export function onslaughtOf(v) {
  const n = Number(v);
  return Number.isInteger(n) && n >= 1 && n <= 10 ? n : 1;
}

/** The most models a map may gain, as `models=N`: the game's own limit is not known. */
export const MODEL_LIMITS = [
  [20, "20"],
  [40, "40"],
  [0, "No limit"],
];

export const defaults = () => ({
  shuffle: Object.fromEntries([...CATEGORIES, ...PLACES].map(([k]) => [k, 100])),
  bias: Object.fromEntries(LEANS.map(([k]) => [k, 0])),
  bosses: 0,
  ordinary: 0,
  mixOn: false,
  mix: 10,
  models: 20,
  scale: true,
  autoUpgrade: false,
  dlc: "mixed",
  leyndellRunes: 2,
  keepWhetblades: false,
  keepMaps: false,
  wholeMap: false,
  nightBosses: false,
  usableStart: false,
  lightStart: false,
  allRecipes: false,
  cheapUpgrades: false,
  noRequirements: false,
  bossRunes: false,
  noMaleniaHeal: false,
  noGargoylePoison: false,
  hiddenSeed: false,
  oopsAll: "",
  onslaught: 1,
  gaolKeys: false,
  kindlingShards: 1,
  kindlingNeeded: 1,
  dlcStart: false,
  randomGestures: false,
});

/** Settings kept from an earlier visit over the defaults, sliders added since included. */
export function settled(kept) {
  const d = defaults();
  return {
    ...d,
    ...kept,
    // Saved before the switch: a mix above 0 was the way to turn it on.
    mixOn: kept?.mixOn ?? (kept?.mix ?? 0) > 0,
    shuffle: { ...d.shuffle, ...kept?.shuffle },
    bias: { ...d.bias, ...kept?.bias },
  };
}

/** `apoc-rando --enemies`, or nothing if no enemy moves. A mix needs its switch on and bosses to send out. */
function enemies({ bosses = 0, ordinary = 0, mixOn = false, mix = 0, models = 20 }) {
  if (!bosses && !ordinary) return "";
  return `bosses=${bosses},ordinary=${ordinary},mix=${bosses && mixOn ? mix : 0},models=${models}`;
}

const sliders = (list, values, or) => list.map(([k]) => `${k}=${values[k] ?? or}`).join(",");

/** One seed's request to the module. `stack` is what the other mods hold ([`stackOf`]). */
export function request(seed, s, stack = null) {
  const r = {
    op: "run",
    seed,
    shuffle: sliders([...CATEGORIES, ...PLACES], s.shuffle, 100),
    bias: sliders(LEANS, s.bias, 0),
    dlcSeparate: s.dlc === "separate",
    enemies: enemies(s),
    noScaling: !s.scale,
    autoUpgrade: !!s.autoUpgrade,
    leyndellRunes: s.leyndellRunes ?? 2,
    keepWhetblades: !!s.keepWhetblades,
    keepMaps: !!s.keepMaps,
    wholeMap: !!s.wholeMap,
    nightBossesAlways: !!s.nightBosses,
    usableStart: !!s.usableStart,
    lightStart: !!s.usableStart && !!s.lightStart,
    allRecipes: !!s.allRecipes,
    cheapUpgrades: !!s.cheapUpgrades,
    noRequirements: !!s.noRequirements,
    bossRunesFollow: !!s.bossRunes,
    hidden: !!s.hiddenSeed,
    oopsAll: s.oopsAll || "",
    gaolKeys: !!s.gaolKeys,
    dlcStart: !!s.dlcStart,
    randomGestures: !!s.randomGestures,
    ...shardsOf(s.kindlingShards, s.kindlingNeeded),
    onslaught: onslaughtOf(s.onslaught),
    noMaleniaHeal: !!s.noMaleniaHeal,
    noGargoylePoison: !!s.noGargoylePoison,
  };
  if (stack && (stack.files.length || stack.regulations.length)) r.stack = stack;
  return r;
}

/** Folders of the game a seed reads and rewrites; a mod's other files do not bear on it. */
const READS = /^(map\/mapstudio|event|script\/talk|msg)\//i;

/** What the mod manager says the other mods put in the game, as the module wants it. */
export function stackOf(answer) {
  const files = (answer?.files || []).filter((f) => READS.test(f.path)).map((f) => ({ path: f.path, size: f.size }));
  const regulations = (answer?.merged || []).find((m) => m.path.toLowerCase() === "regulation.bin")?.copies || [];
  return { files, regulations: regulations.map((c) => c.mod), mods: answer?.mods || [] };
}

/** Whether the mods a seed stands on are no longer those on now: it holds their tables. */
export function modsChanged(built = [], now = []) {
  return built.length !== now.length || built.some((m, i) => m !== now[i]);
}

/** The native module's place in the mod: at its root, which the mod manager loads as code. */
export const NATIVE = "apoc_native.dll";

/** A seed as typed: digits only, up to what a double holds exactly, or null. */
export function parseSeed(text) {
  const t = text.trim();
  if (!/^\d{1,15}$/.test(t)) return null;
  return Number(t);
}

export const newSeed = (random = Math.random) => Math.floor(random() * 1e9);

/** A hidden seed's number, drawn from the browser's strong randomness and never shown: up to 15 digits, as a typed seed. */
export function hiddenSeed(fill = (a) => crypto.getRandomValues(a)) {
  const [hi, lo] = fill(new Uint32Array(2));
  return ((hi % 232831) * 2 ** 32 + lo) % 1e15;
}

/** How a built seed is named on the page: its number, or "Hidden" for a hidden seed. */
export const seedName = (seed) => (seed == null ? "Hidden" : String(seed));

/** The first file of a seed's mod that an earlier one left and this one lacks. */
export function stale(before, now) {
  const kept = new Set(now);
  return before.filter((p) => !kept.has(p));
}

/** How far along a run is, 0 to 1: reading is most of the wait at first, then building. */
export function progress({ read, toRead, writing }) {
  if (writing) return 1;
  if (!toRead) return 0;
  return Math.min(0.6, (read / toRead) * 0.6);
}

export function seconds(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

export function ago(at, now = Date.now()) {
  const m = Math.floor((now - at) / 60000);
  if (m < 1) return "Just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  return h < 24 ? `${h} h ago` : `${Math.floor(h / 24)} d ago`;
}
