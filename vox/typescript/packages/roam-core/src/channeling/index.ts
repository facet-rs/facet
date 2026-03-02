// Channeling module exports

export { type ChannelId, Role, ChannelError } from "./types.ts";
export { ChannelIdAllocator } from "./allocator.ts";
export { createChannel, ChannelSender, ChannelReceiver, type Channel } from "./channel.ts";
export {
  ChannelRegistry,
  OutgoingSender,
  type OutgoingMessage,
  type OutgoingPoll,
} from "./registry.ts";
export { Tx, createServerTx } from "./tx.ts";
export { Rx, createServerRx } from "./rx.ts";
export { channel } from "./pair.ts";
export { type TaskMessage, type TaskSender, type ChannelContext } from "./task.ts";

// Runtime descriptor types
export {
  type MethodDescriptor,
  type ServiceDescriptor,
  type RoamCall,
} from "./descriptor.ts";

// Schema types and binding
export {
  type PrimitiveKind,
  type TxSchema,
  type RxSchema,
  type VecSchema,
  type OptionSchema,
  type MapSchema,
  type StructSchema,
  type TupleSchema,
  type EnumVariant,
  type EnumSchema,
  type RefSchema,
  type Schema,
  type SchemaRegistry,
} from "./schema.ts";

// Schema helper functions
export {
  resolveSchema,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  isRefSchema,
} from "./schema.ts";

export { bindChannels } from "./binding.ts";
