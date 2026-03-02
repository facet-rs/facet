// Roam wire protocol schemas for TypeScript.
//
// Source of truth: generated from rust/roam-types facet shapes.

import type { Schema, SchemaRegistry } from "@bearcove/roam-postcard";
export {
  ParitySchema,
  ConnectionSettingsSchema,
  MetadataValueSchema,
  MetadataEntrySchema,
  HelloSchema,
  HelloYourselfSchema,
  ProtocolErrorSchema,
  PingSchema,
  PongSchema,
  ConnectionOpenSchema,
  ConnectionAcceptSchema,
  ConnectionRejectSchema,
  ConnectionCloseSchema,
  RequestBodySchema,
  RequestMessageSchema,
  ChannelBodySchema,
  ChannelMessageSchema,
  MessagePayloadSchema,
  MessageSchema,
  wireSchemaRegistry,
} from "./schemas.generated.ts";

export { type Schema, type SchemaRegistry };
