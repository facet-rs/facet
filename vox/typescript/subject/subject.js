// Node subject for the compliance suite.
//
// The harness sets PEER_ADDR (e.g. "127.0.0.1:1234"). We connect, immediately
// send Hello, then enforce a small subset of the spec needed by the initial
// compliance tests.

const { encodeVarint, decodeVarint, decodeVarintNumber } = require("../src/binary/varint");
const { concat, encodeString, encodeBytes } = require("../src/binary/bytes");
const { cobsEncode, cobsDecode } = require("../src/binary/cobs");
const { decodeString } = require("../src/postcard/string");
const { decodeBytes } = require("../src/postcard/bytes");
const { encodeResultOk, encodeResultErr } = require("../src/postcard/result");
const { encodeUnknownMethod, encodeInvalidPayload } = require("../src/postcard/rapace_error");

function die(message) {
  console.error(message);
  process.exit(1);
}

const peerAddr = process.env.PEER_ADDR;
if (!peerAddr) die("PEER_ADDR is not set");

const lastColon = peerAddr.lastIndexOf(":");
if (lastColon < 0) die(`Invalid PEER_ADDR ${peerAddr}`);
const host = peerAddr.slice(0, lastColon);
const port = Number(peerAddr.slice(lastColon + 1));
if (!Number.isFinite(port) || port <= 0 || port > 65535) die(`Invalid PEER_ADDR port in ${peerAddr}`);

// Postcard encoding for the specific Message subset we need.
const LOCAL_MAX_PAYLOAD = 1024 * 1024;
const LOCAL_INITIAL_CREDIT = 64 * 1024;

function encodeHello(maxPayloadSize, initialStreamCredit) {
  // Message::Hello (0), Hello::V1 (0)
  return concat(
    encodeVarint(0),
    encodeVarint(0),
    encodeVarint(maxPayloadSize),
    encodeVarint(initialStreamCredit),
  );
}

function encodeGoodbye(reason) {
  // Message::Goodbye (1)
  return concat(encodeVarint(1), encodeString(reason));
}

function encodeResponse(requestId, payloadBytes) {
  // Message::Response (3)
  // Response { request_id: u64, metadata: Vec<(String, MetadataValue)>, payload: bytes }
  return concat(
    encodeVarint(3),
    encodeVarint(requestId),
    encodeVarint(0), // empty metadata vec
    encodeBytes(payloadBytes),
  );
}

function frame(payload) {
  const encoded = cobsEncode(payload);
  return concat(encoded, Uint8Array.from([0x00]));
}

function sendMsg(socket, payload) {
  socket.write(frame(payload));
}

let negotiatedMaxPayload = LOCAL_MAX_PAYLOAD;
let haveSentHello = false;
let haveReceivedHello = false;

const METHOD_ID = {
  Echo: {
    echo: 0x3d66dd9ee36b4240n,
    reverse: 0x268246d3219503fbn,
  },
};

function handleRequest(socket, requestId, methodId, payloadBytes) {
  // Enforce handshake ordering (spec: message.hello.ordering).
  if (!haveSentHello || !haveReceivedHello) return;

  if (methodId === METHOD_ID.Echo.echo) {
    try {
      // args payload is the Postcard encoding of a tuple of args in declaration order.
      // For one arg, that's a 1-tuple: `(String,)` encoded as `String`.
      const msg = decodeString(payloadBytes, 0);
      if (msg.next !== payloadBytes.length) throw new Error("args: trailing bytes");
      const resultPayload = encodeResultOk(encodeString(msg.value));
      sendMsg(socket, encodeResponse(requestId, resultPayload));
    } catch (_e) {
      const resultPayload = encodeResultErr(encodeInvalidPayload());
      sendMsg(socket, encodeResponse(requestId, resultPayload));
    }
    return;
  }

  if (methodId === METHOD_ID.Echo.reverse) {
    try {
      const msg = decodeString(payloadBytes, 0);
      if (msg.next !== payloadBytes.length) throw new Error("args: trailing bytes");
      const reversed = Array.from(msg.value).reverse().join("");
      const resultPayload = encodeResultOk(encodeString(reversed));
      sendMsg(socket, encodeResponse(requestId, resultPayload));
    } catch (_e) {
      const resultPayload = encodeResultErr(encodeInvalidPayload());
      sendMsg(socket, encodeResponse(requestId, resultPayload));
    }
    return;
  }

  // Spec: unary.error.unknown-method
  const resultPayload = encodeResultErr(encodeUnknownMethod());
  sendMsg(socket, encodeResponse(requestId, resultPayload));
}

