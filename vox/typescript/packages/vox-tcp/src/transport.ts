import net from "node:net";
import { singleLinkSource, type LinkSource } from "@bearcove/vox-core";
import { LengthPrefixedFramed } from "./framing.ts";

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

export class TcpLinkSource implements LinkSource<LengthPrefixedFramed> {
  constructor(private readonly addr: string) {}

  async nextLink(): Promise<{ link: LengthPrefixedFramed }> {
    const { host, port } = parseAddress(this.addr);
    const socket = await new Promise<net.Socket>((resolve, reject) => {
      const candidate = net.createConnection({ host, port }, () => resolve(candidate));
      candidate.on("error", reject);
    });
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
