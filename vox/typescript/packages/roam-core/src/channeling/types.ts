// Channel type definitions

/** Channel ID type (matches wire format). */
export type ChannelId = bigint;

/** Connection role - determines channel ID parity. */
export const Role = {
  /** Initiator (client) uses odd channel IDs (1, 3, 5, ...). */
  Initiator: "initiator",
  /** Acceptor (server) uses even channel IDs (2, 4, 6, ...). */
  Acceptor: "acceptor",
} as const;
export type Role = (typeof Role)[keyof typeof Role];

/** Error types for channel operations. */
export class ChannelError extends Error {
  constructor(
    public kind:
      | "unknown"
      | "dataAfterClose"
      | "closed"
      | "serialize"
      | "deserialize"
      | "notBound"
      | "alreadyConsumed",
    message: string,
  ) {
    super(message);
    this.name = "ChannelError";
  }

  static unknown(channelId: ChannelId): ChannelError {
    return new ChannelError("unknown", `unknown channel ID: ${channelId}`);
  }

  static dataAfterClose(channelId: ChannelId): ChannelError {
    return new ChannelError("dataAfterClose", `data after close on channel ${channelId}`);
  }

  static closed(): ChannelError {
    return new ChannelError("closed", "channel closed");
  }

  static serialize(cause: unknown): ChannelError {
    return new ChannelError("serialize", `serialize error: ${cause}`);
  }

  static deserialize(cause: unknown): ChannelError {
    return new ChannelError("deserialize", `deserialize error: ${cause}`);
  }

  static notBound(handle: "Tx" | "Rx"): ChannelError {
    return new ChannelError(
      "notBound",
      `${handle} not bound - pass the other end to a method first`,
    );
  }

  static alreadyConsumed(handle: "Tx" | "Rx"): ChannelError {
    return new ChannelError(
      "alreadyConsumed",
      `${handle} already consumed - channels can only be used once`,
    );
  }
}
