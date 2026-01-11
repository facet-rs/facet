// TCP transport for roam connections.

import net from "node:net";
import { CobsFramed } from "./framing.ts";
import {
  Connection,
  ConnectionError,
  defaultHello,
  helloExchangeAcceptor,
  helloExchangeInitiator,
} from "@bearcove/roam-core";

/** TCP transport for roam connections. */
export class Server {
  /**
   * Connect to a peer address and perform handshake as initiator.
   */
  connect(addr: string): Promise<Connection> {
    return new Promise((resolve, reject) => {
      const lastColon = addr.lastIndexOf(":");
      if (lastColon < 0) {
        reject(new Error(`Invalid address: ${addr}`));
        return;
      }
      const host = addr.slice(0, lastColon);
      const port = Number(addr.slice(lastColon + 1));

      const socket = net.createConnection({ host, port }, async () => {
        try {
          const io = new CobsFramed(socket);
          const conn = await helloExchangeInitiator(io, defaultHello());
          resolve(conn);
        } catch (e) {
          reject(e);
        }
      });

      socket.on("error", (err) => {
        reject(ConnectionError.io(err.message));
      });
    });
  }

  /**
   * Accept a connection from a socket and perform handshake as acceptor.
   */
  async accept(socket: net.Socket): Promise<Connection> {
    const io = new CobsFramed(socket);
    return helloExchangeAcceptor(io, defaultHello());
  }
}
