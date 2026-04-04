// Node subject for the vox compliance suite.
//
// This demonstrates the minimal code needed to implement a vox service
// using the @vox/tcp transport library.

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
  Profile,
  Record,
  Status,
  Tag,
  Measurement,
  Config,
  TaggedPoint,
  GnarlyPayload,
} from "@bearcove/vox-generated/testbed.generated.ts";
import { TestbedClient, TestbedDispatcher } from "@bearcove/vox-generated/testbed.generated.ts";
import { tcpConnector, acceptTcp } from "@bearcove/vox-tcp";
import { createServer as createTcpServer, type AddressInfo } from "net";
import { wsConnector } from "@bearcove/vox-ws";
import {
  Driver,
  RpcErrorCode,
  SessionError,
  channel,
  session,
  setVoxLogger,
  voxServiceMetadata,
  type Tx,
  type Rx,
  type SessionConduitKind,
} from "@bearcove/vox-core";

// Enable vox internals logging for test visibility
setVoxLogger({
  debug: (...args) => console.error(...args),
  error: (...args) => console.error(...args),
});

// Service implementation
class TestbedService implements TestbedHandler {
  private async streamRetryProbeValues(count: number, output: Tx<number>): Promise<void> {
    for (let i = 0; i < count; i++) {
      await output.send(i);
    }
  }

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
    // Detect overflow: i64::MIN / -1 overflows
    if (dividend === -9223372036854775808n && divisor === -1n) {
      return { ok: false, error: { tag: "Overflow" } };
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
        if (id >= 100 && id <= 199) {
          return { ok: false, error: { tag: "AccessDenied" } };
        }
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
    await this.streamRetryProbeValues(count, output);
  }

  async generateRetryNonIdem(count: number, output: Tx<number>): Promise<void> {
    await this.streamRetryProbeValues(count, output);
  }

  async generateRetryIdem(count: number, output: Tx<number>): Promise<void> {
    await this.streamRetryProbeValues(count, output);
  }

