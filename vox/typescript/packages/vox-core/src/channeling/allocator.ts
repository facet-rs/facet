// Channel ID allocator with correct parity based on role.

import { type ChannelId, Role } from "./types.ts";

/**
 * Allocates unique channel IDs with correct parity.
 *
 * r[impl lane.request-channel-parity]
 * r[impl rpc.channel.allocation]
 */
export class ChannelIdAllocator {
  private nextId: bigint;

  constructor(role: Role) {
    this.nextId = role === Role.Initiator ? 1n : 2n;
  }

  /** Allocate the next channel ID. */
  next(): ChannelId {
    const id = this.nextId;
    this.nextId += 2n;
    return id;
  }
}
