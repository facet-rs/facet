import { afterEach, describe, expect, it } from "vitest";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import type { Socket } from "node:net";
import { LocalLink, LocalLinkAcceptor } from "./transport.ts";
import { LengthPrefixedFramed } from "./framing.ts";

const LINK_PROLOGUE = Buffer.from([0x56, 0x4f, 0x58, 0x4c, 1, 0]);

const cleanups: Array<() => Promise<void>> = [];

type SocketHandler = (...args: unknown[]) => void;

class CapturingSocket {
  readonly writes: Buffer[] = [];
  private readonly handlers = new Map<string, SocketHandler[]>();

  on(event: string, handler: SocketHandler): this {
    const handlers = this.handlers.get(event) ?? [];
    handlers.push(handler);
    this.handlers.set(event, handlers);
    return this;
  }

  write(payload: Buffer, _callback?: (error?: Error) => void): boolean {
    this.writes.push(Buffer.from(payload));
    return true;
  }

  destroy(error?: Error): void {
    if (error) {
      this.emit("error", error);
    }
    this.emit("close");
  }

  emit(event: string, ...args: unknown[]): void {
    for (const handler of this.handlers.get(event) ?? []) {
      handler(...args);
    }
  }
}

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) {
    await cleanup();
  }
});

async function makeLocalEndpoint(): Promise<{ addr: string; cleanup: () => Promise<void> }> {
  if (process.platform === "win32") {
    return {
      addr: `\\\\.\\pipe\\vox-framing-${randomUUID()}`,
      cleanup: async () => {},
    };
  }

  const dir = await mkdtemp(join(tmpdir(), "vox-framing-"));
  return {
    addr: join(dir, "sock"),
    cleanup: () => rm(dir, { recursive: true, force: true }),
  };
}

describe("LengthPrefixedFramed", () => {
  // r[verify link.tx.alloc.limits]
  it("rejects frames that cannot fit in the u32 length prefix before writing", async () => {
    const socket = new CapturingSocket();
    const link = new LengthPrefixedFramed(socket as unknown as Socket);

    await expect(
      link.send({ length: 0x1_0000_0000 } as Uint8Array),
    ).rejects.toThrow("frame too large for u32 length prefix");
    expect(socket.writes).toEqual([LINK_PROLOGUE]);
  });

  // r[verify link.tx.cancel-safe]
  it("hands the socket one complete frame per send", () => {
    const socket = new CapturingSocket();
    const link = new LengthPrefixedFramed(socket as unknown as Socket);

    void link.send(Uint8Array.of(1, 2, 3));

    expect(socket.writes).toHaveLength(2);
    expect(socket.writes[0]).toEqual(LINK_PROLOGUE);
    expect(socket.writes[1]).toEqual(Buffer.from([3, 0, 0, 0, 1, 2, 3]));
    link.close();
  });

  // r[verify link.rx.error]
  it("treats a receive error as terminal", async () => {
    const socket = new CapturingSocket();
    const link = new LengthPrefixedFramed(socket as unknown as Socket);
    const pending = link.recv();

    socket.emit("error", new Error("boom"));

    await expect(pending).rejects.toThrow("boom");
    await expect(link.recv()).resolves.toBeNull();
  });

  // r[verify rpc.transport.stream.cancel-safe-recv]
  it("keeps partial frames in transport-owned state after recv timeout", async () => {
    const endpoint = await makeLocalEndpoint();
    cleanups.push(endpoint.cleanup);
    const acceptor = await LocalLinkAcceptor.bind(endpoint.addr);

    try {
      const accepted = acceptor.nextLink();
      const client = await LocalLink.connect(endpoint.addr);
      const server = (await accepted).link;
      const socket = server.getSocket();
      const frame = Buffer.alloc(7);
      frame.writeUInt32LE(3, 0);
      frame.set(Uint8Array.of(7, 8, 9), 4);

      socket.write(frame.subarray(0, 5));
      await expect(client.recvTimeout(5)).resolves.toBeNull();
      socket.write(frame.subarray(5));

      await expect(client.recv()).resolves.toEqual(Uint8Array.of(7, 8, 9));

      client.close();
      server.close();
    } finally {
      await acceptor.close();
    }
  });
});
