import net from "node:net";
import { singleLinkSource, type LinkSource } from "@bearcove/vox-core";
import { LengthPrefixedFramed } from "./framing.ts";

function connectSocket(options: net.NetConnectOpts): Promise<net.Socket> {
  return new Promise<net.Socket>((resolve, reject) => {
    const candidate = net.createConnection(options, () => {
      candidate.off("error", reject);
      resolve(candidate);
    });
    candidate.once("error", reject);
  });
}

function parseAddress(addr: string): { host: string; port: number } {
  const lastColon = addr.lastIndexOf(":");
  if (lastColon < 0) {
    throw new Error(`invalid address: ${addr}`);
  }
  return {
    host: addr.slice(0, lastColon),
    port: Number(addr.slice(lastColon + 1)),
  };
}

// r[impl transport.stream]
// r[impl transport.stream.kinds]
export class TcpLinkSource implements LinkSource<LengthPrefixedFramed> {
  private readonly addr: string;

  constructor(addr: string) {
    this.addr = addr;
  }

  async nextLink(): Promise<{ link: LengthPrefixedFramed }> {
    const { host, port } = parseAddress(this.addr);
    const socket = await connectSocket({ host, port });
    return { link: new LengthPrefixedFramed(socket) };
  }
}

export function tcpConnector(addr: string): TcpLinkSource {
  return new TcpLinkSource(addr);
}

export function connectTcp(addr: string): TcpLinkSource {
  return tcpConnector(addr);
}

export function acceptTcp(socket: net.Socket) {
  return singleLinkSource(new LengthPrefixedFramed(socket));
}

// r[impl transport.stream.local]
export class LocalLink extends LengthPrefixedFramed {
  static async connect(addr: string): Promise<LocalLink> {
    const socket = await connectSocket({ path: addr });
    return new LocalLink(socket);
  }
}

// r[impl transport.stream.local]
export class LocalLinkSource implements LinkSource<LocalLink> {
  private readonly addr: string;

  constructor(addr: string) {
    this.addr = addr;
  }

  async nextLink(): Promise<{ link: LocalLink }> {
    return { link: await LocalLink.connect(this.addr) };
  }
}

type LocalWaiter = {
  resolve: (attachment: { link: LocalLink }) => void;
  reject: (error: Error) => void;
};

// r[impl transport.stream.local]
export class LocalLinkAcceptor implements LinkSource<LocalLink> {
  private readonly pending: LocalLink[] = [];
  private readonly waiters: LocalWaiter[] = [];
  private readonly server: net.Server;
  private closedError: Error | null = null;

  private constructor(server: net.Server) {
    this.server = server;
  }

  static async bind(addr: string): Promise<LocalLinkAcceptor> {
    const server = net.createServer();
    const acceptor = new LocalLinkAcceptor(server);
    server.on("connection", (socket) => {
      acceptor.acceptSocket(socket);
    });
    await new Promise<void>((resolve, reject) => {
      const onError = (error: Error) => {
        server.off("listening", onListening);
        reject(error);
      };
      const onListening = () => {
        server.off("error", onError);
        resolve();
      };
      server.once("error", onError);
      server.once("listening", onListening);
      server.listen(addr);
    });
    server.on("error", (error) => {
      acceptor.fail(error);
    });
    server.on("close", () => {
      acceptor.fail(new Error("LocalLinkAcceptor closed"));
    });
    return acceptor;
  }

  async nextLink(): Promise<{ link: LocalLink }> {
    const link = this.pending.shift();
    if (link) {
      return { link };
    }
    if (this.closedError) {
      throw this.closedError;
    }
    return new Promise((resolve, reject) => {
      this.waiters.push({ resolve, reject });
    });
  }

  close(): Promise<void> {
    const error = new Error("LocalLinkAcceptor closed");
    this.fail(error);
    return new Promise((resolve, reject) => {
      this.server.close((closeError) => {
        if (closeError) {
          reject(closeError);
          return;
        }
        resolve();
      });
    });
  }

  private acceptSocket(socket: net.Socket): void {
    const attachment = { link: new LocalLink(socket) };
    const waiter = this.waiters.shift();
    if (waiter) {
      waiter.resolve(attachment);
      return;
    }
    this.pending.push(attachment.link);
  }

  private fail(error: Error): void {
    if (this.closedError) {
      return;
    }
    this.closedError = error;
    for (const waiter of this.waiters.splice(0)) {
      waiter.reject(error);
    }
  }
}

export function localConnector(addr: string): LocalLinkSource {
  return new LocalLinkSource(addr);
}

export function connectLocal(addr: string): LocalLinkSource {
  return localConnector(addr);
}

export function listenLocal(addr: string): Promise<LocalLinkAcceptor> {
  return LocalLinkAcceptor.bind(addr);
}

export function acceptLocal(socket: net.Socket) {
  return singleLinkSource(new LocalLink(socket));
}
