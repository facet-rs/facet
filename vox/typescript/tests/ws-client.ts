// WebSocket client for cross-language testing (Node.js).
//
// Connects to a WebSocket server, performs Hello exchange, and runs Echo tests.

import { WsTransport, connectWs } from "../packages/roam-ws/src/transport.ts";
import { helloExchangeInitiator, defaultHello } from "../packages/roam-core/src/connection.ts";
import { EchoClient } from "../generated/echo.ts";

const serverAddr = process.env.SERVER_ADDR || "ws://localhost:9000";

async function main() {
  console.error(`Connecting to ${serverAddr}...`);

  const transport = await connectWs(serverAddr);
  console.error("Connected! Running tests...");

  const conn = await helloExchangeInitiator(transport, defaultHello());
  const client = new EchoClient(conn);

  // Test 1: Echo
  const echo1 = await client.echo("Hello, World!");
  if (echo1 !== "Hello, World!") {
    throw new Error(`Echo failed: got "${echo1}"`);
  }
  console.error("Echo: PASS");

  // Test 2: Reverse
  const rev1 = await client.reverse("Hello, World!");
  if (rev1 !== "!dlroW ,olleH") {
    throw new Error(`Reverse failed: got "${rev1}"`);
  }
  console.error("Reverse: PASS");

  // Test 3: Echo with unicode
  const echo2 = await client.echo("こんにちは世界");
  if (echo2 !== "こんにちは世界") {
    throw new Error(`Echo unicode failed: got "${echo2}"`);
  }
  console.error("Echo unicode: PASS");

  // Test 4: Reverse with unicode
  const rev2 = await client.reverse("こんにちは世界");
  if (rev2 !== "界世はちにんこ") {
    throw new Error(`Reverse unicode failed: got "${rev2}"`);
  }
  console.error("Reverse unicode: PASS");

  console.log("All tests passed!");
  transport.close();
  process.exit(0);
}

main().catch((err) => {
  console.error("Error:", err);
  process.exit(1);
});
