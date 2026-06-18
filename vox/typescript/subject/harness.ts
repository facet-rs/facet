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
        ? (_request, pending) => {
            void pending.accept().then((lane) => {
              const driver = new Driver(lane, createDispatcher());
              void driver.run().catch((e: unknown) => {
                console.error(`[harness] service lane error: ${e instanceof Error ? e.message : String(e)}`);
              });
            }).catch((e: unknown) => {
              console.error(`[harness] service lane accept error: ${e instanceof Error ? e.message : String(e)}`);
            });
          }
        : undefined,
    });
    const handle = connection.handle();

    try {
      await connection.closed();
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