  async transform(input: Rx<string>, output: Tx<string>): Promise<void> {
    // Server receives via Rx, sends via Tx (echo back as-is)
    for await (const s of input) {
      await output.send(s);
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

  echoGnarly(payload: GnarlyPayload): GnarlyPayload {
    return payload;
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

  echoBytes(data: Uint8Array): Uint8Array {
    return data;
  }

  echoBool(b: boolean): boolean {
    return b;
  }

  echoU64(n: bigint): bigint {
    return n;
  }

  echoOptionString(s: string | null): string | null {
    return s;
  }

  async sumLarge(numbers: Rx<number>): Promise<bigint> {
    let total = 0n;
    for await (const n of numbers) {
      total += BigInt(n);
    }
    return total;
  }

  async generateLarge(count: number, output: Tx<number>): Promise<void> {
    await this.streamRetryProbeValues(count, output);
  }

  allColors(): Color[] {
    return [{ tag: "Red" }, { tag: "Green" }, { tag: "Blue" }];
  }

  describePoint(label: string, x: number, y: number, active: boolean): TaggedPoint {
    return { label, x, y, active };
  }

  echoShape(shape: Shape): Shape {
    return shape;
  }

  echoStatusV1(status: Status): Status {
    return status;
  }

  echoTagV1(tag: Tag): Tag {
    return tag;
  }

  // Schema evolution methods
  echoProfile(profile: Profile): Profile {
    return profile;
  }

  echoRecord(record: Record): Record {
    return record;
  }

  echoStatus(status: Status): Status {
    return status;
  }

  echoTag(tag: Tag): Tag {
    return tag;
  }

  echoMeasurement(m: Measurement): Measurement {
    return m;
  }

  echoConfig(c: Config): Config {
    return c;
  }
}

function subjectConduit(): SessionConduitKind {
  return process.env.SPEC_CONDUIT === "stable" ? "stable" : "bare";
}

const RETRY_PROBE_ITEM_COUNT = 40;

function expectSequentialPrefix(received: number[], label: string): void {
  const expected = Array.from({ length: received.length }, (_, idx) => idx);
  if (received.length !== expected.length || received.some((value, idx) => value !== expected[idx])) {
    throw new Error(`${label} expected sequential prefix [${expected.join(", ")}], got [${received.join(", ")}]`);
  }
}


function makeConnector(addr: string) {
  if (addr.startsWith("ws://") || addr.startsWith("wss://")) {
    return wsConnector(addr);
  }
  return tcpConnector(addr);
}

async function runServer() {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
  const acceptConnections = process.env.ACCEPT_CONNECTIONS === "1";

  console.error(`server mode: connecting to ${addr}, acceptConnections=${acceptConnections}`);
  const established = await session.initiator(makeConnector(addr), {
    transport: subjectConduit(),
    metadata: voxServiceMetadata("Testbed"),
    onConnection: acceptConnections
      ? (connection) => {
          const driver = new Driver(
            connection,
            new TestbedDispatcher(new TestbedService()),
          );
          void driver.run();
        }
      : undefined,
  });
  const root = established.rootConnection();
  const driver = new Driver(root, new TestbedDispatcher(new TestbedService()));

  try {
    await driver.run();
  } catch (e) {
    if (e instanceof SessionError) {
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

  // Enable session resumption when the peer supports it — this allows
  // automatic reconnect and retry for idempotent/persist methods.
  const established = await session.initiator(makeConnector(addr), {
    transport: subjectConduit(),
    metadata: voxServiceMetadata("Testbed"),
    resumable: true,
  });
  const client = new TestbedClient(established.rootConnection().caller());
  const handle = established.handle();

  try {
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
    case "channel_retry_non_idem": {
      const [tx, rx] = channel<number>();
      const callPromise = client.generateRetryNonIdem(RETRY_PROBE_ITEM_COUNT, tx);
      const receiveTask = (async () => {
        const received: number[] = [];
        for await (const n of rx) {
          received.push(n);
        }
        return received;
      })();

      let result: unknown;
      try {
        await callPromise;
      } catch (error) {
        result = error;
      }

      const received = await receiveTask;
      if (!(result instanceof Error) || !("code" in result) || result.code !== RpcErrorCode.INDETERMINATE) {
        throw new Error(`expected indeterminate error, got ${String(result)}`);
      }
      expectSequentialPrefix(received, "non-idem retry");
      break;
    }
    case "channel_retry_idem": {
      const [tx, rx] = channel<number>();
      const callPromise = client.generateRetryIdem(RETRY_PROBE_ITEM_COUNT, tx);
      const receiveTask = (async () => {
        const received: number[] = [];
        for await (const n of rx) {
          received.push(n);
        }
        return received;
      })();

      await callPromise;
      const received = await receiveTask;
      const restart = received.findIndex((value, idx) => idx > 0 && value === 0);
      expectSequentialPrefix(received.slice(0, restart), "idem retry first attempt");
      const rerun = Array.from({ length: RETRY_PROBE_ITEM_COUNT }, (_, idx) => idx);
      const suffix = received.slice(restart);
      if (suffix.length !== rerun.length || suffix.some((value, idx) => value !== rerun[idx])) {
        throw new Error(`expected rerun suffix [${rerun.join(", ")}], got [${suffix.join(", ")}]`);
      }
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
    case "reverse": {
      const result = await client.reverse("hello");
      if (result !== "olleh") throw new Error(`reverse: expected 'olleh', got ${result}`);
      console.error(`reverse OK`);
      break;
    }
    case "divide_success": {
      const r = await client.divide(10n, 3n);
      if (!r.ok || r.value !== 3n) throw new Error(`divide_success: expected 3, got ${JSON.stringify(r)}`);
      console.error(`divide_success OK`);
      break;
    }
    case "divide_zero": {
      const r = await client.divide(10n, 0n);
      if (r.ok || r.error.tag !== "DivisionByZero") throw new Error(`divide_zero: expected DivisionByZero, got ${JSON.stringify(r)}`);
      console.error(`divide_zero OK`);
      break;
    }
    case "divide_overflow": {
      const r = await client.divide(-9223372036854775808n, -1n);
      if (r.ok || r.error.tag !== "Overflow") throw new Error(`divide_overflow: expected Overflow, got ${JSON.stringify(r)}`);
      console.error(`divide_overflow OK`);
      break;
    }
    case "lookup_found": {
      const r = await client.lookup(1);
      if (!r.ok || r.value.name !== "Alice") throw new Error(`lookup_found: expected Alice, got ${JSON.stringify(r)}`);
      console.error(`lookup_found OK: ${r.value.name}`);
      break;
    }
    case "lookup_found_no_email": {
      const r = await client.lookup(2);
      if (!r.ok || r.value.name !== "Bob" || r.value.email !== null) throw new Error(`lookup_found_no_email: ${JSON.stringify(r)}`);
      console.error(`lookup_found_no_email OK`);
      break;
    }
    case "lookup_not_found": {
      const r = await client.lookup(999);
      if (r.ok || r.error.tag !== "NotFound") throw new Error(`lookup_not_found: expected NotFound, got ${JSON.stringify(r)}`);
      console.error(`lookup_not_found OK`);
      break;
    }
    case "lookup_access_denied": {
      const r = await client.lookup(100);
      if (r.ok || r.error.tag !== "AccessDenied") throw new Error(`lookup_access_denied: expected AccessDenied, got ${JSON.stringify(r)}`);
      console.error(`lookup_access_denied OK`);
      break;
    }
    case "echo_point": {
      const pt = { x: 42, y: -7 };
      const result = await client.echoPoint(pt);
      if (result.x !== 42 || result.y !== -7) throw new Error(`echo_point: ${JSON.stringify(result)}`);
      console.error(`echo_point OK`);
      break;
    }
    case "create_person": {
      const p = await client.createPerson("Dave", 40, "dave@example.com");
      if (p.name !== "Dave" || p.age !== 40 || p.email !== "dave@example.com") throw new Error(`create_person: ${JSON.stringify(p)}`);
      const p2 = await client.createPerson("Eve", 25, null);
      if (p2.name !== "Eve" || p2.email !== null) throw new Error(`create_person null email: ${JSON.stringify(p2)}`);
      console.error(`create_person OK`);
      break;
    }
    case "rectangle_area": {
      const area = await client.rectangleArea({ top_left: { x: 0, y: 10 }, bottom_right: { x: 5, y: 0 }, label: null });
      if (Math.abs(area - 50) > 1e-9) throw new Error(`rectangle_area: expected 50, got ${area}`);
      console.error(`rectangle_area OK: ${area}`);
      break;
    }
    case "parse_color": {
      const r = await client.parseColor("red");
      if (r?.tag !== "Red") throw new Error(`parse_color red: ${JSON.stringify(r)}`);
      const g = await client.parseColor("green");
      if (g?.tag !== "Green") throw new Error(`parse_color green: ${JSON.stringify(g)}`);
      const b = await client.parseColor("blue");
      if (b?.tag !== "Blue") throw new Error(`parse_color blue: ${JSON.stringify(b)}`);
      const n = await client.parseColor("purple");
      if (n !== null) throw new Error(`parse_color purple: expected null, got ${JSON.stringify(n)}`);
      console.error(`parse_color OK (all variants)`);
      break;
    }
    case "get_points": {
      const pts = await client.getPoints(5);
      if (pts.length !== 5) throw new Error(`get_points: expected 5, got ${pts.length}`);
      if (pts[0].x !== 0 || pts[4].x !== 4) throw new Error(`get_points: unexpected values`);
      console.error(`get_points OK: ${pts.length} points`);
      break;
    }
    case "swap_pair": {
      const result = await client.swapPair([99, "hello"]);
      if (result[0] !== "hello" || result[1] !== 99) throw new Error(`swap_pair: ${JSON.stringify(result)}`);
      console.error(`swap_pair OK`);
      break;
    }
    case "echo_bytes": {
      const data = new Uint8Array([1, 2, 3, 255, 0, 128]);
      const result = await client.echoBytes(data);
      if (result.length !== data.length || !data.every((v, i) => result[i] === v)) throw new Error(`echo_bytes mismatch`);
      console.error(`echo_bytes OK`);
      break;
    }
    case "echo_bool": {
      if (await client.echoBool(true) !== true) throw new Error(`echo_bool true failed`);
      if (await client.echoBool(false) !== false) throw new Error(`echo_bool false failed`);
      console.error(`echo_bool OK`);
      break;
    }
    case "echo_u64": {
      for (const n of [0n, 1n, 18446744073709551615n, 1000000000000n]) {
        const result = await client.echoU64(n);
        if (result !== n) throw new Error(`echo_u64 ${n}: got ${result}`);
      }
      console.error(`echo_u64 OK`);
      break;
    }
    case "echo_option_string": {
      const s = await client.echoOptionString("hello");
      if (s !== "hello") throw new Error(`echo_option_string Some: ${s}`);
      const n = await client.echoOptionString(null);
      if (n !== null) throw new Error(`echo_option_string None: ${n}`);
      console.error(`echo_option_string OK`);
      break;
    }
    case "describe_point": {
      const tp = await client.describePoint("origin", 0, 0, true);
      if (tp.label !== "origin" || tp.x !== 0 || tp.y !== 0 || !tp.active) throw new Error(`describe_point: ${JSON.stringify(tp)}`);
      const tp2 = await client.describePoint("far", -100, 200, false);
      if (tp2.label !== "far" || tp2.x !== -100 || tp2.y !== 200 || tp2.active) throw new Error(`describe_point 2: ${JSON.stringify(tp2)}`);
      console.error(`describe_point OK`);
      break;
    }
    case "all_colors": {
      const colors = await client.allColors();
      if (colors.length !== 3) throw new Error(`all_colors: expected 3, got ${colors.length}`);
      if (colors[0].tag !== "Red" || colors[1].tag !== "Green" || colors[2].tag !== "Blue") throw new Error(`all_colors order wrong: ${JSON.stringify(colors)}`);
      console.error(`all_colors OK`);
      break;
    }
    case "echo_shape": {
      const shapes = [
        { tag: "Point" } as const,
        { tag: "Circle", radius: 3.14 } as const,
        { tag: "Rectangle", width: 2.0, height: 5.0 } as const,
      ];
      for (const shape of shapes) {
        const result = await client.echoShape(shape);
        if (result.tag !== shape.tag) throw new Error(`echo_shape ${shape.tag}: got ${JSON.stringify(result)}`);
      }
      console.error(`echo_shape OK (all 3 variants)`);
      break;
    }
    case "pipelining": {
      const promises = Array.from({ length: 10 }, (_, i) =>
        client.echo(`msg${i}`).then(r => {
          if (r !== `msg${i}`) throw new Error(`pipelining[${i}]: expected msg${i}, got ${r}`);
        })
      );
      await Promise.all(promises);
      console.error(`pipelining OK (10 concurrent echo calls)`);
      break;
    }
    case "sum_large": {
      // Client gives Rx to server (server receives), client keeps Tx and sends.
      // Bind rx first via the call, then start sending via tx.
      const [tx, rx] = channel<number>();
      const n = 100;
      const callPromise = client.sumLarge(rx);  // give rx to server — binds it
      // tx is now bound; send n items (> initial credit, tests flow control)
      for (let i = 0; i < n; i++) await tx.send(i);
      tx.close();
      const result = await callPromise;
      const expected = BigInt(n * (n - 1) / 2);
      if (result !== expected) throw new Error(`sum_large: expected ${expected}, got ${result}`);
      console.error(`sum_large OK: ${result}`);
      break;
    }
    case "generate_large": {
      // Client gives Tx to server (server sends), client keeps Rx and receives.
      // Bind tx first via the call, then start draining rx concurrently to grant credit.
      const [tx, rx] = channel<number>();
      const n = 100;
      const callPromise = client.generateLarge(n, tx);  // give tx to server — binds it
      // rx is now bound; drain it concurrently so we grant credit back to the server
      const received: number[] = [];
      const recvTask = (async () => {
        for await (const v of rx) received.push(v);
      })();
      await Promise.all([callPromise, recvTask]);
      if (received.length !== n) throw new Error(`generate_large: expected ${n}, got ${received.length}`);
      for (let i = 0; i < n; i++) {
        if (received[i] !== i) throw new Error(`generate_large[${i}]: expected ${i}, got ${received[i]}`);
      }
      console.error(`generate_large OK: ${received.length} items`);
      break;
    }
    case "sum_client_to_server": {
      // Client gives Rx to server (server receives), client keeps Tx and sends.
      const [tx, rx] = channel<number>();
      const callPromise = client.sum(rx);  // give rx to server — binds it
      for (const n of [1, 2, 3, 4, 5]) await tx.send(n);
      tx.close();
      const result = await callPromise;
      if (result !== 15n) throw new Error(`sum_client_to_server: expected 15n, got ${result}`);
      console.error(`sum_client_to_server OK: ${result}`);
      break;
    }
    case "transform_bidi": {
      // Client gives inputRx to server (server receives strings from client).
      // Client gives outputTx to server (server sends strings back to client).
      // Client keeps inputTx (sends) and outputRx (receives).
      const [inputTx, inputRx] = channel<string>();
      const [outputTx, outputRx] = channel<string>();
      const messages = ["alpha", "beta", "gamma"];
      const callPromise = client.transform(inputRx, outputTx);  // bind both — now inputTx & outputRx usable
      const received: string[] = [];
      const recvTask = (async () => {
        for await (const s of outputRx) received.push(s);
      })();
      for (const m of messages) await inputTx.send(m);
      inputTx.close();
      await callPromise;
      await recvTask;
      if (received.length !== messages.length || messages.some((m, i) => received[i] !== m)) {
        throw new Error(`transform_bidi: expected ${JSON.stringify(messages)}, got ${JSON.stringify(received)}`);
      }
      console.error(`transform_bidi OK`);
      break;
    }
    default:
      throw new Error(`unknown CLIENT_SCENARIO: ${scenario}`);
  }
  } finally {
    handle.shutdown();
    await established.closed().catch(() => {});
  }

}

async function runServerListen() {
  // Bind a TCP server, announce the address, serve one connection.
  // Used by cross-language harness tests where another subject is the client.
  const listenPort = process.env.LISTEN_PORT ? parseInt(process.env.LISTEN_PORT) : 0;

  const tcpServer = createTcpServer();
  await new Promise<void>((resolve) => tcpServer.listen(listenPort, "127.0.0.1", resolve));
  const { port } = tcpServer.address() as AddressInfo;

  // Signal readiness to the harness — it reads this line from stdout.
  process.stdout.write(`LISTEN_ADDR=127.0.0.1:${port}\n`);
  console.error(`server-listen mode: bound to 127.0.0.1:${port}`);

  const socket = await new Promise<import("net").Socket>((resolve) => {
    tcpServer.once("connection", (s) => {
      tcpServer.close();
      resolve(s);
    });
  });

  const established = await session.acceptorOn(acceptTcp(socket), {
    transport: subjectConduit(),
    metadata: voxServiceMetadata("Testbed"),
    // Provide a session resume key so Rust clients (which default to
    // resumable=true) don't reject the handshake. The key is generated
    // randomly; there is no session registry so reconnection won't work,
    // but the key satisfies the protocol requirement.
    resumable: true,
  });
  const driver = new Driver(
    established.rootConnection(),
    new TestbedDispatcher(new TestbedService()),
  );

  try {
    await driver.run();
  } catch (e) {
    if (e instanceof SessionError) return;
    throw e;
  }
}

async function main() {
  const mode = process.env.SUBJECT_MODE ?? "server";

  if (mode === "client") {
    await runClient();
  } else if (mode === "server-listen") {
    await runServerListen();
  } else {
    await runServer();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
