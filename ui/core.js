// Talking to the module: one request in, JSON (and maybe bytes) out, and
// the loop that hands it the files it asks for.

/** Split an answer into its JSON and the bytes after it. */
export function decode(buffer) {
  const view = new DataView(buffer);
  const n = view.getUint32(0, true);
  const json = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 4, n)));
  return { json, bytes: new Uint8Array(buffer, 4 + n) };
}

/** A caller for an instantiated module's exports. */
export function caller(ex) {
  return (req, data = new Uint8Array(0)) => {
    const enc = new TextEncoder().encode(JSON.stringify(req));
    const rp = ex.alloc(enc.length);
    new Uint8Array(ex.memory.buffer, rp, enc.length).set(enc);
    const dp = ex.alloc(data.length);
    new Uint8Array(ex.memory.buffer, dp, data.length).set(data);
    const r = ex.call(rp, enc.length, dp, data.length);
    ex.dealloc(rp, enc.length);
    ex.dealloc(dp, data.length);
    const ptr = Number(r >> 32n);
    const len = Number(r & 0xffffffffn);
    const copy = new Uint8Array(ex.memory.buffer, ptr, len).slice();
    ex.dealloc(ptr, len);
    return decode(copy.buffer);
  };
}

/**
 * Ask until the module has every file it wants: `read(path)` gives a
 * file's bytes, or null when there is none. `call` may answer at once or
 * with a promise (the module in a worker). `onRead(done, of)` hears each
 * file fetched, counted over every round so far; `stopped()` ends the loop.
 */
export async function until(call, req, read, onRead = () => {}, stopped = () => false) {
  const asked = new Set();
  let done = 0;
  for (;;) {
    if (stopped()) throw new Stopped();
    const a = await call(req);
    if (!a.json.need) return a;
    // A file asked for twice was not taken: stop rather than loop.
    const again = a.json.need.filter((p) => asked.has(p));
    if (again.length) throw new Error("The module keeps asking for " + again.join(", "));
    a.json.need.forEach((p) => asked.add(p));
    for (const p of a.json.need) {
      if (stopped()) throw new Stopped();
      const b = await read(p);
      if (b) await call({ op: "put", path: p }, new Uint8Array(b));
      else await call({ op: "put", path: p, absent: true });
      onRead(++done, asked.size);
    }
  }
}

/** The files a run gave back, as `{ path, bytes }`. */
export function outputs(a) {
  let at = 0;
  return a.json.files.map((f) => {
    const bytes = a.bytes.slice(at, at + f.size);
    at += f.size;
    return { path: f.path, bytes };
  });
}

/** Thrown when a loop is stopped part way. */
export class Stopped extends Error {
  constructor() {
    super("Stopped");
  }
}

/**
 * The module in a worker, so a run leaves the page free to draw: a caller
 * whose answers are promises. Falls back to the page itself when a worker
 * cannot be made.
 */
export async function workerCaller(url) {
  try {
    const w = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
    let next = 1;
    const waiting = new Map();
    w.onmessage = (e) => {
      const { id, json, bytes, error } = e.data;
      const p = waiting.get(id);
      waiting.delete(id);
      if (error) p.reject(new Error(error));
      else p.resolve({ json, bytes: new Uint8Array(bytes) });
    };
    let dead = null;
    w.onerror = (e) => {
      dead = new Error(e.message || "The module stopped.");
      w.terminate();
      for (const p of waiting.values()) p.reject(new Error(e.message || "The module stopped."));
      waiting.clear();
    };
    const call = (req, data = new Uint8Array(0)) =>
      new Promise((resolve, reject) => {
        if (dead) return reject(dead);
        const id = next++;
        waiting.set(id, { resolve, reject });
        const copy = data.slice();
        w.postMessage({ id, req, data: copy.buffer, url: String(url) }, [copy.buffer]);
      });
    await call({ op: "files", paths: [] });
    return call;
  } catch {
    const { instance } = await WebAssembly.instantiateStreaming(fetch(url));
    return caller(instance.exports);
  }
}
