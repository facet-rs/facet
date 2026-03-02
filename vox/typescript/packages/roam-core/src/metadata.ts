// Metadata conversion utilities.
//
// Provides ClientMetadata class for building metadata with flags,
// and conversion functions to/from wire format.

import type { MetadataEntry, MetadataValue } from "@bearcove/roam-wire";
import { metadataString, metadataBytes, metadataU64, MetadataFlags } from "@bearcove/roam-wire";

/**
 * Metadata value type for client middleware.
 * Values can be strings, u64 bigints, or raw bytes.
 */
export type ClientMetadataValue = string | bigint | Uint8Array;

/**
 * Internal storage for a metadata entry with its flags.
 */
interface MetadataEntryInternal {
  value: ClientMetadataValue;
  flags: bigint;
}



/**
 * Client-side metadata storage with flag support.
 *
 * Use `set()` for normal metadata and `setSensitive()` for metadata that
 * should be redacted in logs and traces.
 *
 * @example
 * ```typescript
 * const meta = new ClientMetadata();
 * meta.set("trace-id", "abc123");
 * meta.setSensitive("authorization", "Bearer secret-token");
 * ```
 */
export class ClientMetadata {
  private entries = new Map<string, MetadataEntryInternal>();

  /**
   * Set a metadata entry with default flags (none).
   */
  set(key: string, value: ClientMetadataValue): this {
    this.entries.set(key, { value, flags: MetadataFlags.NONE });
    return this;
  }

  /**
   * Set a sensitive metadata entry (will be redacted in logs).
   * r[impl call.metadata.flags] - SENSITIVE flag marks values for redaction
   */
  setSensitive(key: string, value: ClientMetadataValue): this {
    this.entries.set(key, { value, flags: MetadataFlags.SENSITIVE });
    return this;
  }

  /**
   * Set a metadata entry with custom flags.
   */
  setWithFlags(key: string, value: ClientMetadataValue, flags: bigint): this {
    this.entries.set(key, { value, flags });
    return this;
  }

  /**
   * Get a metadata value by key.
   */
  get(key: string): ClientMetadataValue | undefined {
    return this.entries.get(key)?.value;
  }

  /**
   * Get the flags for a key.
   */
  getFlags(key: string): bigint {
    return this.entries.get(key)?.flags ?? MetadataFlags.NONE;
  }

  /**
   * Check if a key is marked as sensitive.
   */
  isSensitive(key: string): boolean {
    const flags = this.getFlags(key);
    return (flags & MetadataFlags.SENSITIVE) !== 0n;
  }

  /**
   * Check if a key exists.
   */
  has(key: string): boolean {
    return this.entries.has(key);
  }

  /**
   * Delete a metadata entry.
   */
  delete(key: string): boolean {
    return this.entries.delete(key);
  }

  /**
   * Get the number of entries.
   */
  get size(): number {
    return this.entries.size;
  }

  /**
  * Iterate over entries as [key, value, flags] tuples.
   */
  *[Symbol.iterator](): Iterator<[string, ClientMetadataValue, bigint]> {
    for (const [key, entry] of this.entries) {
      yield [key, entry.value, entry.flags];
    }
  }

  /**
   * Iterate over keys.
   */
  keys(): IterableIterator<string> {
    return this.entries.keys();
  }

  /**
   * Convert to wire format entries.
   */
  toWireEntries(): MetadataEntry[] {
    const result: MetadataEntry[] = [];
    for (const [key, entry] of this.entries) {
      let wireValue: MetadataValue;
      if (typeof entry.value === "string") {
        wireValue = metadataString(entry.value);
      } else if (typeof entry.value === "bigint") {
        wireValue = metadataU64(entry.value);
      } else {
        wireValue = metadataBytes(entry.value);
      }
      result.push({ key, value: wireValue, flags: entry.flags });
    }
    return result;
  }

  /**
   * Create from wire format entries.
   */
  static fromWireEntries(entries: MetadataEntry[]): ClientMetadata {
    const meta = new ClientMetadata();
    for (const entry of entries) {
      meta.setWithFlags(entry.key, entry.value.value, entry.flags);
    }
    return meta;
  }

  /**
   * Create a copy of this metadata.
   */
  clone(): ClientMetadata {
    const copy = new ClientMetadata();
    for (const [key, entry] of this.entries) {
      copy.entries.set(key, { ...entry });
    }
    return copy;
  }
}

/**
 * Convert a ClientMetadata to wire format entries.
 */
export function clientMetadataToEntries(metadata: ClientMetadata): MetadataEntry[] {
  return metadata.toWireEntries();
}

/**
 * Convert wire format metadata entries to ClientMetadata.
 */
export function metadataEntriesToClientMetadata(entries: MetadataEntry[]): ClientMetadata {
  return ClientMetadata.fromWireEntries(entries);
}
