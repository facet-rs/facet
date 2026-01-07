// Streaming type definitions

/** Stream ID type (matches wire format). */
export type StreamId = bigint;

/** Connection role - determines stream ID parity. */
export const Role = {
  /** Initiator (client) uses odd stream IDs (1, 3, 5, ...). */
  Initiator: "initiator",
  /** Acceptor (server) uses even stream IDs (2, 4, 6, ...). */
  Acceptor: "acceptor",
} as const;
export type Role = (typeof Role)[keyof typeof Role];

/** Error types for streaming operations. */
export class StreamError extends Error {
  constructor(
    public kind: "unknown" | "dataAfterClose" | "closed" | "serialize" | "deserialize",
    message: string,
  ) {
    super(message);
    this.name = "StreamError";
  }

  static unknown(streamId: StreamId): StreamError {
    return new StreamError("unknown", `unknown stream ID: ${streamId}`);
  }

  static dataAfterClose(streamId: StreamId): StreamError {
    return new StreamError("dataAfterClose", `data after close on stream ${streamId}`);
  }

  static closed(): StreamError {
    return new StreamError("closed", "stream closed");
  }

  static serialize(cause: unknown): StreamError {
    return new StreamError("serialize", `serialize error: ${cause}`);
  }

  static deserialize(cause: unknown): StreamError {
    return new StreamError("deserialize", `deserialize error: ${cause}`);
  }
}
