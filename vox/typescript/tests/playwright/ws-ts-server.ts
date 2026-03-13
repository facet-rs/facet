import { WebSocketServer, type RawData, type WebSocket } from "ws";
import {
  Driver,
  SessionError,
  SessionRegistry,
  session,
  type Link,
  type Rx,
  type SessionHandle,
  type Tx,
} from "@bearcove/roam-core";
import type {
  Canvas,
  Color,
  LookupError,
  MathError,
  Message,
  Person,
  Point,
  Rectangle,
  Shape,
  TestbedHandler,
} from "@bearcove/roam-generated/testbed.ts";
import { TestbedDispatcher } from "@bearcove/roam-generated/testbed.ts";

class NodeWsLink implements Link {
  lastReceived: Uint8Array | undefined;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  constructor(private readonly ws: WebSocket) {
    ws.on("message", (data: RawData) => {
      const payload = rawDataToUint8Array(data);
      this.lastReceived = payload;
      if (this.waitingResolve) {
        const resolve = this.waitingResolve;
        this.waitingResolve = null;
        resolve(payload);
      } else {
        this.pendingMessages.push(payload);
      }
    });

    ws.on("close", () => {
      this.closed = true;
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve?.(null);
    });

    ws.on("error", () => {
      this.closed = true;
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve?.(null);
    });
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.closed || this.ws.readyState !== this.ws.OPEN) {
      throw new Error("WebSocket not open");
    }
    await new Promise<void>((resolve, reject) => {
      this.ws.send(payload, (error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve();
      });
    });
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

function rawDataToUint8Array(data: RawData): Uint8Array {
  if (data instanceof Uint8Array) {
    return data;
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  if (Array.isArray(data)) {
    const joined = Buffer.concat(data.map((part) => Buffer.from(part)));
    return new Uint8Array(joined.buffer, joined.byteOffset, joined.byteLength);
  }
  const buffer = Buffer.from(data);
  return new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
}

class TestbedService implements TestbedHandler {
  async echo(message: string): Promise<string> {
    if (message === "__roam_reconnect__") {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    return message;
  }

  reverse(message: string): string {
    return Array.from(message).toReversed().join("");
  }

  divide(
    dividend: bigint,
    divisor: bigint,
  ): { ok: true; value: bigint } | { ok: false; error: MathError } {
    if (divisor === 0n) {
      return { ok: false, error: { tag: "DivisionByZero" } };
    }
    return { ok: true, value: dividend / divisor };
  }

  lookup(id: number): { ok: true; value: Person } | { ok: false; error: LookupError } {
    switch (id) {
      case 1:
        return { ok: true, value: { name: "Alice", age: 30, email: "alice@example.com" } };
      case 2:
        return { ok: true, value: { name: "Bob", age: 25, email: null } };
      case 3:
        return { ok: true, value: { name: "Charlie", age: 35, email: "charlie@example.com" } };
      default:
        return { ok: false, error: { tag: "NotFound" } };
    }
  }

  async sum(numbers: Rx<number>): Promise<bigint> {
    let total = 0n;
    for await (const n of numbers) {
      total += BigInt(n);
    }
    return total;
  }

  async generate(count: number, output: Tx<number>): Promise<void> {
    for (let i = 0; i < count; i++) {
      await output.send(i);
    }
  }

  async transform(input: Rx<string>, output: Tx<string>): Promise<void> {
    for await (const s of input) {
      await output.send(s);
    }
  }

  echoPoint(point: Point): Point {
    return point;
  }

  createPerson(name: string, age: number, email: string | null): Person {
    return { name, age, email };
  }

  rectangleArea(rect: Rectangle): number {
    const width = Math.abs(rect.bottom_right.x - rect.top_left.x);
    const height = Math.abs(rect.bottom_right.y - rect.top_left.y);
    return width * height;
  }

  parseColor(name: string): Color | null {
    switch (name.toLowerCase()) {
      case "red":
        return { tag: "Red" };
      case "green":
        return { tag: "Green" };
      case "blue":
        return { tag: "Blue" };
      default:
        return null;
    }
  }

  shapeArea(shape: Shape): number {
    switch (shape.tag) {
      case "Circle":
        return Math.PI * shape.radius * shape.radius;
      case "Rectangle":
        return shape.width * shape.height;
      case "Point":
        return 0;
    }
  }

  createCanvas(name: string, shapes: Shape[], background: Color): Canvas {
    return { name, shapes, background };
  }

  processMessage(msg: Message): Message {
    switch (msg.tag) {
      case "Text":
        return { tag: "Text", value: `Processed: ${msg.value}` };
      case "Number":
        return { tag: "Number", value: msg.value * 2n };
      case "Data":
        return { tag: "Data", value: msg.value.toReversed() };
    }
  }

  getPoints(count: number): Point[] {
    const points: Point[] = [];
    for (let i = 0; i < count; i++) {
      points.push({ x: i, y: i * 2 });
    }
    return points;
  }

  swapPair(pair: [number, string]): [string, number] {
    return [pair[1], pair[0]];
  }
}

export interface TsWsServerHandle {
  close(): Promise<void>;
}

export async function startTsWsServer(port: number): Promise<TsWsServerHandle> {
  const wss = new WebSocketServer({ host: "127.0.0.1", port });
  const registry = new SessionRegistry();
  const activeSessions = new Set<SessionHandle>();
  const activeDrivers = new Set<Promise<void>>();

  const dispatcher = new TestbedDispatcher(new TestbedService());

  wss.on("connection", (socket) => {
    const link = new NodeWsLink(socket);
    void session.acceptorTransportOrResume(link, registry).then((accepted) => {
      if (accepted.tag === "Resumed") {
        return;
      }

      const established = accepted.session;
      activeSessions.add(established.handle());
      const driver = new Driver(established.rootConnection(), dispatcher);
      const run = driver.run().catch((error) => {
        if (!(error instanceof SessionError)) {
          throw error;
        }
      }).finally(() => {
        activeDrivers.delete(run);
        activeSessions.delete(established.handle());
      });
      activeDrivers.add(run);
    }).catch((error) => {
      console.error("[ts-ws-server] session error:", error);
      link.close();
    });
  });

  await new Promise<void>((resolve, reject) => {
    wss.once("listening", () => resolve());
    wss.once("error", (error) => reject(error));
  });

  return {
    async close(): Promise<void> {
      for (const handle of activeSessions) {
        handle.shutdown();
      }
      for (const client of wss.clients) {
        client.close();
      }
      await new Promise<void>((resolve, reject) => {
        wss.close((error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      });
      await Promise.allSettled([...activeDrivers]);
    },
  };
}
