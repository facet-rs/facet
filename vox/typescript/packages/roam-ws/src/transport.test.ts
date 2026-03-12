import { afterEach, describe, expect, it, vi } from "vitest";
import { connectWs } from "./transport.ts";

type Listener = (event: { data?: ArrayBufferLike }) => void;

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static OPEN = 1;

  readonly url: string;
  readonly listeners = new Map<string, Listener[]>();
  readonly sent: Uint8Array[] = [];
  binaryType = "arraybuffer";
  readyState = FakeWebSocket.OPEN;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
    queueMicrotask(() => {
      this.dispatch("open", {});
    });
  }

  addEventListener(type: string, listener: Listener, _options?: unknown): void {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  send(payload: Uint8Array): void {
    this.sent.push(payload);
  }

  close(): void {
    this.dispatch("close", {});
  }

  emitMessage(payload: Uint8Array): void {
    this.dispatch("message", {
      data: payload.buffer.slice(
        payload.byteOffset,
        payload.byteOffset + payload.byteLength,
      ),
    });
  }

  private dispatch(type: string, event: { data?: ArrayBufferLike }): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

describe("WsLinkSource", () => {
  const originalWebSocket = globalThis.WebSocket;

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
    FakeWebSocket.instances.length = 0;
    vi.restoreAllMocks();
  });

  it("opens a link and forwards send/recv", async () => {
    globalThis.WebSocket = FakeWebSocket as unknown as typeof WebSocket;

    const source = connectWs("ws://example.test/roam");
    const attachment = await source.nextLink();
    const socket = FakeWebSocket.instances[0];

    expect(socket?.url).toBe("ws://example.test/roam");

    const outbound = new Uint8Array([1, 2, 3]);
    await attachment.link.send(outbound);
    expect(socket?.sent).toEqual([outbound]);

    const incoming = new Uint8Array([4, 5, 6]);
    socket?.emitMessage(incoming);
    await expect(attachment.link.recv()).resolves.toEqual(incoming);
  });
});
