import type { Link } from "./link.ts";
import type { SessionConduitKind } from "./session.ts";

const TRANSPORT_HELLO_MAGIC = new Uint8Array([0x52, 0x4f, 0x54, 0x48]); // ROTH
const TRANSPORT_ACCEPT_MAGIC = new Uint8Array([0x52, 0x4f, 0x54, 0x41]); // ROTA
const TRANSPORT_REJECT_MAGIC = new Uint8Array([0x52, 0x4f, 0x54, 0x52]); // ROTR
const TRANSPORT_VERSION = 9;
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

function encodeHello(requestedMode: SessionConduitKind): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_HELLO_MAGIC,
    TRANSPORT_VERSION,
    requestedMode === "stable" ? 1 : 0,
    0,
    0,
  ]);
}

function encodeAccept(selectedMode: SessionConduitKind): Uint8Array {
  return new Uint8Array([
    ...TRANSPORT_ACCEPT_MAGIC,
    TRANSPORT_VERSION,
    selectedMode === "stable" ? 1 : 0,
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

function decodeMode(byte: number): SessionConduitKind {
  switch (byte) {
    case 0:
      return "bare";
    case 1:
      return "stable";
    default:
      throw new TransportPrologueError(`unknown conduit mode ${byte}`);
  }
}

export async function requestTransportMode(
  link: Link,
  requestedMode: SessionConduitKind,
): Promise<void> {
  await link.send(encodeHello(requestedMode));
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
    const selectedMode = decodeMode(response[5] ?? 255);
    if (selectedMode !== requestedMode) {
      throw new TransportPrologueError(
        `transport selected ${selectedMode}, requested ${requestedMode}`,
      );
    }
    return;
  }
  if (sameBytes(response.subarray(0, 4), TRANSPORT_REJECT_MAGIC)) {
    const reason = response[5] ?? 0;
    if (reason === REJECT_UNSUPPORTED_MODE) {
      throw new TransportPrologueError(`transport rejected unsupported mode ${requestedMode}`);
    }
    throw new TransportPrologueError(`transport rejected with reason ${reason}`);
  }
  throw new TransportPrologueError("expected TransportAccept or TransportReject");
}

export async function acceptTransportMode(link: Link): Promise<SessionConduitKind> {
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
  const requestedMode = decodeMode(hello[5] ?? 255);
  await link.send(encodeAccept(requestedMode));
  return requestedMode;
}
