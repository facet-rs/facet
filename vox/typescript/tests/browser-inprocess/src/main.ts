// Browser test for roam in-process transport.
//
// Wires a Rust WASM acceptor (server) to a TypeScript initiator (client)
// in the same browser tab, with no network involved.

import init, { start_acceptor } from "../pkg/wasm_inprocess_tests.js";
import { InProcessLink } from "@bearcove/roam-inprocess";
import {
  session,
} from "@bearcove/roam-core";
import type { TestbedClient } from "@bearcove/roam-generated/testbed.ts";

// Make test results available to Playwright
declare global {
  interface Window {
    testResults: TestResult[];
    testsComplete: boolean;
  }
}

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
}

const results: TestResult[] = [];
window.testResults = results;
window.testsComplete = false;

function log(message: string) {
  const status = document.getElementById("status");
  if (status) status.textContent = message;
  console.log(message);
}

function addResult(name: string, passed: boolean, error?: string) {
  const result: TestResult = { name, passed, error };
  results.push(result);

  const resultsDiv = document.getElementById("results");
  if (resultsDiv) {
    const div = document.createElement("div");
    div.className = passed ? "pass" : "fail";
    div.textContent = `${passed ? "PASS" : "FAIL"}: ${name}${error ? ` - ${error}` : ""}`;
    resultsDiv.appendChild(div);
  }
}

async function testEcho(client: TestbedClient): Promise<void> {
  log("Testing echo...");
  const echoMessage = "Hello from in-process TS!";
  const echoResult = await client.echo(echoMessage);
  if (echoResult !== echoMessage) {
    throw new Error(
      `Echo mismatch: expected "${echoMessage}", got "${echoResult}"`,
    );
  }
  addResult("echo", true);

  log("Testing reverse...");
  const reverseMessage = "Hello";
  const reverseResult = await client.reverse(reverseMessage);
  const expectedReverse = reverseMessage.split("").toReversed().join("");
  if (reverseResult !== expectedReverse) {
    throw new Error(
      `Reverse mismatch: expected "${expectedReverse}", got "${reverseResult}"`,
    );
  }
  addResult("reverse", true);
}

