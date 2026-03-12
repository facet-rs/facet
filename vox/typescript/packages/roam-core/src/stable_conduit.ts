import {
  decodeWithSchema,
  encodeWithSchema,
  type Schema,
  type SchemaRegistry,
} from "@bearcove/roam-postcard";
import {
  MessageSchema,
  decodeMessage,
  encodeMessage,
  type Message,
  wireSchemaRegistry,
} from "@bearcove/roam-wire";
import type { Conduit } from "./conduit.ts";
import type { Link, LinkSource } from "./link.ts";

const CLIENT_HELLO_MAGIC = new Uint8Array([0x52, 0x4f, 0x43, 0x48]); // ROCH
const SERVER_HELLO_MAGIC = new Uint8Array([0x52, 0x4f, 0x53, 0x48]); // ROSH

const CH_HAS_RESUME_KEY = 1 << 0;
const CH_HAS_LAST_RECEIVED = 1 << 1;
const SH_REJECTED = 1 << 0;
const SH_HAS_LAST_RECEIVED = 1 << 1;

interface PacketAck {
  max_delivered: number;
}

interface StableFrame {
  seq: number;
  ack: PacketAck | null;
  item: Message;
}

const FRAME_SCHEMA: Schema = {
  kind: "struct",
  fields: {
    seq: { kind: "u32" },
    ack: {
      kind: "option",
      inner: {
        kind: "struct",
        fields: {
          max_delivered: { kind: "u32" },
        },
      },
    },
    item: MessageSchema,
  },
};

const FRAME_SCHEMA_REGISTRY: SchemaRegistry = wireSchemaRegistry;

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

function u32ToBytes(value: number): Uint8Array {
  const out = new Uint8Array(4);
  const view = new DataView(out.buffer, out.byteOffset, out.byteLength);
  view.setUint32(0, value, true);
  return out;
}

function bytesToU32(bytes: Uint8Array, offset: number): number {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return view.getUint32(offset, true);
}

function randomResumeKey(): Uint8Array {
  const key = new Uint8Array(16);
  crypto.getRandomValues(key);
  return key;
}

function encodeClientHello(
  resumeKey: Uint8Array | null,
  lastReceived: number | null,
): Uint8Array {
  const out = new Uint8Array(25);
  out.set(CLIENT_HELLO_MAGIC, 0);
  let flags = 0;
  if (resumeKey) {
    flags |= CH_HAS_RESUME_KEY;
  }
  if (lastReceived !== null) {
    flags |= CH_HAS_LAST_RECEIVED;
  }
  out[4] = flags;
  out.set(resumeKey ?? new Uint8Array(16), 5);
  out.set(u32ToBytes(lastReceived ?? 0), 21);
  return out;
}

function decodeClientHello(bytes: Uint8Array): { resumeKey: Uint8Array | null; lastReceived: number | null } {
  if (bytes.length !== 25 || !sameBytes(bytes.subarray(0, 4), CLIENT_HELLO_MAGIC)) {
    throw new Error("invalid StableConduit ClientHello");
  }

  const flags = bytes[4] ?? 0;
  const resumeKey = (flags & CH_HAS_RESUME_KEY) !== 0 ? bytes.slice(5, 21) : null;
  const lastReceived = (flags & CH_HAS_LAST_RECEIVED) !== 0 ? bytesToU32(bytes, 21) : null;
  return { resumeKey, lastReceived };
}

function encodeServerHello(
  resumeKey: Uint8Array,
  lastReceived: number | null,
  rejected: boolean,
): Uint8Array {
  const out = new Uint8Array(25);
  out.set(SERVER_HELLO_MAGIC, 0);
  let flags = 0;
  if (rejected) {
    flags |= SH_REJECTED;
  }
  if (lastReceived !== null) {
    flags |= SH_HAS_LAST_RECEIVED;
  }
  out[4] = flags;
  out.set(resumeKey, 5);
  out.set(u32ToBytes(lastReceived ?? 0), 21);
  return out;
}

function decodeServerHello(bytes: Uint8Array): {
  resumeKey: Uint8Array;
  lastReceived: number | null;
  rejected: boolean;
} {
  if (bytes.length !== 25 || !sameBytes(bytes.subarray(0, 4), SERVER_HELLO_MAGIC)) {
    throw new Error("invalid StableConduit ServerHello");
  }

  const flags = bytes[4] ?? 0;
  return {
    resumeKey: bytes.slice(5, 21),
    lastReceived: (flags & SH_HAS_LAST_RECEIVED) !== 0 ? bytesToU32(bytes, 21) : null,
    rejected: (flags & SH_REJECTED) !== 0,
  };
}

interface ReplayEntry {
  seq: number;
  item: Uint8Array;
}

class ReplayBuffer {
  private entries: ReplayEntry[] = [];

  push(seq: number, item: Uint8Array): void {
    this.entries.push({ seq, item: item.slice() });
  }

  trim(maxDelivered: number): void {
    while (this.entries[0] && this.entries[0].seq <= maxDelivered) {
      this.entries.shift();
    }
  }

  snapshot(): ReplayEntry[] {
    return this.entries.map((entry) => ({ seq: entry.seq, item: entry.item.slice() }));
  }
}

export class StableConduit implements Conduit<Message> {
  private link: Link | null = null;
  private readonly recvWaiters: Array<(value: Message | null) => void> = [];
  private readonly recvQueue: Message[] = [];
  private readonly replay = new ReplayBuffer();
  private reconnecting: Promise<void> | null = null;
  private resumeKey: Uint8Array | null = null;
  private nextSendSeq = 0;
  private lastReceived: number | null = null;
  private closed = false;
  private recvLoopPromise: Promise<void> | null = null;

