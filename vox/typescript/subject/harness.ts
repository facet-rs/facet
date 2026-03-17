// Shared subject harness for roam compliance testing.
//
// Generic over the service being served — callers provide a Dispatcher factory.

import { tcpConnector } from "@bearcove/roam-tcp";
import {
  Driver,
  SessionError,
  session,
  type Dispatcher,
  type SessionConduitKind,
} from "@bearcove/roam-core";

function subjectConduit(): SessionConduitKind {
  return process.env.SPEC_CONDUIT === "stable" ? "stable" : "bare";
}

export async function runSubjectServer(createDispatcher: () => Dispatcher): Promise<void> {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  const acceptConnections = process.env.ACCEPT_CONNECTIONS === "1";

  console.error(`server mode: connecting to ${addr}, acceptConnections=${acceptConnections}`);
  const established = await session.initiator(tcpConnector(addr), {
    transport: subjectConduit(),
    onConnection: acceptConnections
      ? (connection) => {
          const driver = new Driver(connection, createDispatcher());
          void driver.run();
        }
      : undefined,
  });
  const root = established.rootConnection();
  const driver = new Driver(root, createDispatcher());

  try {
    await driver.run();
  } catch (e) {
    if (e instanceof SessionError) {
      return;
    }
    throw e;
  }
}
