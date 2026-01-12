// WebSocket transport for roam messages.
//
// r[impl transport.message.one-to-one] - WebSocket provides message framing.
// r[impl transport.message.binary] - Uses binary WebSocket frames.

export { WsTransport, connectWs } from "./transport.ts";
export {
  ReconnectingWsClient,
  createReconnectingClient,
  type ReconnectingClientConfig,
  type ConnectionState,
  type BackoffConfig,
  ClientClosedError,
  ReconnectFailedError,
} from "./reconnecting.ts";
