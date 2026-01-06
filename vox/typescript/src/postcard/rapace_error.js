const { encodeVarint } = require("../binary/varint");

const RAPACE_ERROR = {
  USER: 0,
  UNKNOWN_METHOD: 1,
  INVALID_PAYLOAD: 2,
  CANCELLED: 3,
};

function encodeUnknownMethod() {
  return encodeVarint(RAPACE_ERROR.UNKNOWN_METHOD);
}

function encodeInvalidPayload() {
  return encodeVarint(RAPACE_ERROR.INVALID_PAYLOAD);
}

module.exports = {
  RAPACE_ERROR,
  encodeUnknownMethod,
  encodeInvalidPayload,
};

