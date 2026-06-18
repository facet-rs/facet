import { afterEach, describe, expect, it } from "vitest";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { LocalLink, LocalLinkAcceptor } from "./transport.ts";

const cleanups: Array<() => Promise<void>> = [];

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) {
    await cleanup();
  }
});

async function makeLocalEndpoint(): Promise<{ addr: string; cleanup: () => Promise<void> }> {
  if (process.platform === "win32") {
    return {
      addr: `\\\\.\\pipe\\vox-test-${randomUUID()}`,
      cleanup: async () => {},
    };
  }

  const dir = await mkdtemp(join(tmpdir(), "vox-local-"));
  return {
    addr: join(dir, "sock"),
    cleanup: () => rm(dir, { recursive: true, force: true }),
  };
}

describe("LocalLink", () => {
  // r[verify transport.stream]
  // r[verify transport.stream.kinds]
  // r[verify transport.stream.local]
  it("round-trips length-prefixed frames over a local endpoint", async () => {
    const endpoint = await makeLocalEndpoint();
    cleanups.push(endpoint.cleanup);
    const acceptor = await LocalLinkAcceptor.bind(endpoint.addr);

    try {
      const accepted = acceptor.nextLink();
      const client = await LocalLink.connect(endpoint.addr);
      const server = (await accepted).link;

      await client.send(Uint8Array.of(1, 2, 3));
      await expect(server.recv()).resolves.toEqual(Uint8Array.of(1, 2, 3));

      await server.send(Uint8Array.of(4, 5, 6));
      await expect(client.recv()).resolves.toEqual(Uint8Array.of(4, 5, 6));

      client.close();
      server.close();
    } finally {
      await acceptor.close();
    }
  });
});
