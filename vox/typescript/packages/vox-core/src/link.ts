export interface Link {
  send(payload: Uint8Array): Promise<void>;
  recv(): Promise<Uint8Array | null>;
  close(): void;
  isClosed(): boolean;
  readonly lastReceived?: Uint8Array;
}

export interface LinkAttachment<L extends Link = Link> {
  link: L;
  clientHello?: Uint8Array;
}

export interface LinkSource<L extends Link = Link> {
  nextLink(): Promise<LinkAttachment<L>>;
}

export function singleLinkSource<L extends Link = Link>(
  link: L,
  clientHello?: Uint8Array,
): LinkSource<L> {
  let used = false;
  return {
    async nextLink(): Promise<LinkAttachment<L>> {
      if (used) {
        throw new Error("single-use LinkSource exhausted");
      }
      used = true;
      return { link, clientHello };
    },
  };
}
