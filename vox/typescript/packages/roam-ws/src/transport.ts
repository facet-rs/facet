import type { Link, LinkSource } from "@bearcove/roam-core";

export class WsLink implements Link {
  lastReceived: Uint8Array | undefined;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  constructor(private readonly ws: WebSocket) {
    ws.binaryType = "arraybuffer";

    ws.addEventListener("message", (event: MessageEvent) => {
      if (!(event.data instanceof ArrayBuffer)) {
        return;
      }
      const payload = new Uint8Array(event.data);
      this.lastReceived = payload;
      if (this.waitingResolve) {
        const resolve = this.waitingResolve;
        this.waitingResolve = null;
        resolve(payload);
      } else {
        this.pendingMessages.push(payload);
      }
    });

    ws.addEventListener("close", () => {
      this.closed = true;
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve?.(null);
    });

    ws.addEventListener("error", () => {
      this.closed = true;
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve?.(null);
    });
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("WebSocket not open");
    }
    this.ws.send(payload);
  }

  recv(): Promise<Uint8Array | null> {
    if (this.pendingMessages.length > 0) {
      return Promise.resolve(this.pendingMessages.shift()!);
    }
    if (this.closed) {
      return Promise.resolve(null);
    }
    return new Promise((resolve) => {
      this.waitingResolve = resolve;
    });
  }

  close(): void {
    this.closed = true;
    this.ws.close();
  }

  isClosed(): boolean {
    return this.closed;
  }
}

export class WsLinkSource implements LinkSource<WsLink> {
  constructor(private readonly url: string) {}

  async nextLink(): Promise<{ link: WsLink }> {
    const ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(this.url);
      socket.binaryType = "arraybuffer";
      socket.addEventListener("open", () => resolve(socket), { once: true });
      socket.addEventListener(
        "error",
        () => reject(new Error(`failed to connect to ${this.url}`)),
        { once: true },
      );
    });
    return { link: new WsLink(ws) };
  }
}

export function connectWs(url: string): LinkSource<WsLink> {
  return new WsLinkSource(url);
}