  constructor(private readonly source: LinkSource) {}

  static async connect(source: LinkSource): Promise<StableConduit> {
    // r[impl stable]
    // r[impl stable.link-source]
    const conduit = new StableConduit(source);
    await conduit.attachFreshLink();
    conduit.ensureRecvLoop();
    return conduit;
  }

  async send(item: Message): Promise<void> {
    // r[impl stable.framing]
    // r[impl stable.ack]
    // r[impl stable.replay-buffer]
    const itemBytes = encodeMessage(item);
    while (!this.closed) {
      try {
        const link = await this.requireLink();
        const seq = this.nextSendSeq++;
        const frame: StableFrame = {
          seq,
          ack: this.lastReceived === null ? null : { max_delivered: this.lastReceived },
          item,
        };
        this.replay.push(seq, itemBytes);
        await link.send(encodeWithSchema(frame, FRAME_SCHEMA, FRAME_SCHEMA_REGISTRY));
        return;
      } catch {
        await this.ensureReconnected();
      }
    }

    throw new Error("StableConduit closed");
  }

  async recv(): Promise<Message | null> {
    // r[impl stable.seq.monotonic]
    // r[impl stable.ack.trim]
    const queued = this.recvQueue.shift();
    if (queued) {
      return queued;
    }

    if (this.closed) {
      return null;
    }

    return new Promise((resolve) => {
      this.recvWaiters.push(resolve);
    });
  }

  close(): void {
    this.closed = true;
    this.link?.close();
    this.link = null;
    for (const waiter of this.recvWaiters.splice(0)) {
      waiter(null);
    }
    this.recvQueue.length = 0;
  }

  isClosed(): boolean {
    return this.closed;
  }

  private async attachFreshLink(): Promise<void> {
    // r[impl stable.handshake]
    // r[impl stable.reconnect]
    // r[impl stable.reconnect.server-replay]
    // r[impl stable.reconnect.client-replay]
    // r[impl stable.replay-buffer.order]
    // r[impl retry.reconnect.stable-conduit]
    const attachment = await this.source.nextLink();
    const link = attachment.link;

    if (attachment.clientHello) {
      const client = decodeClientHello(attachment.clientHello);
      const resumeKey = randomResumeKey();
      await link.send(encodeServerHello(resumeKey, this.lastReceived, false));
      this.resumeKey = resumeKey;
      this.link = link;

      const replayEntries = this.replay.snapshot();
      for (const entry of replayEntries) {
        if (client.lastReceived !== null && entry.seq <= client.lastReceived) {
          continue;
        }
        const frame: StableFrame = {
          seq: entry.seq,
          ack: this.lastReceived === null ? null : { max_delivered: this.lastReceived },
          item: decodeMessage(entry.item).value,
        };
        await link.send(encodeWithSchema(frame, FRAME_SCHEMA, FRAME_SCHEMA_REGISTRY));
      }
      return;
    }

    await link.send(encodeClientHello(this.resumeKey, this.lastReceived));
    const rawHello = await link.recv();
    if (!rawHello) {
      throw new Error("StableConduit handshake failed");
    }
    const server = decodeServerHello(rawHello);
    if (server.rejected) {
      throw new Error("StableConduit session lost");
    }
    this.resumeKey = server.resumeKey;
    this.link = link;

    const replayEntries = this.replay.snapshot();
    for (const entry of replayEntries) {
      if (server.lastReceived !== null && entry.seq <= server.lastReceived) {
        continue;
      }
      const frame: StableFrame = {
        seq: entry.seq,
        ack: this.lastReceived === null ? null : { max_delivered: this.lastReceived },
        item: decodeMessage(entry.item).value,
      };
      await link.send(encodeWithSchema(frame, FRAME_SCHEMA, FRAME_SCHEMA_REGISTRY));
    }
  }

  private async requireLink(): Promise<Link> {
    if (this.closed) {
      throw new Error("StableConduit closed");
    }
    if (this.link) {
      return this.link;
    }
    await this.ensureReconnected();
    if (!this.link) {
      throw new Error("StableConduit link unavailable");
    }
    return this.link;
  }

  private ensureRecvLoop(): void {
    if (this.recvLoopPromise) {
      return;
    }

    this.recvLoopPromise = (async () => {
      while (!this.closed) {
        try {
          const link = await this.requireLink();
          const payload = await link.recv();
          if (!payload) {
            await this.ensureReconnected();
            continue;
          }

          const decoded = decodeWithSchema(
            payload,
            0,
            FRAME_SCHEMA,
            FRAME_SCHEMA_REGISTRY,
          );
          const frame = decoded.value as StableFrame;
          if (frame.ack) {
            this.replay.trim(frame.ack.max_delivered);
          }
          if (this.lastReceived !== null && frame.seq <= this.lastReceived) {
            continue;
          }

          this.lastReceived = frame.seq;
          const waiter = this.recvWaiters.shift();
          if (waiter) {
            waiter(frame.item);
          } else {
            this.recvQueue.push(frame.item);
          }
        } catch {
          await this.ensureReconnected();
        }
      }
    })().finally(() => {
      this.recvLoopPromise = null;
    });
  }

  private async ensureReconnected(): Promise<void> {
    // r[impl stable.reconnect]
    // r[impl stable.reconnect.failure]
    if (this.closed) {
      throw new Error("StableConduit closed");
    }
    if (this.reconnecting) {
      await this.reconnecting;
      return;
    }

    this.link?.close();
    this.link = null;
    this.reconnecting = this.attachFreshLink().finally(() => {
      this.reconnecting = null;
    });
    await this.reconnecting;
  }
}
