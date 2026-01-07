// Stream ID allocator with correct parity based on role.

import { type StreamId, Role } from "./types.ts";

/**
 * Allocates unique stream IDs with correct parity.
 *
 * r[impl streaming.id.uniqueness] - IDs are unique within a connection.
 * r[impl streaming.id.parity] - Initiator uses odd, Acceptor uses even.
 */
export class StreamIdAllocator {
  private nextId: bigint;

  constructor(role: Role) {
    this.nextId = role === Role.Initiator ? 1n : 2n;
  }

  /** Allocate the next stream ID. */
  next(): StreamId {
    const id = this.nextId;
    this.nextId += 2n;
    return id;
  }
}
