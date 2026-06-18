// Client-side metadata: a self-describing `Value` map (`r[rpc.metadata]`).
//
// On the wire metadata is a phon `Value` map. Key sigils (`#`, `-`, `-#`) are
// conventions on the key string, not separate flag-list entries.
// r[impl rpc.metadata]
// r[impl rpc.metadata.value]
// r[impl rpc.metadata.keys]
// r[impl rpc.metadata.duplicates]
// r[impl rpc.metadata.unknown]
// r[impl schema.interaction.metadata]

import type { Value } from "@bearcove/phon-schema";
import { type Metadata } from "@bearcove/vox-wire";

/** A metadata value: string, u64 (bigint), or raw bytes. */
export type ClientMetadataValue = string | bigint | Uint8Array;

/**
 * Client-side metadata builder.
 *
 * Use `set()` with the key string that should appear on the wire. A leading `#`
 * marks values sensitive for logging, and `-#` marks sensitive/no-propagate.
 */
export class ClientMetadata {
  private readonly map: Metadata = new Map();

  set(key: string, value: ClientMetadataValue): this {
    this.map.set(key, value as Value);
    return this;
  }

  get(key: string): Value | undefined {
    return this.map.get(key);
  }

  has(key: string): boolean {
    return this.map.has(key);
  }

  delete(key: string): boolean {
    return this.map.delete(key);
  }

  get size(): number {
    return this.map.size;
  }

  keys(): IterableIterator<string> {
    return this.map.keys();
  }

  entries(): IterableIterator<[string, Value]> {
    return this.map.entries();
  }

  /** The wire `Value` map. */
  toWire(): Metadata {
    return this.map;
  }

  clone(): ClientMetadata {
    const copy = new ClientMetadata();
    for (const [k, v] of this.map) copy.map.set(k, v);
    return copy;
  }

  static fromWire(metadata: Metadata): ClientMetadata {
    const m = new ClientMetadata();
    for (const [k, v] of metadata) m.map.set(k, v);
    return m;
  }
}

/** Convert a `ClientMetadata` to the wire `Value` map. */
export function clientMetadataToWire(metadata: ClientMetadata): Metadata {
  return metadata.toWire();
}
