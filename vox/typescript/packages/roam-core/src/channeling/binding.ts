// Runtime channel binder.
//
// Walks argument structures to find and bind Tx/Rx channels. The canonical
// RPC path uses Rust-shaped schema refs; the legacy TS schema AST remains here
// only for backward-compatible helpers.

import type { Schema, SchemaRegistry } from "./schema.ts";
import {
  findVariantByName,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  resolveSchema,
} from "./schema.ts";
import type { ChannelIdAllocator } from "./allocator.ts";
import type { ChannelRegistry } from "./registry.ts";
import { Tx } from "./tx.ts";
import { Rx } from "./rx.ts";
import { DEFAULT_INITIAL_CREDIT } from "./types.ts";
import {
  encodeWithSchema,
  decodeWithSchema,
  encodeWithTypeRef,
  decodeWithTypeRef,
  resolveWireTypeRef,
  type WireSchemaRegistry,
  type WireTypeRef,
  type WireSchemaKind,
  type WireVariantPayload,
} from "@bearcove/roam-postcard";

/**
 * Bind all Tx/Rx channels found in the arguments.
 *
 * Walks the argument structure using the provided schema, finds any
 * Tx/Rx channels, allocates channel IDs for them, and binds them to
 * the registry.
 *
 * Returns the channel IDs in declaration order, for inclusion in the
 * Request message's `channels` field.
 *
 * r[impl call.request.channels] - Collects channel IDs in declaration order.
 *
 * @param schemas - Schema for each argument
 * @param args - The actual argument values
 * @param allocator - Allocator for channel IDs
 * @param registry - Registry to bind channels to
 * @returns Channel IDs in declaration order
 */
export function bindChannels(
  schemas: Schema[],
  args: unknown[],
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  schemaRegistry?: SchemaRegistry,
): bigint[] {
  const channelIds: bigint[] = [];
  for (let i = 0; i < schemas.length; i++) {
    bindValue(schemas[i], args[i], allocator, registry, channelIds, schemaRegistry);
  }
  return channelIds;
}

export function finalizeBoundChannels(
  schemas: Schema[],
  args: unknown[],
  schemaRegistry?: SchemaRegistry,
): void {
  for (let i = 0; i < schemas.length; i++) {
    finalizeValue(schemas[i], args[i], schemaRegistry);
  }
}

/**
 * Bind all Tx/Rx channels found in canonical args type refs.
 *
 * Generated roam clients use this path. `schemas` must be the tuple element
 * refs for the method args shape.
 */
export function bindChannelsForTypeRefs(
  schemas: WireTypeRef[],
  args: unknown[],
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  schemaRegistry: WireSchemaRegistry,
): bigint[] {
  const channelIds: bigint[] = [];
  for (let i = 0; i < schemas.length; i++) {
    bindWireValue(schemas[i], args[i], allocator, registry, channelIds, schemaRegistry);
  }
  return channelIds;
}

export function finalizeBoundChannelsForTypeRefs(
  schemas: WireTypeRef[],
  args: unknown[],
  schemaRegistry: WireSchemaRegistry,
): void {
  for (let i = 0; i < schemas.length; i++) {
    finalizeWireValue(schemas[i], args[i], schemaRegistry);
  }
}

/**
 * Bind a single value according to its schema.
 */
