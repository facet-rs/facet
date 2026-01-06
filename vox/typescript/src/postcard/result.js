const { encodeVarint } = require("../binary/varint");
const { concat } = require("../binary/bytes");

function encodeResultOk(okBytes) {
  return concat(encodeVarint(0), okBytes);
}

function encodeResultErr(errBytes) {
  return concat(encodeVarint(1), errBytes);
}

module.exports = {
  encodeResultOk,
  encodeResultErr,
};

