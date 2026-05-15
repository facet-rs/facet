import type { Link } from "./link.ts";

const TRANSPORT_HELLO_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x48]); // VOTH
const TRANSPORT_ACCEPT_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x41]); // VOTA
const TRANSPORT_REJECT_MAGIC = new Uint8Array([0x56, 0x4F, 0x54, 0x52]); // VOTR
const TRANSPORT_VERSION = 9;
const TRANSPORT_MODE_BARE = 0;
const REJECT_UNSUPPORTED_MODE = 1;

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
    TRANSPORT_MODE_BARE,
    0,
    0,
  ]);
}

function encodeAccept(): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_ACCEPT_MAGIC,
    TRANSPORT_VERSION,
    TRANSPORT_MODE_BARE,
    0,
    0,
  ]);
}

function encodeReject(reason = REJECT_UNSUPPORTED_MODE): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_REJECT_MAGIC,
    TRANSPORT_VERSION,
    reason,
    0,
    0,
  ]);
}

export async function requestTransportMode(link: Link): Promise<void> {
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
    if ((response[5] ?? 255) !== TRANSPORT_MODE_BARE) {
      throw new TransportPrologueError(`unknown conduit mode ${response[5] ?? 255}`);
    }
    return;
  }
  if (sameBytes(response.subarray(0, 4), TRANSPORT_REJECT_MAGIC)) {
    const reason = response[5] ?? 0;
    if (reason === REJECT_UNSUPPORTED_MODE) {
      throw new TransportPrologueError("transport rejected unsupported mode bare");
    }
    throw new TransportPrologueError(`transport rejected with reason ${reason}`);
  }
  throw new TransportPrologueError("expected TransportAccept or TransportReject");
}

export async function acceptTransportMode(link: Link): Promise<void> {
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
  if ((hello[5] ?? 255) !== TRANSPORT_MODE_BARE) {
    await link.send(encodeReject());
    throw new TransportPrologueError(`unknown conduit mode ${hello[5] ?? 255}`);
  }
  await link.send(encodeAccept());
}
