import { decodeMessage, encodeMessage, type Message } from "@bearcove/roam-wire";
import type { Link } from "./link.ts";
import { roamLogger } from "./logger.ts";

export interface Conduit<T> {
  send(item: T): Promise<void>;
  recv(): Promise<T | null>;
  close(): void;
  isClosed(): boolean;
}

export class BareConduit implements Conduit<Message> {
  constructor(private readonly link: Link) {}

  async send(item: Message): Promise<void> {
    await this.link.send(encodeMessage(item));
  }

  async recv(): Promise<Message | null> {
    const payload = await this.link.recv();
    if (!payload) {
      return null;
    }
    try {
      return decodeMessage(payload).value;
    } catch (e) {
      roamLogger()?.error(`[roam:conduit] decode failed (${payload.length} bytes):`, e);
      throw e;
    }
  }

  close(): void {
    this.link.close();
  }

  isClosed(): boolean {
    return this.link.isClosed();
  }
}
