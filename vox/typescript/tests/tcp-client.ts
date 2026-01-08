// TypeScript TCP client for cross-language testing.
//
// Connects to a TCP server, performs Hello exchange, and makes RPC calls.
// Used to test TypeScript client against servers implemented in other languages.

import * as net from "node:net";
import { CobsFramed } from "../packages/roam-tcp/src/framing.ts";
import { helloExchangeInitiator, defaultHello } from "../packages/roam-core/src/connection.ts";
import { EchoClient } from "../generated/echo.ts";

async function main() {
  const serverAddr = process.env.SERVER_ADDR || "127.0.0.1:9001";
  const [host, portStr] = serverAddr.split(":");
  const port = parseInt(portStr, 10);

  console.error(`Connecting to ${serverAddr}...`);

  // Create TCP socket
  const socket = await new Promise<net.Socket>((resolve, reject) => {
    const sock = net.createConnection({ host, port }, () => {
      resolve(sock);
    });
    sock.on("error", reject);
  });

  console.error("TCP connected, performing Hello exchange...");

  // Wrap in COBS framing
  const transport = new CobsFramed(socket);

  // Do Hello exchange as initiator (client)
  const conn = await helloExchangeInitiator(transport, defaultHello());

  console.error("Connected! Running tests...");

  // Create Echo client
  const client = new EchoClient(conn);

  // Test Echo
  let result = await client.echo("Hello, World!");
  if (result !== "Hello, World!") {
    console.error(`Echo mismatch: got "${result}", want "Hello, World!"`);
    process.exit(1);
  }
  console.error("Echo: PASS");

  // Test Reverse
  result = await client.reverse("Hello");
  if (result !== "olleH") {
    console.error(`Reverse mismatch: got "${result}", want "olleH"`);
    process.exit(1);
  }
  console.error("Reverse: PASS");

  // Test with unicode
  result = await client.echo("Hello, World! \uD83C\uDF89");
  if (result !== "Hello, World! \uD83C\uDF89") {
    console.error(`Echo unicode mismatch: got "${result}", want "Hello, World! 🎉"`);
    process.exit(1);
  }
  console.error("Echo unicode: PASS");

  // Test Reverse with unicode
  result = await client.reverse("日本語");
  if (result !== "語本日") {
    console.error(`Reverse unicode mismatch: got "${result}", want "語本日"`);
    process.exit(1);
  }
  console.error("Reverse unicode: PASS");

  console.log("All tests passed!");

  // Close connection
  transport.close();
}

main().catch((err) => {
  console.error("Error:", err);
  process.exit(1);
});
