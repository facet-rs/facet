// Node subject for the roam compliance suite.
//
// This demonstrates the minimal code needed to implement a roam service
// using the @roam/tcp transport library.

import type {
  TestbedHandler,
  Point,
  Person,
  Rectangle,
  Color,
  Shape,
  Canvas,
  Message,
  MathError,
  LookupError,
} from "@bearcove/roam-generated/testbed.ts";
import { TestbedClient, TestbedDispatcher } from "@bearcove/roam-generated/testbed.ts";
import { Server } from "@bearcove/roam-tcp";
import { type Tx, type Rx, channel, ConnectionError } from "@bearcove/roam-core";

// Service implementation
class TestbedService implements TestbedHandler {
  // Echo methods
  echo(message: string): string {
    return message;
  }

  reverse(message: string): string {
    return Array.from(message).toReversed().join("");
  }

  // Fallible methods
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

  // Streaming methods
  async sum(numbers: Rx<number>): Promise<bigint> {
    // Server receives numbers via Rx channel and sums them
    let total = 0n;
    for await (const n of numbers) {
      total += BigInt(n);
    }
    return total;
  }

  async generate(count: number, output: Tx<number>): Promise<void> {
    // Server sends count numbers via Tx channel
    for (let i = 0; i < count; i++) {
      output.send(i);
    }
    // Note: output.close() is called by the generated handler after this returns
  }

  async transform(input: Rx<string>, output: Tx<string>): Promise<void> {
    // Server receives via Rx, sends via Tx (echo back as-is)
    for await (const s of input) {
      output.send(s);
    }
    // Note: output.close() is called by the generated handler after this returns
  }

  // Complex type methods
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


async function runServer() {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
  const acceptConnections = process.env.ACCEPT_CONNECTIONS === "1";

  console.error(`server mode: connecting to ${addr}, acceptConnections=${acceptConnections}`);
  const server = new Server();
  const conn = await server.connect(addr, { acceptConnections });

  try {
    await conn.runChanneling(new TestbedDispatcher(new TestbedService()));
  } catch (e) {
    if (e instanceof ConnectionError && e.kind === "closed") {
      // Clean shutdown
      return;
    }
    throw e;
  }
}

async function runClient() {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  const scenario = process.env.CLIENT_SCENARIO ?? "echo";
  console.error(`client mode: connecting to ${addr}, scenario=${scenario}`);

  const server = new Server();
  const conn = await server.connect(addr);
  const client = new TestbedClient(conn.asCaller());

  switch (scenario) {
    case "echo": {
      const result = await client.echo("hello from client");
      console.error(`echo result: ${result}`);
      break;
    }
    case "sum": {
      // Client-to-server streaming: create channel, start call, then send
      const [tx, rx] = channel<number>();

      // Start the call first - this binds the channels
      const resultPromise = client.sum(rx);

      // Now send data through the bound Tx
      for (let i = 1; i <= 5; i++) {
        console.error(`sending ${i}`);
        await tx.send(i);
      }
      console.error("closing tx");
      tx.close();

      // Wait for result
      const result = await resultPromise;
      console.error(`sum result: ${result}`);
      break;
    }
    case "generate": {
      // Server-to-client streaming: create channel, call, receive
      const [tx, rx] = channel<number>();

      // Start the call - server will send through our Rx
      await client.generate(5, tx);

      // Receive values from Rx
      const received: number[] = [];
      for await (const n of rx) {
        console.error(`received ${n}`);
        received.push(n);
      }
      console.error(`generate received: [${received.join(", ")}]`);
      break;
    }
    case "shape_area": {
      const result = await client.shapeArea({ tag: "Rectangle", width: 3, height: 4 });
      if (result !== 12) {
        throw new Error(`shape_area expected 12, got ${result}`);
      }
      console.error(`shape_area result: ${result}`);
      break;
    }
    case "create_canvas": {
      const result = await client.createCanvas(
        "enum-canvas",
        [{ tag: "Point" }, { tag: "Circle", radius: 2.5 }],
        { tag: "Green" },
      );
      if (result.name !== "enum-canvas") {
        throw new Error(`create_canvas expected name enum-canvas, got ${result.name}`);
      }
      if (result.background.tag !== "Green") {
        throw new Error(`create_canvas expected background Green, got ${result.background.tag}`);
      }
      if (
        result.shapes.length !== 2 ||
        result.shapes[0]?.tag !== "Point" ||
        result.shapes[1]?.tag !== "Circle" ||
        result.shapes[1].radius !== 2.5
      ) {
        throw new Error(
          `create_canvas returned unexpected shapes: ${JSON.stringify(result.shapes)}`,
        );
      }
      console.error(`create_canvas result OK`);
      break;
    }
    case "process_message": {
      const result = await client.processMessage({
        tag: "Data",
        value: new Uint8Array([1, 2, 3, 4]),
      });
      if (
        result.tag !== "Data" ||
        result.value.length !== 4 ||
        result.value.join(",") !== "4,3,2,1"
      ) {
        throw new Error(`process_message returned unexpected value`);
      }
      console.error(`process_message result OK`);
      break;
    }
    default:
      throw new Error(`unknown CLIENT_SCENARIO: ${scenario}`);
  }

  // Close the connection to allow process to exit
  conn.getIo().close();
}

async function main() {
  const mode = process.env.SUBJECT_MODE ?? "server";

  if (mode === "client") {
    await runClient();
  } else {
    await runServer();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
