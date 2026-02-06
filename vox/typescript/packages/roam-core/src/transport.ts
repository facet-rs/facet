/**
 * Message transport abstraction.
 *
 * This module defines the MessageTransport interface that abstracts over different
 * transport mechanisms for sending and receiving roam messages.
 *
 * Implementations:
 * - LengthPrefixedFramed (roam-tcp) for byte streams (TCP)
 * - WsTransport (roam-ws) for WebSocket
 */

/**
 * Interface for transports that can send and receive roam messages.
 *
 * This abstracts over the framing mechanism:
 * - Byte streams need length-prefix framing to delimit messages
 * - Message-oriented transports (WebSocket) have built-in framing
 *
 * Both cases share the same protocol logic in Connection.
 */
export interface MessageTransport {
  /**
   * Send a message (raw postcard-encoded payload for WebSocket,
   * or length-prefixed for byte streams).
   */
  send(payload: Uint8Array): Promise<void>;

  /**
   * Receive a message with timeout.
   *
   * Returns null if:
   * - Timeout expires
   * - Connection is closed cleanly
   */
  recvTimeout(timeoutMs: number): Promise<Uint8Array | null>;

  /**
   * Get the last decoded bytes (for error detection).
   *
   * Used to detect specific error conditions like unknown message variants.
   */
  readonly lastDecoded: Uint8Array;

  /**
   * Close the transport.
   */
  close(): void;
}
