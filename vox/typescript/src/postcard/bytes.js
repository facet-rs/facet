const { decodeVarintNumber } = require("../binary/varint");

function decodeBytes(buf, offset) {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("bytes: overrun");
  return { value: buf.subarray(start, end), next: end };
}

module.exports = { decodeBytes };

