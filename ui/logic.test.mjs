import assert from "node:assert/strict";
import { test } from "node:test";
import * as L from "./logic.js";

test("ordinary enemies never move", () => {
  const r = L.request(1, { ...L.defaults(), bosses: 60 });
  assert.equal(r.enemies, "bosses=60,ordinary=0,mix=0");
  assert.equal(L.request(1, L.defaults()).enemies, "");
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
