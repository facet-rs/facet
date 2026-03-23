// Canonical schema types shared by handshake, method args/ret, and translation plans.

/** Content hash uniquely identifying a type's postcard-level structure. */
export type SchemaHash = bigint;

/**
 * A reference to a type in a schema. Matches Rust's `TypeRef`.
 * Discriminated by `tag`.
 */
export type TypeRef =
  | { tag: "concrete"; type_id: SchemaHash; args: TypeRef[] }
  | { tag: "var"; name: string };

/** A complete schema for a single type. */
export interface Schema {
  id: SchemaHash;
  type_params: string[];
  kind: SchemaKind;
}

/**
 * The structural kind of a type. Matches Rust's `SchemaKind`.
 * Discriminated by `tag`.
 */
export type SchemaKind =
  | { tag: "struct"; name: string; fields: FieldSchema[] }
  | { tag: "enum"; name: string; variants: VariantSchema[] }
  | { tag: "tuple"; elements: TypeRef[] }
  | { tag: "list"; element: TypeRef }
  | { tag: "map"; key: TypeRef; value: TypeRef }
  | { tag: "array"; element: TypeRef; length: number }
  | { tag: "option"; element: TypeRef }
  | { tag: "channel"; direction: ChannelDirection; element: TypeRef }
  | { tag: "primitive"; primitive_type: PrimitiveType };

/** Primitive types supported by the wire format. */
export type PrimitiveType =
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
export type ChannelDirection = "tx" | "rx";

/** Describes a single field in a struct or struct variant. */
export interface FieldSchema {
  name: string;
  type_ref: TypeRef;
  required: boolean;
}

/** Describes a single variant in an enum. */
export interface VariantSchema {
  name: string;
  index: number;
  payload: VariantPayload;
}

/** The payload of an enum variant. Discriminated by `tag`. */
export type VariantPayload =
  | { tag: "unit" }
  | { tag: "newtype"; type_ref: TypeRef }
  | { tag: "tuple"; types: TypeRef[] }
  | { tag: "struct"; fields: FieldSchema[] };

/** Registry mapping `SchemaHash` to `Schema`. */
export type SchemaRegistry = Map<SchemaHash, Schema>;

/** Schema payload exchanged on the wire for method bindings. */
export interface SchemaPayload {
  schemas: Schema[];
  root: TypeRef;
}

/** Binding direction for method schema bindings. */
export type BindingDirection = "args" | "response";

/**
 * Look up the schema for a `TypeRef` in the registry and return the
 * schema's kind with all type variables substituted.
 */
export function resolveTypeRef(
  ref_: TypeRef,
  registry: SchemaRegistry,
): SchemaKind | undefined {
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

  const subst = new Map<string, TypeRef>();
  for (let i = 0; i < schema.type_params.length && i < ref_.args.length; i++) {
    subst.set(schema.type_params[i], ref_.args[i]);
  }
  return substituteTypeRefs(schema.kind, subst);
}

function substituteTypeRef(
  ref_: TypeRef,
  subst: Map<string, TypeRef>,
): TypeRef {
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
  kind: SchemaKind,
  subst: Map<string, TypeRef>,
): SchemaKind {
  const sub = (ref_: TypeRef) => substituteTypeRef(ref_, subst);

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
  payload: VariantPayload,
  subst: Map<string, TypeRef>,
): VariantPayload {
  const sub = (ref_: TypeRef) => substituteTypeRef(ref_, subst);

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