async function testComplex(client: TestbedClient): Promise<void> {
  log("Testing echoPoint...");
  const point = { x: 42, y: -17 };
  const pointResult = await client.echoPoint(point);
  if (pointResult.x !== point.x || pointResult.y !== point.y) {
    throw new Error(
      `echoPoint mismatch: expected ${JSON.stringify(point)}, got ${JSON.stringify(pointResult)}`,
    );
  }
  addResult("echoPoint", true);

  log("Testing createPerson...");
  const person = await client.createPerson("Alice", 30, "alice@example.com");
  if (
    person.name !== "Alice" ||
    person.age !== 30 ||
    person.email !== "alice@example.com"
  ) {
    throw new Error(`createPerson mismatch: got ${JSON.stringify(person)}`);
  }
  addResult("createPerson", true);

  log("Testing createPerson with null email...");
  const personNoEmail = await client.createPerson("Bob", 25, null);
  if (
    personNoEmail.name !== "Bob" ||
    personNoEmail.age !== 25 ||
    personNoEmail.email !== null
  ) {
    throw new Error(
      `createPerson (null email) mismatch: got ${JSON.stringify(personNoEmail)}`,
    );
  }
  addResult("createPerson (null email)", true);

  log("Testing rectangleArea...");
  const rect = {
    top_left: { x: 0, y: 0 },
    bottom_right: { x: 10, y: 5 },
    label: null,
  };
  const area = await client.rectangleArea(rect);
  if (area !== 50.0) {
    throw new Error(`rectangleArea mismatch: expected 50, got ${area}`);
  }
  addResult("rectangleArea", true);

  log("Testing parseColor...");
  const color = await client.parseColor("red");
  if (color === null || color.tag !== "Red") {
    throw new Error(
      `parseColor mismatch: expected Red, got ${JSON.stringify(color)}`,
    );
  }
  addResult("parseColor", true);

  log("Testing parseColor (unknown)...");
  const unknownColor = await client.parseColor("purple");
  if (unknownColor !== null) {
    throw new Error(
      `parseColor (unknown) mismatch: expected null, got ${JSON.stringify(unknownColor)}`,
    );
  }
  addResult("parseColor (unknown)", true);

  log("Testing shapeArea (Circle)...");
  const circleArea = await client.shapeArea({ tag: "Circle", radius: 2.0 });
  const expectedCircleArea = Math.PI * 4;
  if (Math.abs(circleArea - expectedCircleArea) > 0.0001) {
    throw new Error(
      `shapeArea (Circle) mismatch: expected ${expectedCircleArea}, got ${circleArea}`,
    );
  }
  addResult("shapeArea (Circle)", true);

  log("Testing shapeArea (Rectangle)...");
  const rectArea = await client.shapeArea({
    tag: "Rectangle",
    width: 3.0,
    height: 4.0,
  });
  if (rectArea !== 12.0) {
    throw new Error(
      `shapeArea (Rectangle) mismatch: expected 12, got ${rectArea}`,
    );
  }
  addResult("shapeArea (Rectangle)", true);

  log("Testing shapeArea (Point)...");
  const pointArea = await client.shapeArea({ tag: "Point" });
  if (pointArea !== 0.0) {
    throw new Error(
      `shapeArea (Point) mismatch: expected 0, got ${pointArea}`,
    );
  }
  addResult("shapeArea (Point)", true);

  log("Testing getPoints...");
  const points = await client.getPoints(3);
  if (points.length !== 3) {
    throw new Error(
      `getPoints mismatch: expected 3 points, got ${points.length}`,
    );
  }
  if (points[0].x !== 0 || points[0].y !== 0) {
    throw new Error(`getPoints[0] mismatch: got ${JSON.stringify(points[0])}`);
  }
  if (points[1].x !== 1 || points[1].y !== 2) {
    throw new Error(`getPoints[1] mismatch: got ${JSON.stringify(points[1])}`);
  }
  if (points[2].x !== 2 || points[2].y !== 4) {
    throw new Error(`getPoints[2] mismatch: got ${JSON.stringify(points[2])}`);
  }
  addResult("getPoints", true);

  log("Testing swapPair...");
  const swapped = await client.swapPair([42, "hello"]);
  if (swapped[0] !== "hello" || swapped[1] !== 42) {
    throw new Error(
      `swapPair mismatch: expected ["hello", 42], got ${JSON.stringify(swapped)}`,
    );
  }
  addResult("swapPair", true);

  log("Testing processMessage (Text)...");
  const textMsg = await client.processMessage({
    tag: "Text",
    value: "hello",
  });
  if (textMsg.tag !== "Text" || textMsg.value !== "Processed: hello") {
    throw new Error(
      `processMessage (Text) mismatch: got ${JSON.stringify(textMsg)}`,
    );
  }
  addResult("processMessage (Text)", true);

  log("Testing processMessage (Number)...");
  const numMsg = await client.processMessage({ tag: "Number", value: 21n });
  if (numMsg.tag !== "Number" || numMsg.value !== 42n) {
    throw new Error(
      `processMessage (Number) mismatch: got ${JSON.stringify(numMsg)}`,
    );
  }
  addResult("processMessage (Number)", true);

  log("Testing createCanvas...");
  const canvas = await client.createCanvas(
    "MyCanvas",
    [
      { tag: "Circle", radius: 5.0 },
      { tag: "Rectangle", width: 10.0, height: 20.0 },
    ],
    { tag: "Blue" },
  );
  if (canvas.name !== "MyCanvas") {
    throw new Error(`createCanvas name mismatch: got ${canvas.name}`);
  }
  if (canvas.shapes.length !== 2) {
    throw new Error(
      `createCanvas shapes length mismatch: got ${canvas.shapes.length}`,
    );
  }
  if (canvas.background.tag !== "Blue") {
    throw new Error(
      `createCanvas background mismatch: got ${JSON.stringify(canvas.background)}`,
    );
  }
  addResult("createCanvas", true);
}

