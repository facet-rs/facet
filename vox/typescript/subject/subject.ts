// Node subject for the roam compliance suite.
//
// This demonstrates the minimal code needed to implement a roam service
// using the @roam/tcp transport library.

import type { EchoHandler } from "../generated/echo.ts";
import { echo_methodHandlers } from "../generated/echo.ts";
import { Server, type ServiceDispatcher } from "../tcp/index.ts";
import { UnaryDispatcher } from "../src/index.ts";

// Service implementation
class EchoService implements EchoHandler {
  echo(message: string): string {
    return message;
  }

  reverse(message: string): string {
    return Array.from(message).reverse().join("");
  }
}

// Dispatcher wraps the generated dispatch function
class EchoDispatcher implements ServiceDispatcher {
  private service = new EchoService();
  private dispatcher = new UnaryDispatcher(echo_methodHandlers);

  async dispatchUnary(methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    return this.dispatcher.dispatch(this.service, methodId, payload);
  }
}

async function main() {
  const server = new Server();
  await server.runSubject(new EchoDispatcher());
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
