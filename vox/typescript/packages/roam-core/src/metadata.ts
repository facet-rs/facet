// Metadata conversion utilities.
//
// Converts between the user-friendly Map<string, ClientMetadataValue>
// and the wire format MetadataEntry[].

import type { MetadataEntry, MetadataValue } from "@bearcove/roam-wire";
import { metadataString, metadataBytes, metadataU64 } from "@bearcove/roam-wire";
import type { ClientMetadataValue } from "./middleware.ts";

/**
 * Convert a client metadata Map to wire format entries.
 *
 * @param map - Client metadata map
 * @returns Wire format metadata entries
 */
export function metadataMapToEntries(
  map: Map<string, ClientMetadataValue>
): MetadataEntry[] {
  const entries: MetadataEntry[] = [];
  for (const [key, value] of map) {
    let wireValue: MetadataValue;
    if (typeof value === "string") {
      wireValue = metadataString(value);
    } else if (typeof value === "bigint") {
      wireValue = metadataU64(value);
    } else {
      wireValue = metadataBytes(value);
    }
    entries.push([key, wireValue]);
  }
  return entries;
}

/**
 * Convert wire format metadata entries to a client-friendly Map.
 *
 * @param entries - Wire format metadata entries
 * @returns Client metadata map
 */
export function metadataEntriesToMap(
  entries: MetadataEntry[]
): Map<string, ClientMetadataValue> {
  const map = new Map<string, ClientMetadataValue>();
  for (const [key, value] of entries) {
    map.set(key, value.value);
  }
  return map;
}
