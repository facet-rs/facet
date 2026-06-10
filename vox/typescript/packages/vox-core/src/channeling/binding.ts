// Runtime channel binder — out-of-band index design, phon per-item codec.
//
// `Tx`/`Rx` arguments are opaque on the wire: each encodes only a `u32` index
// into `RequestCall.channels`, and the allocated `ChannelId`s travel out-of-band
// in that list (`r[rpc.channel.payload-encoding]`, `r[rpc.channel.allocation]`).
// r[impl schema.interaction.channels]
//
// For each channel argument we allocate a `ChannelId`, bind the *local-facing*
// handle (the pair of the one passed into the call) with a phon per-item codec
// keyed on the element root, and replace the argument with its wire-index bytes.

import { encodeTyped, decodeTyped } from "@bearcove/phon-engine";
import type { Registry } from "@bearcove/phon-schema";

import type { ChannelIdAllocator } from "./allocator.ts";
import type { ChannelRegistry } from "./registry.ts";
import type { Tx } from "./tx.ts";
import type { Rx } from "./rx.ts";
import type { BindingDirection, PhonChannelMeta, SchemaTracker } from "../schema_tracker.ts";

/** The 4-byte little-endian phon-compact encoding of a `u32` wire index. */
// r[impl rpc.channel.payload-encoding]
function wireIndexBytes(index: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, index, true);
  return out;
}

/** A phon serializer for a channel element type identified by `elementRoot`. */
function makeSerialize(elementRoot: bigint, registry: Registry): (value: unknown) => Uint8Array {
  return (value) => encodeTyped(value as never, elementRoot, registry);
}

export interface ChannelSchemaContext {
  methodId: bigint;
  direction: BindingDirection;
  tracker: SchemaTracker;
}

function channelElementRole(meta: PhonChannelMeta): string {
  return `channel.arg.${meta.index}.${meta.direction}.element`;
}

function makeDeserialize(
  elementRoot: bigint,
  registry: Registry,
  schemaContext: ChannelSchemaContext | undefined,
  role: string,
): (bytes: Uint8Array) => unknown {
  return (bytes) => {
    const decoder = schemaContext?.tracker.buildAuxiliaryDecoder(
      schemaContext.methodId,
      schemaContext.direction,
      role,
      elementRoot,
      registry,
    );
    if (decoder) {
      return decoder(bytes) as unknown;
    }
    return decodeTyped(bytes, elementRoot, elementRoot, registry) as unknown;
  };
}

/** Per-direction initial credit windows for a bound channel. */
export interface ChannelCredit {
  /** Credit we may spend sending (the peer's advertised initial grant). */
  outgoing: number;
  /** Credit window we offer the peer when receiving (drives re-grant cadence). */
  incoming: number;
}

export interface BoundChannels {
  /** `RequestCall.channels`, in wire-index (allocation) order. */
  channels: bigint[];
  /** The args values with each `Tx`/`Rx` replaced by its wire-index `Bytes`. */
  values: unknown[];
  /** Finalize call-bound handles after the request settles. */
  finalize: () => void;
}

type CallBindingFinalizable = { finishCallBinding?: () => void };

/**
 * Bind the `Tx`/`Rx` channels in a call's argument list. `channelMetas` comes
 * from the generated `{service}Methods[...].channels` table; each entry's
 * `index` is the argument position and `direction` is the method-signature point
 * of view (so a `tx` arg means the *callee* sends and the caller receives).
 */
export function bindPhonChannels(
  args: unknown[],
  channelMetas: PhonChannelMeta[],
  allocator: ChannelIdAllocator,
  channelRegistry: ChannelRegistry,
  registry: Registry,
  credit: ChannelCredit,
  schemaContext?: ChannelSchemaContext,
): BoundChannels {
  if (channelMetas.length === 0) {
    return { channels: [], values: args, finalize: () => {} };
  }

  const values = [...args];
  const channels: bigint[] = [];
  const bound: Array<Tx<unknown> | Rx<unknown>> = [];

  // Allocate in argument-position order so the wire index is stable.
  // r[impl rpc.channel.discovery]
  const metas = [...channelMetas].sort((a, b) => a.index - b.index);
  for (const meta of metas) {
    const handle = values[meta.index] as Tx<unknown> | Rx<unknown>;
    const channelId = allocator.next();
    const wireIndex = channels.length;
    channels.push(channelId);
    bindOne(handle, meta, channelId, channelRegistry, registry, credit, schemaContext);
    values[meta.index] = wireIndexBytes(wireIndex);
    bound.push(handle);
  }

  const finalize = (): void => {
    for (const handle of bound) {
      const pair = (handle as { _pair?: CallBindingFinalizable })._pair;
      pair?.finishCallBinding?.();
      (handle as CallBindingFinalizable).finishCallBinding?.();
    }
  };

  return { channels, values, finalize };
}

function bindOne(
  handle: Tx<unknown> | Rx<unknown>,
  meta: PhonChannelMeta,
  channelId: bigint,
  channelRegistry: ChannelRegistry,
  registry: Registry,
  credit: ChannelCredit,
  schemaContext: ChannelSchemaContext | undefined,
): void {
  if (meta.direction === "tx") {
    // Method wants a `Tx` (callee sends). The caller passed a `Tx` and keeps the
    // paired `Rx` — the caller receives. Bind that pair for INCOMING.
    const tx = handle as Tx<unknown>;
    // r[impl rpc.channel.binding.caller-args]
    // r[impl rpc.channel.binding.caller-args.tx]
    // r[impl rpc.channel.pair.binding-propagation]
    const rx = (tx as { _pair?: Rx<unknown> })._pair;
    // r[impl schema.exchange.channels.tx-args]
    const deserialize = makeDeserialize(meta.elementRoot, registry, schemaContext, channelElementRole(meta));
    if (rx) {
      if (rx.isBound) rx.rebind(channelId, channelRegistry, deserialize, credit.incoming);
      else rx.bind(channelId, channelRegistry, deserialize, credit.incoming);
    }
    return;
  }

  // Method wants an `Rx` (callee receives). The caller passed an `Rx` and keeps
  // the paired `Tx` — the caller sends. Bind that pair for OUTGOING.
  const rx = handle as Rx<unknown>;
  // r[impl rpc.channel.binding.caller-args]
  // r[impl rpc.channel.binding.caller-args.rx]
  // r[impl rpc.channel.pair.binding-propagation]
  const tx = (rx as { _pair?: Tx<unknown> })._pair;
  const serialize = makeSerialize(meta.elementRoot, registry);
  if (tx) {
    if (tx.isBound) tx.rebind(channelId, channelRegistry, serialize, credit.outgoing);
    else tx.bind(channelId, channelRegistry, serialize, credit.outgoing);
  }
}