async function testFallible(client: TestbedClient): Promise<void> {
  log("Testing divide (success)...");
  const divideResult = await client.divide(10n, 2n);
  if (!divideResult.ok) {
    throw new Error(
      `divide (success) expected Ok, got Err: ${JSON.stringify(divideResult.error)}`,
    );
  }
  if (divideResult.value !== 5n) {
    throw new Error(
      `divide (success) mismatch: expected 5n, got ${divideResult.value}`,
    );
  }
  addResult("divide (success)", true);

  log("Testing divide (error)...");
  const divideError = await client.divide(10n, 0n);
  if (divideError.ok) {
    throw new Error(
      `divide (error) expected Err, got Ok: ${divideError.value}`,
    );
  }
  if (divideError.error.tag !== "DivisionByZero") {
    throw new Error(
      `divide (error) mismatch: expected DivisionByZero, got ${JSON.stringify(divideError.error)}`,
    );
  }
  addResult("divide (error)", true);

  log("Testing lookup (success)...");
  const lookupResult = await client.lookup(1);
  if (!lookupResult.ok) {
    throw new Error(
      `lookup (success) expected Ok, got Err: ${JSON.stringify(lookupResult.error)}`,
    );
  }
  const alice = lookupResult.value;
  if (
    alice.name !== "Alice" ||
    alice.age !== 30 ||
    alice.email !== "alice@example.com"
  ) {
    throw new Error(
      `lookup (success) mismatch: got ${JSON.stringify(alice)}`,
    );
  }
  addResult("lookup (success)", true);

  log("Testing lookup (error)...");
  const lookupError = await client.lookup(999);
  if (lookupError.ok) {
    throw new Error(
      `lookup (error) expected Err, got Ok: ${JSON.stringify(lookupError.value)}`,
    );
  }
  if (lookupError.error.tag !== "NotFound") {
    throw new Error(
      `lookup (error) mismatch: expected NotFound, got ${JSON.stringify(lookupError.error)}`,
    );
  }
  addResult("lookup (error)", true);

  log("Testing lookup (null email)...");
  const bobResult = await client.lookup(2);
  if (!bobResult.ok) {
    throw new Error(
      `lookup (null email) expected Ok, got Err: ${JSON.stringify(bobResult.error)}`,
    );
  }
  const bob = bobResult.value;
  if (bob.name !== "Bob" || bob.age !== 25 || bob.email !== null) {
    throw new Error(
      `lookup (null email) mismatch: got ${JSON.stringify(bob)}`,
    );
  }
  addResult("lookup (null email)", true);
}

async function runTests(): Promise<void> {
  log("Initializing WASM module...");

  try {
    await init();
    log("WASM module loaded. Setting up in-process transport...");

    // Wire up the in-process link:
    // - TS creates InProcessLink with a deliver callback
    // - Rust creates JsInProcessLink with an on_message callback
    let rustLink: ReturnType<typeof start_acceptor> | null = null;
    const link = new InProcessLink((payload: Uint8Array) => {
      if (!rustLink) {
        throw new Error("rustLink not initialized");
      }
      rustLink.deliver(payload);
    });
    rustLink = start_acceptor((payload: Uint8Array) => {
      link.pushMessage(payload);
    });

    log("Establishing session as initiator...");
    const established = await session.initiatorOn(link, { transport: "bare" });

    // Import the TestbedClient constructor dynamically to avoid circular issues
    const { TestbedClient } = await import(
      "@bearcove/roam-generated/testbed.ts"
    );
    const client = new TestbedClient(established.rootConnection().caller());

    log("Connection established! Running tests...");

    await testEcho(client);
    await testComplex(client);
    await testFallible(client);
    // Channel tests (sum, generate, transform) are skipped because
    // Tx/Rx runtime methods are not available on wasm32 targets.

    log("All in-process tests passed!");
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    log(`Error: ${error}`);
    console.error(e);
    addResult("connection", false, error);
  }

  window.testsComplete = true;
}

// Auto-run
runTests();
