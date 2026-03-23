// Vox wire protocol schemas for TypeScript.
//
// Source of truth: generated from rust/vox-types facet shapes.

export type {
  Schema,
  SchemaRegistry,
  TypeRef,
} from "@bearcove/vox-postcard";
export {
  messageSchemasCbor,
  messageSchemaRegistry,
  messageRootRef,
} from "./wire.generated.ts";
