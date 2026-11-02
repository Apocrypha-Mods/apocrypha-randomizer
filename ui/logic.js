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

export const defaults = () => ({
  shuffle: Object.fromEntries(CATEGORIES.map(([k]) => [k, 100])),
  bosses: 0,
  ordinary: 0,
  mix: 0,
  dlc: "mixed",
});

/** `apoc-rando --enemies`, or nothing if no enemy moves. A mix needs bosses to send out. */
function enemies({ bosses = 0, ordinary = 0, mix = 0 }) {
  if (!bosses && !ordinary) return "";
  return `bosses=${bosses},ordinary=${ordinary},mix=${bosses ? mix : 0}`;
}

/** One seed's request to the module. */
export function request(seed, s) {
  return {
    op: "run",
    seed,
    shuffle: CATEGORIES.map(([k]) => `${k}=${s.shuffle[k] ?? 100}`).join(","),
    dlcSeparate: s.dlc === "separate",
    enemies: enemies(s),
  };
}

/** A seed as typed: digits only, up to what a double holds exactly, or null. */
export function parseSeed(text) {
  const t = text.trim();
  if (!/^\d{1,15}$/.test(t)) return null;
  return Number(t);
}

export const newSeed = (random = Math.random) => Math.floor(random() * 1e9);

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
