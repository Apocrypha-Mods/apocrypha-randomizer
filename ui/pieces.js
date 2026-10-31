// A piece of an archive, as the module names one: `Data0.bdt#offset+length`.
export function pieceOf(path) {
  const m = /^([^#]+)#(\d+)\+(\d+)$/.exec(path);
  return m ? { file: m[1], at: Number(m[2]), length: Number(m[3]) } : null;
}
