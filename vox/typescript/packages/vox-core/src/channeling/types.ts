// Channel type definitions

/** Channel ID type (matches wire format). */
export type ChannelId = bigint;

/** Default per-channel initial credit when the const generic `N` is omitted. */
// r[impl rpc.flow-control.credit.initial]
export const DEFAULT_INITIAL_CREDIT = 16;

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
  public kind:
    | "unknown"
    | "dataAfterClose"
    | "closed"
    | "reset"
    | "requestClosed"
    | "cancelled"
    | "timedOut"
    | "connectionClosed"
    | "serialize"
    | "deserialize"
    | "notBound"
    | "alreadyConsumed";

  constructor(
    kind:
      | "unknown"
      | "dataAfterClose"
      | "closed"
      | "reset"
      | "requestClosed"
      | "cancelled"
      | "timedOut"
      | "connectionClosed"
      | "serialize"
      | "deserialize"
      | "notBound"
      | "alreadyConsumed",
    message: string,
  ) {
    super(message);
    this.kind = kind;
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

  static reset(channelId?: ChannelId): ChannelError {
    return new ChannelError(
      "reset",
      channelId === undefined ? "channel reset" : `channel ${channelId} reset`,
    );
  }

  static requestClosed(): ChannelError {
    return new ChannelError("requestClosed", "request scope ended before channel close");
  }

  static cancelled(): ChannelError {
    return new ChannelError("cancelled", "request cancelled before channel close");
  }

  static timedOut(): ChannelError {
    return new ChannelError("timedOut", "request timed out before channel close");
  }

  static connectionClosed(): ChannelError {
    return new ChannelError("connectionClosed", "connection closed while channel was live");
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
