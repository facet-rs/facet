// Roam wire protocol schemas for TypeScript.
//
// Source of truth: generated from rust/roam-types facet shapes.

export type {
  WireSchema as Schema,
  WireSchemaRegistry as SchemaRegistry,
  WireTypeRef as TypeRef,
} from "@bearcove/roam-postcard";
export {
  wireMessageSchemasCbor,
  wireMessageSchemaRegistry,
  wireMessageRootRef,
  wireSchemaRegistry,
  RequestBodySchema,
  MessageSchema,
} from "./wire.generated.ts";
