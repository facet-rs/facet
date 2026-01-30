import { parse as parseRaw } from "./parser.js";
import { Value, Scalar, Sequence, StyxObject } from "./types.js";

/**
 * Schema types parsed from a .schema.styx file
 */
interface SchemaType {
  type:
    | "string"
    | "int"
    | "float"
    | "bool"
    | "unit"
    | "any"
    | "object"
    | "seq"
    | "map"
    | "union"
    | "optional"
    | "default"
    | "ref";
  // For @object{...}
  fields?: Record<string, SchemaType>;
  // For @seq(@T), @optional(@T), @map(@V), @default(@V @T)
  inner?: SchemaType;
  // For @map(@K @V) - key type (usually string)
  keyType?: SchemaType;
  // For @union(@A @B ...)
  variants?: SchemaType[];
  // For named type references like @Question
  refName?: string;
  // For @default(value @type)
  defaultValue?: Value;
}

interface ParsedSchema {
  root: SchemaType;
  types: Record<string, SchemaType>;
}

/**
 * Parse a schema file and extract type definitions
 */
function parseSchema(schemaSource: string): ParsedSchema {
  const doc = parseRaw(schemaSource);
  const types: Record<string, SchemaType> = {};
  let root: SchemaType = { type: "any" };

  // Find the schema entry
  const schemaEntry = doc.entries.find((e) => getScalarContent(e.key) === "schema");
  if (!schemaEntry?.value?.payload || schemaEntry.value.payload.type !== "object") {
    throw new Error("Schema file must have a 'schema' object");
  }

  const schemaObj = schemaEntry.value.payload as StyxObject;

  for (const entry of schemaObj.entries) {
    const keyContent = getScalarContent(entry.key);
    const schemaType = parseSchemaType(entry.value);

    // Check for unit key (@) - represented as a Value with no payload and no tag
    const isUnitKey = !entry.key.payload && !entry.key.tag;

    if (isUnitKey || keyContent === "@") {
      // Root type
      root = schemaType;
    } else if (keyContent !== null) {
      // Named type
      types[keyContent] = schemaType;
    }
  }

  return { root, types };
}

function getScalarContent(value: Value | undefined): string | null {
  if (!value?.payload) return null;
  if (value.payload.type === "scalar") {
    return (value.payload as Scalar).text;
  }
  return null;
}

function isUnitValue(value: Value | undefined): boolean {
  // Unit is represented as a tag "@" with no payload, or no payload at all with @ tag
  if (!value) return false;
  // Check for bare @ (tag with name "" or no payload)
  if (value.tag && !value.payload) return true;
  return false;
}

function getTagName(value: Value | undefined): string | null {
  if (!value?.tag) return null;
  return value.tag.name;
}

function parseSchemaType(value: Value | undefined): SchemaType {
  if (!value) return { type: "any" };

  const tagName = getTagName(value);

  if (!tagName) {
    // No tag - could be a bare scalar reference to a named type
    const content = getScalarContent(value);
    if (content) {
      return { type: "ref", refName: content };
    }
    return { type: "any" };
  }

  switch (tagName) {
    case "string":
      return { type: "string" };
    case "int":
      return { type: "int" };
    case "float":
      return { type: "float" };
    case "bool":
      return { type: "bool" };
    case "unit":
      return { type: "unit" };
    case "any":
      return { type: "any" };

    case "object": {
      const fields: Record<string, SchemaType> = {};
      if (value.payload?.type === "object") {
        const obj = value.payload as StyxObject;
        for (const entry of obj.entries) {
          const fieldName = getScalarContent(entry.key);
          if (fieldName) {
            fields[fieldName] = parseSchemaType(entry.value);
          }
        }
      }
      return { type: "object", fields };
    }

    case "seq": {
      // @seq(@T) - payload is sequence with one element
      if (value.payload?.type === "sequence") {
        const seq = value.payload as Sequence;
        if (seq.items.length > 0) {
          return { type: "seq", inner: parseSchemaType(seq.items[0]) };
        }
      }
      return { type: "seq", inner: { type: "any" } };
    }

    case "map": {
      // @map(@V) or @map(@K @V)
      if (value.payload?.type === "sequence") {
        const seq = value.payload as Sequence;
        if (seq.items.length === 1) {
          return { type: "map", inner: parseSchemaType(seq.items[0]) };
        } else if (seq.items.length === 2) {
          return {
            type: "map",
            keyType: parseSchemaType(seq.items[0]),
            inner: parseSchemaType(seq.items[1]),
          };
        }
      }
      return { type: "map", inner: { type: "any" } };
    }

    case "optional": {
      if (value.payload?.type === "sequence") {
        const seq = value.payload as Sequence;
        if (seq.items.length > 0) {
          return { type: "optional", inner: parseSchemaType(seq.items[0]) };
        }
      }
      return { type: "optional", inner: { type: "any" } };
    }

    case "default": {
      // @default(value @type) - first item is default value, second is the type
      if (value.payload?.type === "sequence") {
        const seq = value.payload as Sequence;
        if (seq.items.length >= 2) {
          return {
            type: "default",
            defaultValue: seq.items[0],
            inner: parseSchemaType(seq.items[1]),
          };
        } else if (seq.items.length === 1) {
          // Just a default value, infer type
          return {
            type: "default",
            defaultValue: seq.items[0],
            inner: { type: "any" },
          };
        }
      }
      return { type: "any" };
    }

    case "union": {
      if (value.payload?.type === "sequence") {
        const seq = value.payload as Sequence;
        return {
          type: "union",
          variants: seq.items.map((el: Value) => parseSchemaType(el)),
        };
      }
      return { type: "union", variants: [] };
    }

    default:
      // Unknown tag - treat as reference to named type
      return { type: "ref", refName: tagName };
  }
}

