import { describe, expect, it } from "vitest";
import type { Link } from "./link.ts";
import { singleLinkSource } from "./link.ts";

class TestLink implements Link {
  readonly lastReceived = undefined;

  async send(_: Uint8Array): Promise<void> {}

  async recv(): Promise<Uint8Array | null> {
    return null;
  }

  close(): void {}

  isClosed(): boolean {
    return false;
  }
}

describe("singleLinkSource", () => {
  // r[verify link.split]
  it("yields exactly one link attachment and preserves the client hello", async () => {
    const link = new TestLink();
    const clientHello = Uint8Array.of(1, 2, 3);
    const source = singleLinkSource(link, clientHello);

    await expect(source.nextLink()).resolves.toEqual({ link, clientHello });
    await expect(source.nextLink()).rejects.toThrow("single-use LinkSource exhausted");
  });
});
