// Node subject for the roam compliance suite.
//
// This demonstrates the minimal code needed to implement a roam service
// using the @roam/tcp transport library.

import type { EchoHandler } from "@bearcove/roam-generated/echo.ts";
import { echo_methodHandlers } from "@bearcove/roam-generated/echo.ts";
import { Server, type ServiceDispatcher } from "@bearcove/roam-tcp";
import { UnaryDispatcher } from "@bearcove/roam-core";

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
