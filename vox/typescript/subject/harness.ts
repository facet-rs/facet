// Shared subject harness for vox compliance testing.
//
// Generic over the service being served — callers provide a Dispatcher factory.

import { tcpConnector } from "@bearcove/vox-tcp";
import {
  connect,
  Driver,
  ConnectionError,
  type Dispatcher,
  type Metadata,
} from "@bearcove/vox-core";
import { withSubjectTimeout } from "./timeout.ts";

export async function runSubjectServer(createDispatcher: () => Dispatcher, metadata: Metadata = new Map()): Promise<void> {
  await withSubjectTimeout("server", async () => {
    const addr = process.env.PEER_ADDR;
    if (!addr) {
      throw new Error("PEER_ADDR env var not set");
    }

    const acceptLanes = process.env.ACCEPT_CONNECTIONS !== "0";

    console.error(`server mode: connecting to ${addr}, acceptLanes=${acceptLanes}`);
    const connection = await connect(tcpConnector(addr), {
      metadata,
      onLane: acceptLanes
        ? (lane) => {
            const driver = new Driver(lane, createDispatcher());
            void driver.run();
          }
        : undefined,
    });
    const driver = new Driver(connection.lane(), createDispatcher());
    const handle = connection.handle();

    try {
      await driver.run();
    } catch (e) {
      if (e instanceof ConnectionError) {
        console.error(`[harness] connection error: ${e.message}`);
        return;
      }
      throw e;
    } finally {
      // r[impl hosted.subject.lifecycle]
      handle.shutdown();
      await connection.closed().catch(() => {});
    }
  });
}
