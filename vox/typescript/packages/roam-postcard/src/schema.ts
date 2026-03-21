// Canonical schema types shared by handshake, method args/ret, and translation plans.

/** Content hash uniquely identifying a type's postcard-level structure. */
export type SchemaHash = bigint;

/**
 * A reference to a type in a schema. Matches Rust's `TypeRef`.
 * Discriminated by `tag`.
 */
export type WireTypeRef =
  | { tag: "concrete"; type_id: SchemaHash; args: WireTypeRef[] }
  | { tag: "var"; name: string };

/** A complete schema for a single type. */
export interface WireSchema {
  id: SchemaHash;
  type_params: string[];
  kind: WireSchemaKind;
}

/**
 * The structural kind of a type. Matches Rust's `SchemaKind`.
 * Discriminated by `tag`.
 */
export type WireSchemaKind =
  | { tag: "struct"; name: string; fields: WireFieldSchema[] }
  | { tag: "enum"; name: string; variants: WireVariantSchema[] }
  | { tag: "tuple"; elements: WireTypeRef[] }
  | { tag: "list"; element: WireTypeRef }
  | { tag: "map"; key: WireTypeRef; value: WireTypeRef }
  | { tag: "array"; element: WireTypeRef; length: number }
  | { tag: "option"; element: WireTypeRef }
  | { tag: "channel"; direction: WireChannelDirection; element: WireTypeRef }
  | { tag: "primitive"; primitive_type: WirePrimitiveType };

/** Primitive types supported by the wire format. */
export type WirePrimitiveType =
  | "bool"
  | "u8"
  | "u16"
  | "u32"
  | "u64"
  | "u128"
  | "i8"
  | "i16"
  | "i32"
  | "i64"
  | "i128"
  | "f32"
  | "f64"
  | "char"
  | "string"
  | "unit"
  | "never"
  | "bytes"
  | "payload";

/** Channel direction. */
export type WireChannelDirection = "tx" | "rx";

/** Describes a single field in a struct or struct variant. */
export interface WireFieldSchema {
  name: string;
  type_ref: WireTypeRef;
  required: boolean;
}

/** Describes a single variant in an enum. */
export interface WireVariantSchema {
  name: string;
  index: number;
  payload: WireVariantPayload;
}

/** The payload of an enum variant. Discriminated by `tag`. */
export type WireVariantPayload =
  | { tag: "unit" }
  | { tag: "newtype"; type_ref: WireTypeRef }
  | { tag: "tuple"; types: WireTypeRef[] }
  | { tag: "struct"; fields: WireFieldSchema[] };

/** Registry mapping `SchemaHash` to `WireSchema`. */
export type WireSchemaRegistry = Map<SchemaHash, WireSchema>;

/** Schema payload exchanged on the wire for method bindings. */
export interface WireSchemaPayload {
  schemas: WireSchema[];
  root: WireTypeRef;
}

/** Binding direction for method schema bindings. */
export type WireBindingDirection = "args" | "response";

/**
 * Look up the schema for a `WireTypeRef` in the registry and return the
 * schema's kind with all type variables substituted.
 */
export function resolveWireTypeRef(
  ref_: WireTypeRef,
  registry: WireSchemaRegistry,
): WireSchemaKind | undefined {
  if (ref_.tag === "var") {
    return undefined;
  }

  const schema = registry.get(ref_.type_id);
  if (!schema) {
    return undefined;
  }
  if (ref_.args.length === 0) {
    return schema.kind;
  }

  const subst = new Map<string, WireTypeRef>();
  for (let i = 0; i < schema.type_params.length && i < ref_.args.length; i++) {
    subst.set(schema.type_params[i], ref_.args[i]);
  }
  return substituteTypeRefs(schema.kind, subst);
}

function substituteTypeRef(
  ref_: WireTypeRef,
  subst: Map<string, WireTypeRef>,
): WireTypeRef {
  if (ref_.tag === "var") {
    return subst.get(ref_.name) ?? ref_;
  }

  return {
    tag: "concrete",
    type_id: ref_.type_id,
    args: ref_.args.map((arg) => substituteTypeRef(arg, subst)),
  };
}

function substituteTypeRefs(
  kind: WireSchemaKind,
  subst: Map<string, WireTypeRef>,
): WireSchemaKind {
  const sub = (ref_: WireTypeRef) => substituteTypeRef(ref_, subst);

  switch (kind.tag) {
    case "primitive":
      return kind;
    case "struct":
      return {
        ...kind,
        fields: kind.fields.map((field) => ({ ...field, type_ref: sub(field.type_ref) })),
      };
    case "enum":
      return {
        ...kind,
        variants: kind.variants.map((variant) => ({
          ...variant,
          payload: substitutePayload(variant.payload, subst),
        })),
      };
    case "tuple":
      return { ...kind, elements: kind.elements.map(sub) };
    case "list":
      return { ...kind, element: sub(kind.element) };
    case "map":
      return { ...kind, key: sub(kind.key), value: sub(kind.value) };
    case "array":
      return { ...kind, element: sub(kind.element) };
    case "option":
      return { ...kind, element: sub(kind.element) };
    case "channel":
      return { ...kind, element: sub(kind.element) };
  }
}

function substitutePayload(
  payload: WireVariantPayload,
  subst: Map<string, WireTypeRef>,
): WireVariantPayload {
  const sub = (ref_: WireTypeRef) => substituteTypeRef(ref_, subst);

  switch (payload.tag) {
    case "unit":
      return payload;
    case "newtype":
      return { ...payload, type_ref: sub(payload.type_ref) };
    case "tuple":
      return { ...payload, types: payload.types.map(sub) };
    case "struct":
      return {
        ...payload,
        fields: payload.fields.map((field) => ({ ...field, type_ref: sub(field.type_ref) })),
      };
  }
}
