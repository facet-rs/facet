// Channeling module exports

export { type ChannelId, Role, ChannelError, DEFAULT_INITIAL_CREDIT } from "./types.ts";
export { ChannelIdAllocator } from "./allocator.ts";
export { createChannel, ChannelSender, ChannelReceiver, type Channel } from "./channel.ts";
export {
  ChannelRegistry,
  OutgoingSender,
  type OutgoingMessage,
  type OutgoingPoll,
  type OutgoingTrySendDetail,
  type ChannelDebugContext,
  type ChannelDebugSnapshot,
  type ChannelRegistryDebugSnapshot,
} from "./registry.ts";
export { Tx, createServerTx, type TrySendResult, type TrySendDetailedResult } from "./tx.ts";
export { Rx, createServerRx } from "./rx.ts";
export { channel } from "./pair.ts";
export { type TaskMessage, type TaskSender, type ChannelContext } from "./task.ts";

// Runtime descriptor types
export {
  type MethodDescriptor,
  type ServiceDescriptor,
  type VoxCall,
} from "./descriptor.ts";

export {
  bindPhonChannels,
  type BoundChannels,
  type ChannelCredit,
} from "./binding.ts";
