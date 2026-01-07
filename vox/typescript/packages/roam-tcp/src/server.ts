// TCP server for accepting roam connections.

import net from "node:net";
import { CobsFramed } from "./framing.ts";
import {
  Connection,
  ConnectionError,
  defaultHello,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  type ServiceDispatcher,
} from "./connection.ts";

/** Configuration for the server. */
export interface ServerConfig {
  /** Max payload size to advertise in Hello. */
  maxPayloadSize: number;
  /** Initial stream credit to advertise in Hello. */
  initialStreamCredit: number;
}

/** Default server configuration. */
export function defaultServerConfig(): ServerConfig {
  return {
    maxPayloadSize: 1024 * 1024,
    initialStreamCredit: 64 * 1024,
  };
}

/** A TCP server that accepts roam connections. */
export class Server {
  private config: ServerConfig;

  constructor(config?: Partial<ServerConfig>) {
    this.config = { ...defaultServerConfig(), ...config };
  }

  private makeHello() {
    return {
      variant: 0 as const,
      maxPayloadSize: this.config.maxPayloadSize,
      initialStreamCredit: this.config.initialStreamCredit,
    };
  }

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
          const conn = await helloExchangeInitiator(io, this.makeHello());
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
    return helloExchangeAcceptor(io, this.makeHello());
  }

  /**
   * Run a single connection with a dispatcher.
   *
   * This connects to the peer (from PEER_ADDR env var), performs handshake,
   * and runs the message loop until the connection closes.
   */
  async runSubject(dispatcher: ServiceDispatcher): Promise<void> {
    const addr = process.env.PEER_ADDR;
    if (!addr) {
      throw ConnectionError.dispatch("PEER_ADDR env var not set");
    }

    const conn = await this.connect(addr);
    try {
      await conn.run(dispatcher);
    } catch (e) {
      if (e instanceof ConnectionError && e.kind === "closed") {
        // Clean shutdown
        return;
      }
      throw e;
    }
  }
}
