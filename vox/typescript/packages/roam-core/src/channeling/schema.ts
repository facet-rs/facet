// Schema types for runtime channel binding.
//
// Re-exported from @bearcove/roam-postcard which is the canonical source.
// This module exists for backward compatibility and convenience.

export type {
  PrimitiveKind,
  VecSchema,
  OptionSchema,
  MapSchema,
  StructSchema,
  TupleSchema,
  EnumVariant,
  EnumSchema,
  RefSchema,
  TxSchema,
  RxSchema,
  Schema,
  SchemaRegistry,
} from "@bearcove/roam-postcard";

export {
  resolveSchema,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  isRefSchema,
} from "@bearcove/roam-postcard";