function bindValue(
  schema: Schema,
  value: unknown,
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  channelIds: bigint[],
  schemaRegistry?: SchemaRegistry,
): void {
  const resolved =
    schema.kind === "ref" && schemaRegistry ? resolveSchema(schema, schemaRegistry) : schema;

  switch (resolved.kind) {
    // Primitives - nothing to bind
    case "bool":
    case "u8":
    case "u16":
    case "u32":
    case "u64":
    case "i8":
    case "i16":
    case "i32":
    case "i64":
    case "f32":
    case "f64":
    case "string":
    case "bytes":
      return;

    case "tx": {
      // Schema Tx in args means: server sends, client receives
      // So from client's perspective, this is INCOMING data
      // We need to bind the paired Rx (which client reads from)
      const tx = value as Tx<unknown>;
      const channelId = allocator.next();
      const elementSchema = resolved.element;
      const initialCredit = DEFAULT_INITIAL_CREDIT;

      // Just set the channel ID on Tx (for wire encoding)
      // Don't register as outgoing - client doesn't send on this channel
      if (tx.isBound) {
        tx.rebindChannelIdOnly(channelId);
      } else {
        tx.setChannelIdOnly(channelId);
      }

      // Bind the paired Rx for receiving (this is what client reads from)
      if (tx._pair) {
        if (tx._pair.isBound) {
          tx._pair.rebind(
            channelId,
            registry,
            (b: Uint8Array) => decodeWithSchema(b, 0, elementSchema, schemaRegistry).value,
            initialCredit,
          );
        } else {
          tx._pair.bind(
            channelId,
            registry,
            (b: Uint8Array) => decodeWithSchema(b, 0, elementSchema, schemaRegistry).value,
            initialCredit,
          );
        }
      }
      // Collect channel ID for Request.channels field
      channelIds.push(tx.channelId);
      return;
    }

    case "rx": {
      // Schema Rx in args means: server receives, client sends
      // So from client's perspective, this is OUTGOING data
      const rx = value as Rx<unknown>;
      const channelId = allocator.next();
      const elementSchema = resolved.element;
      const initialCredit = DEFAULT_INITIAL_CREDIT;
      if (rx.isBound) {
        rx.rebind(
          channelId,
          registry,
          (b: Uint8Array) => decodeWithSchema(b, 0, elementSchema, schemaRegistry).value,
          initialCredit,
        );
      } else {
        rx.bind(
          channelId,
          registry,
          (b: Uint8Array) => decodeWithSchema(b, 0, elementSchema, schemaRegistry).value,
          initialCredit,
        );
      }

      // Bind the paired Tx for sending (this is what client writes to)
      if (rx._pair) {
        if (rx._pair.isBound) {
          rx._pair.rebind(
            channelId,
            registry,
            (v: unknown) => encodeWithSchema(v, elementSchema, schemaRegistry),
            initialCredit,
          );
        } else {
          rx._pair.bind(
            channelId,
            registry,
            (v: unknown) => encodeWithSchema(v, elementSchema, schemaRegistry),
            initialCredit,
          );
        }
      }
      // Collect channel ID for Request.channels field
      channelIds.push(rx.channelId);
      return;
    }

    case "vec": {
      // FIXME: perf: we know the full schema, therefore, we know
      // whether there's even a _possibility_ of there being a Tx/Rx
      // nested in the map. we should check that before iterating the entire vec...
      const arr = value as unknown[];
      for (const item of arr) {
        bindValue(resolved.element, item, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "option": {
      // FIXME: perf: we know the full schema, therefore, we know
      // whether there's even a _possibility_ of there being a Tx/Rx
      // nested in the option
      if (value !== null && value !== undefined) {
        bindValue(resolved.inner, value, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "map": {
      const map = value as Map<unknown, unknown>;
      // FIXME: perf: we know the full schema, therefore, we know
      // whether there's even a _possibility_ of there being a Tx/Rx
      // nested in the map. we should check that before iterating the entire map...
      for (const [k, v] of map) {
        bindValue(resolved.key, k, allocator, registry, channelIds, schemaRegistry);
        bindValue(resolved.value, v, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "struct": {
      // FIXME: perf: we know the full schema, therefore, we know
      // whether there's even a _possibility_ of there being a Tx/Rx
      // nested in any of the fields here
      const obj = value as Record<string, unknown>;
      for (const [fieldName, fieldSchema] of Object.entries(resolved.fields)) {
        if (fieldName in obj) {
          bindValue(fieldSchema, obj[fieldName], allocator, registry, channelIds, schemaRegistry);
        }
      }
      return;
    }

    case "enum": {
      // FIXME: perf: we know the full schema, therefore, we know
      // whether there's even a _possibility_ of there being a Tx/Rx
      // nested in any of the fields here

      // Enum value should be { tag: string, ... } (tagged union)
      const enumVal = value as { tag: string; [key: string]: unknown };
      const variant = findVariantByName(resolved, enumVal.tag);
      if (!variant) {
        return; // Unknown variant, nothing to bind
      }

      if (isNewtypeVariant(variant)) {
        // Newtype variant: value is in a field named after the variant (lowercase)
        // e.g., { tag: "Hello", hello: { ... } }
        const fieldValue = enumVal[variant.name.toLowerCase()] ?? enumVal.value;
        if (fieldValue !== undefined) {
          const fieldSchemas = getVariantFieldSchemas(variant);
          if (fieldSchemas.length === 1) {
            bindValue(fieldSchemas[0], fieldValue, allocator, registry, channelIds, schemaRegistry);
          }
        }
      } else {
        // Struct variant or tuple variant
        const fieldSchemas = getVariantFieldSchemas(variant);
        const fieldNames = getVariantFieldNames(variant);

        if (fieldNames) {
          // Struct variant: fields are named
          for (let i = 0; i < fieldSchemas.length; i++) {
            const fieldValue = enumVal[fieldNames[i]];
            if (fieldValue !== undefined) {
              bindValue(
                fieldSchemas[i],
                fieldValue,
                allocator,
                registry,
                channelIds,
                schemaRegistry,
              );
            }
          }
        } else if (fieldSchemas.length > 0) {
          // Tuple variant: fields in enumVal.values array
          const tupleValues = enumVal.values as unknown[] | undefined;
          if (tupleValues) {
            for (let i = 0; i < fieldSchemas.length; i++) {
              bindValue(
                fieldSchemas[i],
                tupleValues[i],
                allocator,
                registry,
                channelIds,
                schemaRegistry,
              );
            }
          }
        }
      }
      return;
    }

    case "tuple": {
      // Tuple: array of values matching schema.elements
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        bindValue(
          resolved.elements[i],
          arr[i],
          allocator,
          registry,
          channelIds,
          schemaRegistry,
        );
      }
      return;
    }

    case "ref": {
      // If no registry is available, we can't inspect the referenced schema.
      return;
    }

    default: {
      // Exhaustiveness check
      const _exhaustive: never = resolved;
      throw new Error(`Unknown schema kind: ${(resolved as Schema).kind}`);
    }
  }
}

function bindWireValue(
  schemaRef: WireTypeRef,
  value: unknown,
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  channelIds: bigint[],
  schemaRegistry: WireSchemaRegistry,
): void {
  const resolved = resolveWireTypeRef(schemaRef, schemaRegistry);
  if (!resolved) {
    return;
  }

  switch (resolved.tag) {
    case "primitive":
      return;

    case "channel": {
      if (resolved.direction === "tx") {
        const tx = value as Tx<unknown>;
        const channelId = allocator.next();
        const initialCredit = DEFAULT_INITIAL_CREDIT;

        if (tx.isBound) {
          tx.rebindChannelIdOnly(channelId);
        } else {
          tx.setChannelIdOnly(channelId);
        }

        if (tx._pair) {
          if (tx._pair.isBound) {
            tx._pair.rebind(
              channelId,
              registry,
              (bytes: Uint8Array) => decodeWithTypeRef(bytes, 0, resolved.element, schemaRegistry).value,
              initialCredit,
            );
          } else {
            tx._pair.bind(
              channelId,
              registry,
              (bytes: Uint8Array) => decodeWithTypeRef(bytes, 0, resolved.element, schemaRegistry).value,
              initialCredit,
            );
          }
        }
        channelIds.push(tx.channelId);
        return;
      }

      const rx = value as Rx<unknown>;
      const channelId = allocator.next();
      const initialCredit = DEFAULT_INITIAL_CREDIT;

      if (rx.isBound) {
        rx.rebind(
          channelId,
          registry,
          (bytes: Uint8Array) => decodeWithTypeRef(bytes, 0, resolved.element, schemaRegistry).value,
          initialCredit,
        );
      } else {
        rx.bind(
          channelId,
          registry,
          (bytes: Uint8Array) => decodeWithTypeRef(bytes, 0, resolved.element, schemaRegistry).value,
          initialCredit,
        );
      }

      if (rx._pair) {
        if (rx._pair.isBound) {
          rx._pair.rebind(
            channelId,
            registry,
            (item: unknown) => encodeWithTypeRef(item, resolved.element, schemaRegistry),
            initialCredit,
          );
        } else {
          rx._pair.bind(
            channelId,
            registry,
            (item: unknown) => encodeWithTypeRef(item, resolved.element, schemaRegistry),
            initialCredit,
          );
        }
      }
      channelIds.push(rx.channelId);
      return;
    }

    case "list":
    case "array": {
      const arr = value as Iterable<unknown>;
      for (const item of arr) {
        bindWireValue(resolved.element, item, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "option":
      if (value !== null && value !== undefined) {
        bindWireValue(resolved.element, value, allocator, registry, channelIds, schemaRegistry);
      }
      return;

    case "map": {
      const map = value as Map<unknown, unknown>;
      for (const [key, item] of map) {
        bindWireValue(resolved.key, key, allocator, registry, channelIds, schemaRegistry);
        bindWireValue(resolved.value, item, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const field of resolved.fields) {
        if (field.name in obj) {
          bindWireValue(field.type_ref, obj[field.name], allocator, registry, channelIds, schemaRegistry);
        }
      }
      return;
    }

    case "enum": {
      const enumVal = value as { tag: string; [key: string]: unknown };
      const variant = resolved.variants.find((candidate) => candidate.name === enumVal.tag);
      if (!variant) {
        return;
      }
      bindWireVariantPayload(variant.payload, enumVal, allocator, registry, channelIds, schemaRegistry);
      return;
    }

    case "tuple": {
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        bindWireValue(resolved.elements[i], arr[i], allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
  }
}

function bindWireVariantPayload(
  payload: WireVariantPayload,
  enumVal: { tag: string; [key: string]: unknown },
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  channelIds: bigint[],
  schemaRegistry: WireSchemaRegistry,
): void {
  switch (payload.tag) {
    case "unit":
      return;
    case "newtype": {
      const fieldValue = enumVal[enumVal.tag.toLowerCase()] ?? enumVal.value;
      if (fieldValue !== undefined) {
        bindWireValue(payload.type_ref, fieldValue, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
    case "tuple": {
      const tupleValues = enumVal.values as unknown[] | undefined;
      if (!tupleValues) {
        return;
      }
      for (let i = 0; i < payload.types.length; i++) {
        bindWireValue(payload.types[i], tupleValues[i], allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
    case "struct":
      for (const field of payload.fields) {
        const fieldValue = enumVal[field.name];
        if (fieldValue !== undefined) {
          bindWireValue(field.type_ref, fieldValue, allocator, registry, channelIds, schemaRegistry);
        }
      }
      return;
  }
}

function finalizeValue(
  schema: Schema,
  value: unknown,
  schemaRegistry?: SchemaRegistry,
): void {
  const resolved =
    schema.kind === "ref" && schemaRegistry ? resolveSchema(schema, schemaRegistry) : schema;

  switch (resolved.kind) {
    case "tx": {
      const tx = value as Tx<unknown>;
      tx.finishRetryBinding();
      tx._pair?.finishRetryBinding();
      return;
    }
    case "rx": {
      const rx = value as Rx<unknown>;
      rx.finishRetryBinding();
      rx._pair?.finishRetryBinding();
      return;
    }
    case "vec": {
      const arr = value as unknown[];
      for (const item of arr) {
        finalizeValue(resolved.element, item, schemaRegistry);
      }
      return;
    }
    case "option": {
      if (value !== null && value !== undefined) {
        finalizeValue(resolved.inner, value, schemaRegistry);
      }
      return;
    }
    case "map": {
      const map = value as Map<unknown, unknown>;
      for (const [k, v] of map) {
        finalizeValue(resolved.key, k, schemaRegistry);
        finalizeValue(resolved.value, v, schemaRegistry);
      }
      return;
    }
    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const [fieldName, fieldSchema] of Object.entries(resolved.fields)) {
        if (fieldName in obj) {
          finalizeValue(fieldSchema, obj[fieldName], schemaRegistry);
        }
      }
      return;
    }
    case "enum": {
      const enumVal = value as { tag: string; [key: string]: unknown };
      const variant = findVariantByName(resolved, enumVal.tag);
      if (!variant) {
        return;
      }

      if (isNewtypeVariant(variant)) {
        const fieldValue = enumVal[variant.name.toLowerCase()] ?? enumVal.value;
        if (fieldValue !== undefined) {
          const fieldSchemas = getVariantFieldSchemas(variant);
          if (fieldSchemas.length === 1) {
            finalizeValue(fieldSchemas[0], fieldValue, schemaRegistry);
          }
        }
      } else {
        const fieldSchemas = getVariantFieldSchemas(variant);
        const fieldNames = getVariantFieldNames(variant);
        if (fieldNames) {
          for (let i = 0; i < fieldSchemas.length; i++) {
            const fieldValue = enumVal[fieldNames[i]];
            if (fieldValue !== undefined) {
              finalizeValue(fieldSchemas[i], fieldValue, schemaRegistry);
            }
          }
        } else if (fieldSchemas.length > 0) {
          const tupleValues = enumVal.values as unknown[] | undefined;
          if (tupleValues) {
            for (let i = 0; i < fieldSchemas.length; i++) {
              finalizeValue(fieldSchemas[i], tupleValues[i], schemaRegistry);
            }
          }
        }
      }
      return;
    }
    case "tuple": {
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        finalizeValue(resolved.elements[i], arr[i], schemaRegistry);
      }
      return;
    }
    default:
      return;
  }
}

function finalizeWireValue(
  schemaRef: WireTypeRef,
  value: unknown,
  schemaRegistry: WireSchemaRegistry,
): void {
  const resolved = resolveWireTypeRef(schemaRef, schemaRegistry);
  if (!resolved) {
    return;
  }

  switch (resolved.tag) {
    case "channel": {
      const channel = value as Tx<unknown> | Rx<unknown>;
      channel.finishRetryBinding();
      channel._pair?.finishRetryBinding();
      return;
    }

    case "list":
    case "array": {
      const arr = value as Iterable<unknown>;
      for (const item of arr) {
        finalizeWireValue(resolved.element, item, schemaRegistry);
      }
      return;
    }

    case "option":
      if (value !== null && value !== undefined) {
        finalizeWireValue(resolved.element, value, schemaRegistry);
      }
      return;

    case "map": {
      const map = value as Map<unknown, unknown>;
      for (const [key, item] of map) {
        finalizeWireValue(resolved.key, key, schemaRegistry);
        finalizeWireValue(resolved.value, item, schemaRegistry);
      }
      return;
    }

    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const field of resolved.fields) {
        if (field.name in obj) {
          finalizeWireValue(field.type_ref, obj[field.name], schemaRegistry);
        }
      }
      return;
    }

    case "enum": {
      const enumVal = value as { tag: string; [key: string]: unknown };
      const variant = resolved.variants.find((candidate) => candidate.name === enumVal.tag);
      if (!variant) {
        return;
      }
      finalizeWireVariantPayload(variant.payload, enumVal, schemaRegistry);
      return;
    }

    case "tuple": {
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        finalizeWireValue(resolved.elements[i], arr[i], schemaRegistry);
      }
      return;
    }

    case "primitive":
      return;
  }
}

function finalizeWireVariantPayload(
  payload: WireVariantPayload,
  enumVal: { tag: string; [key: string]: unknown },
  schemaRegistry: WireSchemaRegistry,
): void {
  switch (payload.tag) {
    case "unit":
      return;
    case "newtype": {
      const fieldValue = enumVal[enumVal.tag.toLowerCase()] ?? enumVal.value;
      if (fieldValue !== undefined) {
        finalizeWireValue(payload.type_ref, fieldValue, schemaRegistry);
      }
      return;
    }
    case "tuple": {
      const tupleValues = enumVal.values as unknown[] | undefined;
      if (!tupleValues) {
        return;
      }
      for (let i = 0; i < payload.types.length; i++) {
        finalizeWireValue(payload.types[i], tupleValues[i], schemaRegistry);
      }
      return;
    }
    case "struct":
      for (const field of payload.fields) {
        const fieldValue = enumVal[field.name];
        if (fieldValue !== undefined) {
          finalizeWireValue(field.type_ref, fieldValue, schemaRegistry);
        }
      }
      return;
  }
}