function handleMessage(socket, payload) {
  // We only decode what we need. On decode error, send Goodbye and close.
  try {
    let o = 0;
    const d0 = decodeVarintNumber(payload, o);
    const msgDisc = d0.value;
    o = d0.next;

    if (msgDisc === 0) {
      // Hello
      const d1 = decodeVarintNumber(payload, o);
      const helloDisc = d1.value;
      o = d1.next;
      if (helloDisc !== 0) {
        sendMsg(socket, encodeGoodbye("message.hello.unknown-version"));
        socket.end();
        return;
      }
      const maxPayload = decodeVarintNumber(payload, o);
      o = maxPayload.next;
      const _initialCredit = decodeVarintNumber(payload, o);
      o = _initialCredit.next;

      negotiatedMaxPayload = Math.min(LOCAL_MAX_PAYLOAD, maxPayload.value);
      haveReceivedHello = true;
      return;
    }

    // Ignore ordering violations until we have the peer hello; tests don't
    // cover this yet.
    if (!haveReceivedHello) return;

    if (msgDisc === 2) {
      // Request { request_id, method_id, metadata, payload }
      let tmp = decodeVarint(payload, o);
      const requestId = tmp.value;
      o = tmp.next;
      tmp = decodeVarint(payload, o);
      const methodId = tmp.value;
      o = tmp.next;

      // metadata: Vec<(String, MetadataValue)>
      const mdLen = decodeVarintNumber(payload, o);
      o = mdLen.next;
      for (let i = 0; i < mdLen.value; i++) {
        // key string
        const kLen = decodeVarintNumber(payload, o);
        o = kLen.next + kLen.value;
        // value enum
        const vDisc = decodeVarintNumber(payload, o);
        o = vDisc.next;
        if (vDisc.value === 0) {
          // String
          const sLen = decodeVarintNumber(payload, o);
          o = sLen.next + sLen.value;
        } else if (vDisc.value === 1) {
          // Bytes
          const bLen = decodeVarintNumber(payload, o);
          o = bLen.next + bLen.value;
        } else if (vDisc.value === 2) {
          // U64
          const u = decodeVarint(payload, o);
          o = u.next;
        } else {
          throw new Error("unknown MetadataValue");
        }
      }

      // payload: bytes
      const pLen = decodeVarintNumber(payload, o);
      o = pLen.next;
      if (pLen.value > negotiatedMaxPayload) {
        sendMsg(socket, encodeGoodbye("flow.unary.payload-limit"));
        socket.end();
        return;
      }
      const start = o;
      const end = start + pLen.value;
      if (end > payload.length) throw new Error("request payload bytes: overrun");
      const payloadBytes = payload.subarray(start, end);
      o = end;

      handleRequest(socket, requestId, methodId, payloadBytes);
      return;
    }

    if (msgDisc === 3) {
      // Response { request_id, metadata, payload }
      let tmp = decodeVarint(payload, o);
      o = tmp.next; // request_id

      const mdLen = decodeVarintNumber(payload, o);
      o = mdLen.next;
      for (let i = 0; i < mdLen.value; i++) {
        const kLen = decodeVarintNumber(payload, o);
        o = kLen.next + kLen.value;
        const vDisc = decodeVarintNumber(payload, o);
        o = vDisc.next;
        if (vDisc.value === 0) {
          const sLen = decodeVarintNumber(payload, o);
          o = sLen.next + sLen.value;
        } else if (vDisc.value === 1) {
          const bLen = decodeVarintNumber(payload, o);
          o = bLen.next + bLen.value;
        } else if (vDisc.value === 2) {
          const u = decodeVarint(payload, o);
          o = u.next;
        } else {
          throw new Error("unknown MetadataValue");
        }
      }

      const pLen = decodeVarintNumber(payload, o);
      o = pLen.next;
      if (pLen.value > negotiatedMaxPayload) {
        sendMsg(socket, encodeGoodbye("flow.unary.payload-limit"));
        socket.end();
        return;
      }
      return;
    }

    if (msgDisc === 6) {
      // Close { stream_id }
      const sid = decodeVarint(payload, o);
      if (sid.value === 0n) {
        sendMsg(socket, encodeGoodbye("streaming.id.zero-reserved"));
        socket.end();
      }
      return;
    }

    if (msgDisc === 7) {
      // Reset { stream_id }
      const sid = decodeVarint(payload, o);
      if (sid.value === 0n) {
        sendMsg(socket, encodeGoodbye("streaming.id.zero-reserved"));
        socket.end();
      }
      return;
    }
  } catch (_e) {
    try {
      sendMsg(socket, encodeGoodbye("message.decode-error"));
    } finally {
      socket.end();
    }
  }
}

async function main() {
  const net = await import("node:net");

  const socket = net.createConnection({ host, port }, () => {
    // r[message.hello.timing]: send Hello immediately after connection.
    sendMsg(socket, encodeHello(LOCAL_MAX_PAYLOAD, LOCAL_INITIAL_CREDIT));
    haveSentHello = true;
  });

  socket.on("error", (err) => {
    die(`socket error: ${err.message}`);
  });

  let buf = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    while (true) {
      const idx = buf.indexOf(0x00);
      if (idx < 0) break;
      const frameBytes = buf.subarray(0, idx);
      buf = buf.subarray(idx + 1);
      if (frameBytes.length === 0) continue;
      let decoded;
      try {
        decoded = cobsDecode(new Uint8Array(frameBytes));
      } catch (_e) {
        sendMsg(socket, encodeGoodbye("message.decode-error"));
        socket.end();
        return;
      }
      handleMessage(socket, decoded);
    }
  });

  socket.on("close", () => {
    // Exit cleanly; harness controls lifecycle.
    process.exit(0);
  });
}

main().catch((e) => die(String(e?.stack ?? e)));
