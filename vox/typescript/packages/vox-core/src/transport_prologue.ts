import type { Link } from "./link.ts";

const TRANSPORT_HELLO_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x48]); // VOTH
const TRANSPORT_ACCEPT_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x41]); // VOTA
const TRANSPORT_REJECT_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x52]); // VOTR
const TRANSPORT_VERSION = 9;
const TRANSPORT_RESERVED_ZERO = new Uint8Array([0, 0, 0]);
const REJECT_UNSUPPORTED_PROLOGUE = 1;

export class TransportPrologueError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportPrologueError";
  }
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) {
    return false;
  }
  for (let i = 0; i < left.length; i++) {
    if (left[i] !== right[i]) {
      return false;
    }
  }
  return true;
}

function encodeHello(): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_HELLO_MAGIC,
    TRANSPORT_VERSION,
    ...TRANSPORT_RESERVED_ZERO,
  ]);
}

function encodeAccept(): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_ACCEPT_MAGIC,
    TRANSPORT_VERSION,
    ...TRANSPORT_RESERVED_ZERO,
  ]);
}

function encodeReject(reason = REJECT_UNSUPPORTED_PROLOGUE): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_REJECT_MAGIC,
    TRANSPORT_VERSION,
    reason,
    0,
    0,
  ]);
}

function reservedBytesAreZero(bytes: Uint8Array): boolean {
  return sameBytes(bytes, TRANSPORT_RESERVED_ZERO);
}

// r[impl transport.prologue]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
export async function performInitiatorTransportPrologue(link: Link): Promise<void> {
  await link.send(encodeHello());
  const response = await link.recv();
  if (!response) {
    throw new TransportPrologueError("transport closed during prologue");
  }
  if (response.length !== 8) {
    throw new TransportPrologueError("invalid transport prologue response size");
  }
  if (sameBytes(response.subarray(0, 4), TRANSPORT_ACCEPT_MAGIC)) {
    if ((response[4] ?? 0) !== TRANSPORT_VERSION) {
      throw new TransportPrologueError(`unsupported transport version ${response[4] ?? 0}`);
    }
    if (!reservedBytesAreZero(response.subarray(5, 8))) {
      throw new TransportPrologueError("transport accept reserved bytes must be zero");
    }
    return;
  }
  if (sameBytes(response.subarray(0, 4), TRANSPORT_REJECT_MAGIC)) {
    const reason = response[5] ?? 0;
    if (reason === REJECT_UNSUPPORTED_PROLOGUE) {
      throw new TransportPrologueError("transport rejected unsupported prologue");
    }
    throw new TransportPrologueError(`transport rejected with reason ${reason}`);
  }
  throw new TransportPrologueError("expected TransportAccept or TransportReject");
}

// r[impl transport.prologue]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
export async function performAcceptorTransportPrologue(link: Link): Promise<void> {
  const hello = await link.recv();
  if (!hello) {
    throw new TransportPrologueError("transport closed before prologue");
  }
  if (hello.length !== 8 || !sameBytes(hello.subarray(0, 4), TRANSPORT_HELLO_MAGIC)) {
    throw new TransportPrologueError("invalid TransportHello");
  }
  if ((hello[4] ?? 0) !== TRANSPORT_VERSION) {
    await link.send(encodeReject());
    throw new TransportPrologueError(`unsupported transport version ${hello[4] ?? 0}`);
  }
  if (!reservedBytesAreZero(hello.subarray(5, 8))) {
    await link.send(encodeReject());
    throw new TransportPrologueError("transport hello reserved bytes must be zero");
  }
  await link.send(encodeAccept());
}
