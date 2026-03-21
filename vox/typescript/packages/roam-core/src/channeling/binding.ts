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
  resolveWireTypeRef,
  type WireSchemaRegistry,
  type WireTypeRef,
  type WireSchemaKind,
  type WireVariantPayload,
} from "@bearcove/roam-postcard";

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