/**
 * Get the default value for a schema type, if it has one
 */
function getDefaultValue(schemaType: SchemaType, types: Record<string, SchemaType>): unknown {
  // Resolve type references
  if (schemaType.type === "ref" && schemaType.refName) {
    const resolved = types[schemaType.refName];
    if (resolved) {
      return getDefaultValue(resolved, types);
    }
    return undefined;
  }

  if (schemaType.type === "default" && schemaType.defaultValue) {
    // Convert the default value using the inner type
    const innerType = schemaType.inner ?? { type: "any" };
    return toTyped(schemaType.defaultValue, innerType, types);
  }

  // Optional fields default to undefined (no value)
  if (schemaType.type === "optional") {
    return undefined;
  }

  return undefined;
}

/**
 * Convert a parsed Styx value to a typed JS value using the schema
 */
function toTyped(
  value: Value | undefined,
  schemaType: SchemaType,
  types: Record<string, SchemaType>,
): unknown {
  if (!value) return undefined;

  // Resolve type references
  if (schemaType.type === "ref" && schemaType.refName) {
    const resolved = types[schemaType.refName];
    if (resolved) {
      return toTyped(value, resolved, types);
    }
    // Unknown type, return as-is
    return toTypedAny(value, types);
  }

  switch (schemaType.type) {
    case "string": {
      const content = getScalarContent(value);
      return content ?? "";
    }

    case "int": {
      const content = getScalarContent(value);
      return content ? parseInt(content, 10) : 0;
    }

    case "float": {
      const content = getScalarContent(value);
      return content ? parseFloat(content) : 0;
    }

    case "bool": {
      const content = getScalarContent(value);
      return content === "true";
    }

    case "unit":
      return null;

    case "any":
      return toTypedAny(value, types);

    case "object": {
      if (value.payload?.type !== "object") {
        return {};
      }
      const obj = value.payload as StyxObject;
      const result: Record<string, unknown> = {};

      // First, collect field names that are present in the value
      const presentFields = new Set<string>();
      for (const entry of obj.entries) {
        const fieldName = getScalarContent(entry.key);
        if (fieldName && schemaType.fields) {
          presentFields.add(fieldName);
          const fieldType = schemaType.fields[fieldName] ?? { type: "any" };
          result[fieldName] = toTyped(entry.value, fieldType, types);
        }
      }

      // Apply defaults for missing fields
      if (schemaType.fields) {
        for (const [fieldName, fieldType] of Object.entries(schemaType.fields)) {
          if (!presentFields.has(fieldName)) {
            const defaultValue = getDefaultValue(fieldType, types);
            if (defaultValue !== undefined) {
              result[fieldName] = defaultValue;
            }
          }
        }
      }
      return result;
    }

    case "seq": {
      if (value.payload?.type !== "sequence") {
        return [];
      }
      const seq = value.payload as Sequence;
      const innerType = schemaType.inner ?? { type: "any" };
      return seq.items.map((el: Value) => toTyped(el, innerType, types));
    }

    case "map": {
      if (value.payload?.type !== "object") {
        return {};
      }
      const obj = value.payload as StyxObject;
      const result: Record<string, unknown> = {};
      const innerType = schemaType.inner ?? { type: "any" };

      for (const entry of obj.entries) {
        const key = getScalarContent(entry.key);
        if (key) {
          result[key] = toTyped(entry.value, innerType, types);
        }
      }
      return result;
    }

    case "optional": {
      const innerType = schemaType.inner ?? { type: "any" };
      return toTyped(value, innerType, types);
    }

    case "default": {
      const innerType = schemaType.inner ?? { type: "any" };
      return toTyped(value, innerType, types);
    }

    case "union": {
      // For unions, try each variant
      // Simple approach: just convert as "any"
      return toTypedAny(value, types);
    }

    default:
      return toTypedAny(value, types);
  }
}

/**
 * Convert a value without schema guidance
 */
function toTypedAny(value: Value | undefined, types: Record<string, SchemaType>): unknown {
  if (!value) return undefined;

  const payload = value.payload;

  // Handle unit (tag with no payload)
  if (!payload) {
    return null;
  }

  switch (payload.type) {
    case "scalar":
      return (payload as Scalar).text;

    case "sequence": {
      const seq = payload as Sequence;
      return seq.items.map((el: Value) => toTypedAny(el, types));
    }

    case "object": {
      const obj = payload as StyxObject;
      const result: Record<string, unknown> = {};
      for (const entry of obj.entries) {
        const key = getScalarContent(entry.key);
        if (key) {
          result[key] = toTypedAny(entry.value, types);
        }
      }
      return result;
    }

    default:
      return undefined;
  }
}

/**
 * Parse a Styx document with a schema, returning typed JS values
 */
export function parseTyped<T = unknown>(source: string, schemaSource: string): T {
  const schema = parseSchema(schemaSource);
  const doc = parseRaw(source);

  // The document is implicitly an object
  const rootValue: Value = {
    payload: {
      type: "object",
      entries: doc.entries,
      span: { start: 0, end: source.length },
    },
    span: { start: 0, end: source.length },
  };

  return toTyped(rootValue, schema.root, schema.types) as T;
}

/**
 * Parse a Styx document without a schema, returning untyped JS values
 */
export function parseUntyped(source: string): unknown {
  const doc = parseRaw(source);

  const rootValue: Value = {
    payload: {
      type: "object",
      entries: doc.entries,
      span: { start: 0, end: source.length },
    },
    span: { start: 0, end: source.length },
  };

  return toTypedAny(rootValue, {});
}
