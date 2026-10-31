#!/usr/bin/env node
// Runs the add-on's module against a game folder the way the page does, without a
// window, and writes what it gives back: the proof that the module builds what
// apoc-rando builds. node tools/addon-run.mjs <module.wasm> <game> <seed> <out> [enemies]
import { createHash } from "node:crypto";
import { mkdirSync, openSync, readFileSync, readSync, closeSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { caller, outputs, until } from "../ui/core.js";
import { pieceOf } from "../ui/pieces.js";

const [wasm, game, seed, out, enemies = ""] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(wasm));
const call = caller(instance.exports);
const read = (path) => {
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
const a = await until(checked, { op: "run", seed: Number(seed), enemies }, read, (done, of) => {
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
