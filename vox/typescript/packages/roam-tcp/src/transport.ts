// TCP transport for roam connections.

import net from "node:net";
import { LengthPrefixedFramed } from "./framing.ts";
import {
  Connection,
  ConnectionError,
  defaultHello,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  type HelloExchangeOptions,
} from "@bearcove/roam-core";

/** Options for connecting/accepting connections. */
export interface ConnectOptions extends HelloExchangeOptions {}

/** TCP transport for roam connections. */
export class Server {
  /**
   * Connect to a peer address and perform handshake as initiator.
   */
  connect(addr: string, options: ConnectOptions = {}): Promise<Connection> {
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
          const io = new LengthPrefixedFramed(socket);
          const conn = await helloExchangeInitiator(io, defaultHello(), options);
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
  async accept(socket: net.Socket, options: ConnectOptions = {}): Promise<Connection> {
    const io = new LengthPrefixedFramed(socket);
    return helloExchangeAcceptor(io, defaultHello(), options);
  }
}
