const { decodeVarintNumber } = require("../binary/varint");

function decodeString(buf, offset) {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("string: overrun");
  const s = new TextDecoder().decode(buf.subarray(start, end));
  return { value: s, next: end };
}

module.exports = { decodeString };

