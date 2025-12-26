/**
 * Rapace RPC protocol implementation.
 */

export {
  FrameFlags,
  type FrameFlagsType,
  hasFlag,
  setFlag,
  clearFlag,
} from "./frame-flags.js";

export {
  MsgDescHot,
  MsgDescHotError,
  INLINE_PAYLOAD_SIZE,
  INLINE_PAYLOAD_SLOT,
  NO_DEADLINE,
  MSG_DESC_HOT_SIZE,
} from "./msg-desc-hot.js";

export { Frame, MIN_FRAME_SIZE } from "./frame.js";

export { computeMethodId, computeMethodIdFromFullName } from "./method-id.js";

export {
  TransportError,
  WebSocketTransport,
  connectWebSocket,
} from "./transport.js";
export type { Transport } from "./transport.js";

export { RapaceClient, RapaceError } from "./client.js";
