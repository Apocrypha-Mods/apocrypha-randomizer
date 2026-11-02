#!/usr/bin/env node
// Runs the add-on's module against a game folder the way the page does, without a
// window, and writes what it gives back: the proof that the module builds what
// apoc-rando builds. node tools/addon-run.mjs <module.wasm> <game> <seed> <out> [enemies] [options.json]
// options.json holds more of the request, and `mods`: [{ name, dir }], lowest priority first,
// standing in for what the mod manager says the other mods hold.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, openSync, readdirSync, readFileSync, readSync, closeSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { caller, outputs, until } from "../ui/core.js";
import { pieceOf } from "../ui/pieces.js";
import { stackOf } from "../ui/logic.js";

const [wasm, game, seed, out, enemies = "", more] = process.argv.slice(2);
const { mods = [], ...options } = more ? JSON.parse(readFileSync(more, "utf8")) : {};
const walk = (dir) =>
  readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
// The later mod's copy of a file wins, as the mod manager's plan has it.
const held = new Map();
for (const m of mods) {
  for (const p of walk(m.dir)) held.set(relative(m.dir, p).toLowerCase().replaceAll("\\", "/"), { path: relative(m.dir, p), file: p });
}
const copies = mods.filter((m) => existsSync(join(m.dir, "regulation.bin")));
const stack = stackOf({
  files: [...held.values()].map((f) => ({ path: f.path, size: statSync(f.file).size })),
  merged: copies.length ? [{ path: "regulation.bin", copies: copies.map((m) => ({ mod: m.name })) }] : [],
  mods: mods.map((m) => m.name),
});
const { instance } = await WebAssembly.instantiate(readFileSync(wasm));
const call = caller(instance.exports);
const read = (path) => {
  if (path.startsWith("mod:")) return readFileSync(held.get(path.slice(4).toLowerCase().replaceAll("\\", "/")).file);
  if (path.startsWith("mod#")) return readFileSync(join(copies[Number(path.slice(4))].dir, "regulation.bin"));
  const p = pieceOf(path);
  if (!p) return readFileSync(join(game, path));
  const f = openSync(join(game, p.file), "r");
  const bytes = Buffer.alloc(p.length);
  readSync(f, bytes, 0, p.length, p.at);
  closeSync(f);
  return bytes;
};
const started = Date.now();
const checked = (req, data) => {
  const a = call(req, data);
  if (a.json.error) throw new Error(a.json.error);
  return a;
};
let shown = 0;
const a = await until(checked, { op: "run", seed: Number(seed), enemies, ...options, ...(mods.length ? { stack } : {}) }, read, (done, of) => {
  if (done - shown >= 500 || done === of) console.error(`read ${(shown = done)} of ${of}`);
});
for (const f of outputs(a)) {
  const to = join(out, f.path);
  mkdirSync(dirname(to), { recursive: true });
  writeFileSync(to, f.bytes);
  console.log(createHash("sha256").update(f.bytes).digest("hex").slice(0, 16), f.path);
}
writeFileSync(`${out}.spoiler.txt`, a.json.spoiler);
const peak = instance.exports.memory.buffer.byteLength / 2 ** 30;
console.error(`${a.json.slots} slots, ${((Date.now() - started) / 1000).toFixed(0)} s, module memory ${peak.toFixed(2)} GiB`);
