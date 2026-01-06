const { encodeVarint } = require("./varint");

function concat(...parts) {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

function encodeString(str) {
  const bytes = new TextEncoder().encode(str);
  return concat(encodeVarint(bytes.length), bytes);
}

function encodeBytes(bytes) {
  return concat(encodeVarint(bytes.length), bytes);
}

module.exports = {
  concat,
  encodeString,
  encodeBytes,
};

