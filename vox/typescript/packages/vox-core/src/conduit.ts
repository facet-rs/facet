import {
  buildMessageDecoder,
  decodeMessageWith,
  encodeMessage,
  type Message,
  type MessageDecoder,
} from "@bearcove/vox-wire";
import type { Link } from "./link.ts";
import { voxLogger } from "./logger.ts";

export interface Conduit<T> {
  send(item: T): Promise<void>;
  recv(): Promise<T | null>;
  close(): void;
  isClosed(): boolean;
}

/**
 * Build the envelope decoder from the peer's `Message` schema (exchanged
 * in the handshake, as phon schema-closure bytes) against ours.
 * With no peer schema it degenerates to writer==reader.
 */
// r[impl conduit.typeplan]
export function buildMessageDecodePlan(peerSchemaBytes?: Uint8Array): MessageDecoder {
  return buildMessageDecoder(peerSchemaBytes);
}

// r[impl conduit]
// r[impl conduit.bare]
export class BareConduit implements Conduit<Message> {
  private readonly link: Link;
  private readonly decoder: MessageDecoder;

  constructor(link: Link, decoder: MessageDecoder | null = null) {
    this.link = link;
    this.decoder = decoder ?? buildMessageDecoder();
  }

  async send(item: Message): Promise<void> {
    await this.link.send(encodeMessage(item));
  }

  async recv(): Promise<Message | null> {
    const payload = await this.link.recv();
    if (!payload) {
      return null;
    }
    try {
      return decodeMessageWith(this.decoder, payload);
    } catch (e) {
      voxLogger()?.error(`[vox:conduit] decode failed (${payload.length} bytes):`, e);
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
