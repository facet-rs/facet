// Roam wire protocol schemas for TypeScript.
//
// Source of truth: generated from rust/roam-types facet shapes.

export type {
  Schema,
  SchemaRegistry,
  TypeRef,
} from "@bearcove/roam-postcard";
export {
  messageSchemasCbor,
  messageSchemaRegistry,
  messageRootRef,
} from "./wire.generated.ts";
