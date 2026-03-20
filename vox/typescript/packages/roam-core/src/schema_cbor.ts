// CBOR encode/decode for the new wire schema format.
//
// The Rust side uses #[facet(tag = "tag", rename_all = "snake_case")] on schema
// enums, producing internally-tagged CBOR. decodeCbor() output maps directly to
// our Wire* TypeScript types with no reshaping needed.

import type {
  WireSchemaPayload,
  WireSchema,
  WireSchemaKind,
  WireTypeRef,
  WireVariantPayload,
  WireFieldSchema,
  WireVariantSchema,
  WireMethodSchemaBinding,
  SchemaHash,
  WireBindingDirection,
  WirePrimitiveType,
  WireChannelDirection,
} from "@bearcove/roam-postcard";
import {
  decodeCbor,
  cborMap,
  cborText,
  cborArray,
  cborUint64,
  cborBool,
  cborUint,
  type CborMap,
} from "./cbor.ts";

// ============================================================================
// Decode: CBOR → WireSchemaPayload
// ============================================================================

/**
 * Decode a CBOR-encoded schema payload into typed Wire* objects.
 *
 * Because facet-cbor now uses internally-tagged enums, the decoded CBOR is
 * already the right shape — we just need to cast and validate bigints.
 */
export function decodeSchemaPayload(bytes: Uint8Array): WireSchemaPayload {
  const { value } = decodeCbor(bytes);
  const raw = value as CborMap;

  const schemas = (raw["schemas"] as CborMap[]).map(decodeSchema);
  const method_bindings = (raw["method_bindings"] as CborMap[] | undefined ?? []).map(
    decodeMethodBinding,
  );

  return { schemas, method_bindings };
}

function decodeSchema(raw: CborMap): WireSchema {
  return {
    id: raw["id"] as SchemaHash,
    type_params: (raw["type_params"] as string[] | undefined) ?? [],
    kind: raw["kind"] as WireSchemaKind,
  };
}

function decodeMethodBinding(raw: CborMap): WireMethodSchemaBinding {
  return {
    method_id: raw["method_id"] as bigint,
    root_type_ref: raw["root_type_ref"] as WireTypeRef,
    direction: raw["direction"] as WireBindingDirection,
  };
}

// ============================================================================
// Encode: WireSchemaPayload → CBOR
// ============================================================================

/**
 * Encode a schema payload to CBOR, producing bytes that facet-cbor on the
 * Rust side can deserialize (internally-tagged enums).
 */
export function encodeSchemaPayload(payload: WireSchemaPayload): Uint8Array {
  const schemas = cborArray(payload.schemas.map(encodeSchema));
  const bindings = cborArray(payload.method_bindings.map(encodeMethodBinding));
  return cborMap([
    ["schemas", schemas],
    ["method_bindings", bindings],
  ]);
}

function encodeSchema(schema: WireSchema): Uint8Array {
  const entries: [string, Uint8Array][] = [
    ["id", cborUint64(schema.id)],
  ];
  if (schema.type_params.length > 0) {
    entries.push(["type_params", cborArray(schema.type_params.map(cborText))]);
  }
  entries.push(["kind", encodeSchemaKind(schema.kind)]);
  return cborMap(entries);
}

function encodeSchemaKind(kind: WireSchemaKind): Uint8Array {
  // Internally-tagged: struct variants become { "tag": "variant_name", ...fields }
  switch (kind.tag) {
    case "struct":
      return cborMap([
        ["tag", cborText("struct")],
        ["name", cborText(kind.name)],
        ["fields", cborArray(kind.fields.map(encodeFieldSchema))],
      ]);
    case "enum":
      return cborMap([
        ["tag", cborText("enum")],
        ["name", cborText(kind.name)],
        ["variants", cborArray(kind.variants.map(encodeVariantSchema))],
      ]);
    case "tuple":
      return cborMap([
        ["tag", cborText("tuple")],
        ["elements", cborArray(kind.elements.map(encodeTypeRef))],
      ]);
    case "list":
      return cborMap([
        ["tag", cborText("list")],
        ["element", encodeTypeRef(kind.element)],
      ]);
    case "map":
      return cborMap([
        ["tag", cborText("map")],
        ["key", encodeTypeRef(kind.key)],
        ["value", encodeTypeRef(kind.value)],
      ]);
    case "array":
      return cborMap([
        ["tag", cborText("array")],
        ["element", encodeTypeRef(kind.element)],
        ["length", cborUint64(BigInt(kind.length))],
      ]);
    case "option":
      return cborMap([
        ["tag", cborText("option")],
        ["element", encodeTypeRef(kind.element)],
      ]);
    case "channel":
      return cborMap([
        ["tag", cborText("channel")],
        ["direction", encodeChannelDirection(kind.direction)],
        ["element", encodeTypeRef(kind.element)],
      ]);
    case "primitive":
      return cborMap([
        ["tag", cborText("primitive")],
        ["primitive_type", encodePrimitiveType(kind.primitive_type)],
      ]);
  }
}

function encodeTypeRef(ref_: WireTypeRef): Uint8Array {
  switch (ref_.tag) {
    case "concrete":
      return cborMap([
        ["tag", cborText("concrete")],
        ["type_id", cborUint64(ref_.type_id)],
        ["args", cborArray(ref_.args.map(encodeTypeRef))],
      ]);
    case "var":
      return cborMap([
        ["tag", cborText("var")],
        ["name", cborText(ref_.name)],
      ]);
  }
}

function encodeFieldSchema(field: WireFieldSchema): Uint8Array {
  return cborMap([
    ["name", cborText(field.name)],
    ["type_ref", encodeTypeRef(field.type_ref)],
    ["required", cborBool(field.required)],
  ]);
}

function encodeVariantSchema(variant: WireVariantSchema): Uint8Array {
  return cborMap([
    ["name", cborText(variant.name)],
    ["index", cborUint(variant.index)],
    ["payload", encodeVariantPayload(variant.payload)],
  ]);
}

function encodeVariantPayload(payload: WireVariantPayload): Uint8Array {
  // Internally-tagged enum
  switch (payload.tag) {
    case "unit":
      return cborText("unit");
    case "newtype":
      return cborMap([
        ["tag", cborText("newtype")],
        ["type_ref", encodeTypeRef(payload.type_ref)],
      ]);
    case "tuple":
      return cborMap([
        ["tag", cborText("tuple")],
        ["types", cborArray(payload.types.map(encodeTypeRef))],
      ]);
    case "struct":
      return cborMap([
        ["tag", cborText("struct")],
        ["fields", cborArray(payload.fields.map(encodeFieldSchema))],
      ]);
  }
}

/** Unit variant → just the string name */
function encodePrimitiveType(pt: WirePrimitiveType): Uint8Array {
  return cborText(pt);
}

/** Unit variant → just the string name */
function encodeChannelDirection(dir: WireChannelDirection): Uint8Array {
  return cborText(dir);
}

function encodeMethodBinding(binding: WireMethodSchemaBinding): Uint8Array {
  return cborMap([
    ["method_id", cborUint64(binding.method_id)],
    ["root_type_ref", encodeTypeRef(binding.root_type_ref)],
    ["direction", cborText(binding.direction)],
  ]);
}
