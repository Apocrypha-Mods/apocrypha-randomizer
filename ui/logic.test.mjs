import assert from "node:assert/strict";
import { test } from "node:test";
import * as L from "./logic.js";

test("enemies move only when asked", () => {
  assert.equal(L.request(1, L.defaults()).enemies, "");
  assert.equal(L.request(1, { ...L.defaults(), bosses: 60 }).enemies, "bosses=60,ordinary=0,mix=0,models=20");
  assert.equal(L.request(1, { ...L.defaults(), ordinary: 40 }).enemies, "bosses=0,ordinary=40,mix=0,models=20");
  assert.equal(L.request(1, { ...L.defaults(), bosses: 100, ordinary: 100, mixOn: true, mix: 10 }).enemies, "bosses=100,ordinary=100,mix=10,models=20");
});
test("a mix with no bosses moves nothing", () => {
  assert.equal(L.request(1, { ...L.defaults(), mix: 50 }).enemies, "");
  assert.equal(L.request(1, { ...L.defaults(), ordinary: 10, mixOn: true, mix: 50 }).enemies, "bosses=0,ordinary=10,mix=0,models=20");
});

test("the mix moves nothing while its switch is off, and settings saved before the switch keep their mix", () => {
  assert.equal(L.request(1, { ...L.defaults(), bosses: 100, mix: 50 }).enemies, "bosses=100,ordinary=0,mix=0,models=20");
  assert.equal(L.settled({ bosses: 100, mix: 30 }).mixOn, true);
  assert.equal(L.settled({ bosses: 100, mix: 0 }).mixOn, false);
  assert.equal(L.settled({}).mixOn, false);
});
test("a seed is digits", () => {
  assert.equal(L.parseSeed(" 42 "), 42);
  for (const bad of ["", "4.2", "-1", "abc", "1234567890123456"]) assert.equal(L.parseSeed(bad), null);
});
test("a file the last seed had and this one lacks goes", () => {
  assert.deepEqual(L.stale(["a", "b"], ["b", "c"]), ["a"]);
});
test("the sliders reach every category", () => {
  const s = L.defaults();
  s.shuffle.weapons = 0;
  const r = L.request(1, s);
  assert.match(r.shuffle, /weapons=0/);
  assert.equal(r.shuffle.split(",").length, L.CATEGORIES.length + L.PLACES.length);
});
test("every option reaches the request", () => {
  const s = L.defaults();
  s.shuffle.merchants = 30;
  s.shuffle["npc-gear"] = 0;
  s.bias.keys = 75;
  s.bias.gathering = 10;
  Object.assign(s, { bosses: 100, models: 0, scale: false, autoUpgrade: true });
  const r = L.request(1, s);
  assert.match(r.shuffle, /merchants=30/);
  assert.match(r.shuffle, /npc-gear=0/);
  assert.match(r.bias, /keys=75/);
  assert.match(r.bias, /gathering=10/);
  assert.match(r.enemies, /models=0/);
  assert.equal(r.noScaling, true);
  assert.equal(r.autoUpgrade, true);
});
test("Leyndell asks the game's 2 Great Runes unless set", () => {
  assert.equal(L.request(1, L.defaults()).leyndellRunes, 2);
  assert.equal(L.request(1, L.settled({})).leyndellRunes, 2);
  assert.equal(L.request(1, { ...L.defaults(), leyndellRunes: 0 }).leyndellRunes, 0);
  assert.deepEqual(L.LEYNDELL_RUNES.map(([n]) => n), [0, 1, 2, 3, 4]);
});
test("only what can lean is given a bias", () => {
  const r = L.request(1, L.defaults());
  for (const dead of ["merchants", "starting-gear", "keepsakes", "npc-gear"]) assert.doesNotMatch(r.bias, new RegExp(dead));
  // Not even from settings kept from before: the module refuses a lean for them.
  const kept = L.request(1, L.settled({ bias: { merchants: 80, "npc-gear": 5, keys: 30 } }));
  for (const dead of ["merchants", "starting-gear", "keepsakes", "npc-gear"]) assert.doesNotMatch(kept.bias, new RegExp(dead));
  assert.match(kept.bias, /keys=30/);
});
test("settings kept from before the new sliders gain them", () => {
  const s = L.settled({ shuffle: { weapons: 10 }, bosses: 40 });
  assert.equal(s.shuffle.weapons, 10);
  assert.equal(s.shuffle.merchants, 100);
  assert.equal(s.bias.keys, 0);
  assert.equal(s.scale, true);
  assert.equal(s.bosses, 40);
});
test("the stack keeps what a seed reads, and every copy of the tables", () => {
  const stack = L.stackOf({
    files: [
      { path: "map/MapStudio/m10_00_00_00.msb.dcx", size: 1 },
      { path: "chr/c0000.anibnd.dcx", size: 2 },
      { path: "script/talk/m10_00_00_00.talkesdbnd.dcx", size: 3 },
    ],
    merged: [{ path: "regulation.bin", copies: [{ mod: "A" }, { mod: "B" }] }],
    mods: ["A", "B"],
  });
  assert.deepEqual(stack.files.map((f) => f.size), [1, 3]);
  assert.deepEqual(stack.regulations, ["A", "B"]);
});
test("no other mods, no stack in the request", () => {
  assert.equal(L.request(1, L.defaults(), L.stackOf({})).stack, undefined);
  assert.equal(L.request(1, L.defaults(), null).stack, undefined);
});
test("a seed knows when the mods under it have changed", () => {
  assert.equal(L.modsChanged(["A"], ["A"]), false);
  assert.equal(L.modsChanged(undefined, []), false);
  assert.equal(L.modsChanged(["A"], []), true);
  assert.equal(L.modsChanged(["A"], ["B"]), true);
});

test("whetblades and map fragments move unless kept in place", () => {
  const d = L.request(1, L.defaults());
  assert.equal(d.keepWhetblades, false);
  assert.equal(d.keepMaps, false);
  const k = L.request(1, { ...L.defaults(), keepWhetblades: true, keepMaps: true });
  assert.equal(k.keepWhetblades, true);
  assert.equal(k.keepMaps, true);
});

test("the whole map is off unless asked", () => {
  assert.equal(L.request(1, L.defaults()).wholeMap, false);
  assert.equal(L.request(1, { ...L.defaults(), wholeMap: true }).wholeMap, true);
});
