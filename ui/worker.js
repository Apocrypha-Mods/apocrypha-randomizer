// The module, away from the page: a run takes long enough to stall drawing.
import { caller } from "./core.js";

let ready = null;
self.onmessage = async (e) => {
  const { id, req, data, url } = e.data;
  try {
    ready ||= WebAssembly.instantiateStreaming(fetch(new URL(url, self.location.href))).then((r) => caller(r.instance.exports));
    const call = await ready;
    const a = call(req, new Uint8Array(data));
    const bytes = a.bytes.slice().buffer;
    self.postMessage({ id, json: a.json, bytes }, [bytes]);
  } catch (err) {
    self.postMessage({ id, error: String(err && err.message ? err.message : err) });
  }
};
