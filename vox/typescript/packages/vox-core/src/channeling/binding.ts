// Runtime channel binder.
//
// Walks canonical Rust-shaped schema refs to find and bind Tx/Rx channels.

import type { ChannelIdAllocator } from "./allocator.ts";
import type { ChannelRegistry } from "./registry.ts";
import { Tx } from "./tx.ts";
import { Rx } from "./rx.ts";
import { DEFAULT_INITIAL_CREDIT } from "./types.ts";
import {
  encodeWithTypeRef,
  decodeWithTypeRef,
  resolveTypeRef,
  type SchemaRegistry,
  type TypeRef,
  type SchemaKind,
  type VariantPayload,
} from "@bearcove/vox-postcard";

/**
 * Bind all Tx/Rx channels found in canonical args type refs.
 *
 * Generated vox clients use this path. `schemas` must be the tuple element
 * refs for the method args shape.
 */
export function bindChannelsForTypeRefs(
  schemas: TypeRef[],
  args: unknown[],
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  schemaRegistry: SchemaRegistry,
): bigint[] {
  const channelIds: bigint[] = [];
  for (let i = 0; i < schemas.length; i++) {
    bindChannelsInValue(schemas[i], args[i], allocator, registry, channelIds, schemaRegistry);
  }
  return channelIds;
}

export function finalizeBoundChannelsForTypeRefs(
  schemas: TypeRef[],
  args: unknown[],
  schemaRegistry: SchemaRegistry,
): void {
  for (let i = 0; i < schemas.length; i++) {
    finalizeChannelsInValue(schemas[i], args[i], schemaRegistry);
  }
}

function bindChannelsInValue(
  schemaRef: TypeRef,
  value: unknown,
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  channelIds: bigint[],
  schemaRegistry: SchemaRegistry,
): void {
  const resolved = resolveTypeRef(schemaRef, schemaRegistry);
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
        bindChannelsInValue(resolved.element, item, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "option":
      if (value !== null && value !== undefined) {
        bindChannelsInValue(resolved.element, value, allocator, registry, channelIds, schemaRegistry);
      }
      return;

    case "map": {
      const map = value as Map<unknown, unknown>;
      for (const [key, item] of map) {
        bindChannelsInValue(resolved.key, key, allocator, registry, channelIds, schemaRegistry);
        bindChannelsInValue(resolved.value, item, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }

    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const field of resolved.fields) {
        if (field.name in obj) {
          bindChannelsInValue(field.type_ref, obj[field.name], allocator, registry, channelIds, schemaRegistry);
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
      bindChannelsInVariantPayload(variant.payload, enumVal, allocator, registry, channelIds, schemaRegistry);
      return;
    }

    case "tuple": {
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        bindChannelsInValue(resolved.elements[i], arr[i], allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
  }
}

function bindChannelsInVariantPayload(
  payload: VariantPayload,
  enumVal: { tag: string; [key: string]: unknown },
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
  channelIds: bigint[],
  schemaRegistry: SchemaRegistry,
): void {
  switch (payload.tag) {
    case "unit":
      return;
    case "newtype": {
      const fieldValue = enumVal[enumVal.tag.toLowerCase()] ?? enumVal.value;
      if (fieldValue !== undefined) {
        bindChannelsInValue(payload.type_ref, fieldValue, allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
    case "tuple": {
      const tupleValues = enumVal.values as unknown[] | undefined;
      if (!tupleValues) {
        return;
      }
      for (let i = 0; i < payload.types.length; i++) {
        bindChannelsInValue(payload.types[i], tupleValues[i], allocator, registry, channelIds, schemaRegistry);
      }
      return;
    }
    case "struct":
      for (const field of payload.fields) {
        const fieldValue = enumVal[field.name];
        if (fieldValue !== undefined) {
          bindChannelsInValue(field.type_ref, fieldValue, allocator, registry, channelIds, schemaRegistry);
        }
      }
      return;
  }
}

function finalizeChannelsInValue(
  schemaRef: TypeRef,
  value: unknown,
  schemaRegistry: SchemaRegistry,
): void {
  const resolved = resolveTypeRef(schemaRef, schemaRegistry);
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
        finalizeChannelsInValue(resolved.element, item, schemaRegistry);
      }
      return;
    }

    case "option":
      if (value !== null && value !== undefined) {
        finalizeChannelsInValue(resolved.element, value, schemaRegistry);
      }
      return;

    case "map": {
      const map = value as Map<unknown, unknown>;
      for (const [key, item] of map) {
        finalizeChannelsInValue(resolved.key, key, schemaRegistry);
        finalizeChannelsInValue(resolved.value, item, schemaRegistry);
      }
      return;
    }

    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const field of resolved.fields) {
        if (field.name in obj) {
          finalizeChannelsInValue(field.type_ref, obj[field.name], schemaRegistry);
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
      finalizeChannelsInVariantPayload(variant.payload, enumVal, schemaRegistry);
      return;
    }

    case "tuple": {
      const arr = value as unknown[];
      for (let i = 0; i < resolved.elements.length; i++) {
        finalizeChannelsInValue(resolved.elements[i], arr[i], schemaRegistry);
      }
      return;
    }

    case "primitive":
      return;
  }
}

function finalizeChannelsInVariantPayload(
  payload: VariantPayload,
  enumVal: { tag: string; [key: string]: unknown },
  schemaRegistry: SchemaRegistry,
): void {
  switch (payload.tag) {
    case "unit":
      return;
    case "newtype": {
      const fieldValue = enumVal[enumVal.tag.toLowerCase()] ?? enumVal.value;
      if (fieldValue !== undefined) {
        finalizeChannelsInValue(payload.type_ref, fieldValue, schemaRegistry);
      }
      return;
    }
    case "tuple": {
      const tupleValues = enumVal.values as unknown[] | undefined;
      if (!tupleValues) {
        return;
      }
      for (let i = 0; i < payload.types.length; i++) {
        finalizeChannelsInValue(payload.types[i], tupleValues[i], schemaRegistry);
      }
      return;
    }
    case "struct":
      for (const field of payload.fields) {
        const fieldValue = enumVal[field.name];
        if (fieldValue !== undefined) {
          finalizeChannelsInValue(field.type_ref, fieldValue, schemaRegistry);
        }
      }
      return;
  }
}
