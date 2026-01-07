// Browser test client for roam WebSocket
//
// This test connects to a Rust WebSocket server and makes RPC calls
// using generated client code for Echo and Complex services.

import { WsTransport, connectWs } from "@bearcove/roam-ws";
import {
  helloExchangeInitiator,
  defaultHello,
  Connection,
} from "@bearcove/roam-core";
import { EchoClient } from "@bearcove/roam-generated/echo.ts";
import { ComplexClient } from "@bearcove/roam-generated/complex.ts";

// Make test results available to Playwright
declare global {
  interface Window {
    testResults: TestResult[];
    runTests: (wsUrl: string) => Promise<void>;
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

async function testEcho(client: EchoClient): Promise<void> {
  // Test 1: echo - using generated client
  log("Testing echo...");
  const echoMessage = "Hello from browser!";
  const echoResult = await client.echo(echoMessage);
  if (echoResult !== echoMessage) {
    throw new Error(`Echo mismatch: expected "${echoMessage}", got "${echoResult}"`);
  }
  addResult("echo", true);

  // Test 2: reverse - using generated client
  log("Testing reverse...");
  const reverseMessage = "Hello";
  const reverseResult = await client.reverse(reverseMessage);
  const expectedReverse = reverseMessage.split("").reverse().join("");
  if (reverseResult !== expectedReverse) {
    throw new Error(`Reverse mismatch: expected "${expectedReverse}", got "${reverseResult}"`);
  }
  addResult("reverse", true);
}

async function testComplex(client: ComplexClient): Promise<void> {
  // Test: echoPoint - struct encoding/decoding
  log("Testing echoPoint...");
  const point = { x: 42, y: -17 };
  const pointResult = await client.echoPoint(point);
  if (pointResult.x !== point.x || pointResult.y !== point.y) {
    throw new Error(`echoPoint mismatch: expected ${JSON.stringify(point)}, got ${JSON.stringify(pointResult)}`);
  }
  addResult("echoPoint", true);

  // Test: createPerson - multiple args including Option
  log("Testing createPerson...");
  const person = await client.createPerson("Alice", 30, "alice@example.com");
  if (person.name !== "Alice" || person.age !== 30 || person.email !== "alice@example.com") {
    throw new Error(`createPerson mismatch: got ${JSON.stringify(person)}`);
  }
  addResult("createPerson", true);

  // Test: createPerson with null email
  log("Testing createPerson with null email...");
  const personNoEmail = await client.createPerson("Bob", 25, null);
  if (personNoEmail.name !== "Bob" || personNoEmail.age !== 25 || personNoEmail.email !== null) {
    throw new Error(`createPerson (null email) mismatch: got ${JSON.stringify(personNoEmail)}`);
  }
  addResult("createPerson (null email)", true);

  // Test: rectangleArea - nested struct
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

  // Test: parseColor - Option<Enum>
  log("Testing parseColor...");
  const color = await client.parseColor("red");
  if (color === null || color.tag !== "Red") {
    throw new Error(`parseColor mismatch: expected Red, got ${JSON.stringify(color)}`);
  }
  addResult("parseColor", true);

  // Test: parseColor with unknown color
  log("Testing parseColor (unknown)...");
  const unknownColor = await client.parseColor("purple");
  if (unknownColor !== null) {
    throw new Error(`parseColor (unknown) mismatch: expected null, got ${JSON.stringify(unknownColor)}`);
  }
  addResult("parseColor (unknown)", true);

  // Test: shapeArea - enum with different payloads
  log("Testing shapeArea (Circle)...");
  const circleArea = await client.shapeArea({ tag: "Circle", radius: 2.0 });
  const expectedCircleArea = Math.PI * 4;
  if (Math.abs(circleArea - expectedCircleArea) > 0.0001) {
    throw new Error(`shapeArea (Circle) mismatch: expected ${expectedCircleArea}, got ${circleArea}`);
  }
  addResult("shapeArea (Circle)", true);

  log("Testing shapeArea (Rectangle)...");
  const rectArea = await client.shapeArea({ tag: "Rectangle", width: 3.0, height: 4.0 });
  if (rectArea !== 12.0) {
    throw new Error(`shapeArea (Rectangle) mismatch: expected 12, got ${rectArea}`);
  }
  addResult("shapeArea (Rectangle)", true);

  log("Testing shapeArea (Point)...");
  const pointArea = await client.shapeArea({ tag: "Point" });
  if (pointArea !== 0.0) {
    throw new Error(`shapeArea (Point) mismatch: expected 0, got ${pointArea}`);
  }
  addResult("shapeArea (Point)", true);

  // Test: getPoints - Vec<Struct>
  log("Testing getPoints...");
  const points = await client.getPoints(3);
  if (points.length !== 3) {
    throw new Error(`getPoints mismatch: expected 3 points, got ${points.length}`);
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

  // Test: swapPair - tuple types
  log("Testing swapPair...");
  const swapped = await client.swapPair([42, "hello"]);
  if (swapped[0] !== "hello" || swapped[1] !== 42) {
    throw new Error(`swapPair mismatch: expected ["hello", 42], got ${JSON.stringify(swapped)}`);
  }
  addResult("swapPair", true);

  // Test: processMessage - enum with different payload types
  log("Testing processMessage (Text)...");
  const textMsg = await client.processMessage({ tag: "Text", value: "hello" });
  if (textMsg.tag !== "Text" || textMsg.value !== "Processed: hello") {
    throw new Error(`processMessage (Text) mismatch: got ${JSON.stringify(textMsg)}`);
  }
  addResult("processMessage (Text)", true);

  log("Testing processMessage (Number)...");
  const numMsg = await client.processMessage({ tag: "Number", value: 21n });
  if (numMsg.tag !== "Number" || numMsg.value !== 42n) {
    throw new Error(`processMessage (Number) mismatch: got ${JSON.stringify(numMsg)}`);
  }
  addResult("processMessage (Number)", true);

  // Test: createCanvas - complex nested types
  log("Testing createCanvas...");
  const canvas = await client.createCanvas(
    "MyCanvas",
    [
      { tag: "Circle", radius: 5.0 },
      { tag: "Rectangle", width: 10.0, height: 20.0 },
    ],
    { tag: "Blue" }
  );
  if (canvas.name !== "MyCanvas") {
    throw new Error(`createCanvas name mismatch: got ${canvas.name}`);
  }
  if (canvas.shapes.length !== 2) {
    throw new Error(`createCanvas shapes length mismatch: got ${canvas.shapes.length}`);
  }
  if (canvas.background.tag !== "Blue") {
    throw new Error(`createCanvas background mismatch: got ${JSON.stringify(canvas.background)}`);
  }
  addResult("createCanvas", true);
}

async function runTests(wsUrl: string): Promise<void> {
  log(`Connecting to ${wsUrl}...`);

  try {
    const transport = await connectWs(wsUrl);
    log("WebSocket connected!");

    log("Performing Hello exchange...");
    const conn = await helloExchangeInitiator(transport, defaultHello());
    log(`Hello exchange complete. Negotiated maxPayloadSize: ${conn.negotiated().maxPayloadSize}`);

    // Create generated clients
    const echoClient = new EchoClient(conn);
    const complexClient = new ComplexClient(conn);

    // Run Echo tests
    await testEcho(echoClient);

    // Run Complex tests
    await testComplex(complexClient);

    conn.getIo().close();
    log("All tests passed!");
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    log(`Error: ${error}`);
    addResult("connection", false, error);
  }

  window.testsComplete = true;
}

window.runTests = runTests;

// Auto-run if WS_URL is in the URL hash
const urlParams = new URLSearchParams(window.location.search);
const wsUrl = urlParams.get("ws");
if (wsUrl) {
  runTests(wsUrl);
}
