// TypeScript TCP server for cross-language testing.
//
// Listens on a TCP port and handles Echo service requests.
// Used to test clients in other languages against a TypeScript server.

import * as net from "node:net";
import { Server } from "../packages/roam-tcp/src/server.ts";
import { UnaryDispatcher, ConnectionError } from "../packages/roam-core/src/index.ts";
import type { EchoHandler } from "../generated/echo.ts";
import { echo_methodHandlers } from "../generated/echo.ts";

// Echo service implementation
class EchoService implements EchoHandler {
  echo(message: string): string {
    return message;
  }

  reverse(message: string): string {
    return Array.from(message).reverse().join("");
  }
}

// Dispatcher
class EchoDispatcher {
  private service = new EchoService();
  private dispatcher = new UnaryDispatcher(echo_methodHandlers);

  async dispatchUnary(methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    return this.dispatcher.dispatch(this.service, methodId, payload);
  }
}

async function main() {
  const port = parseInt(process.env.TCP_PORT || "9020", 10);
  const addr = `127.0.0.1:${port}`;

  const server = new Server();
  const dispatcher = new EchoDispatcher();

  const tcpServer = net.createServer(async (socket) => {
    const peer = `${socket.remoteAddress}:${socket.remotePort}`;
    console.error(`New connection from ${peer}`);

    try {
      const conn = await server.accept(socket);
      console.error(`Hello exchange complete with ${peer}`);
      await conn.run(dispatcher);
    } catch (e) {
      if (e instanceof ConnectionError && e.kind === "closed") {
        // Clean shutdown
      } else {
        console.error(`Connection error: ${e}`);
      }
    }
    console.error(`Connection closed: ${peer}`);
  });

  tcpServer.listen(port, "127.0.0.1", () => {
    console.error(`TypeScript TCP server listening on ${addr}`);
    console.log(port); // For test harness
  });
}

main().catch((err) => {
  console.error("Error:", err);
  process.exit(1);
});
