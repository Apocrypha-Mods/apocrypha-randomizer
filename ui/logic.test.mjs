import assert from "node:assert/strict";
import { test } from "node:test";
import * as L from "./logic.js";

test("enemies move only when asked", () => {
  assert.equal(L.request(1, L.defaults()).enemies, "");
  assert.equal(L.request(1, { ...L.defaults(), bosses: 60 }).enemies, "bosses=60,ordinary=0,mix=0");
  assert.equal(L.request(1, { ...L.defaults(), ordinary: 40 }).enemies, "bosses=0,ordinary=40,mix=0");
  assert.equal(L.request(1, { ...L.defaults(), bosses: 100, ordinary: 100, mix: 10 }).enemies, "bosses=100,ordinary=100,mix=10");
});
test("a mix with no bosses moves nothing", () => {
  assert.equal(L.request(1, { ...L.defaults(), mix: 50 }).enemies, "");
  assert.equal(L.request(1, { ...L.defaults(), ordinary: 10, mix: 50 }).enemies, "bosses=0,ordinary=10,mix=0");
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
  assert.equal(r.shuffle.split(",").length, L.CATEGORIES.length);
});
