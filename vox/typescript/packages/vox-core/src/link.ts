import type { PeerEvidence } from "./handshake.ts";

// r[impl link]
// r[impl link.message]
// r[impl link.rx.recv]
// r[impl link.rx.eof]
// r[impl link.rx.error]
// r[impl link.tx.send]
// r[impl link.tx.close]
export interface Link {
  send(payload: Uint8Array): Promise<void>;
  recv(): Promise<Uint8Array | null>;
  close(): void;
  isClosed(): boolean;
  readonly lastReceived?: Uint8Array;
}

// r[impl link]
// r[impl link.split]
export interface LinkAttachment<L extends Link = Link> {
  link: L;
  clientHello?: Uint8Array;
  peerEvidence?: PeerEvidence;
}

// r[impl link.split]
export interface LinkSource<L extends Link = Link> {
  nextLink(): Promise<LinkAttachment<L>>;
}

// r[impl link.split]
export function singleLinkSource<L extends Link = Link>(
  link: L,
  clientHello?: Uint8Array,
  peerEvidence?: PeerEvidence,
): LinkSource<L> {
  let used = false;
  return {
    async nextLink(): Promise<LinkAttachment<L>> {
      if (used) {
        throw new Error("single-use LinkSource exhausted");
      }
      used = true;
      return { link, clientHello, peerEvidence };
    },
  };
}
