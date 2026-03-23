import type { Metadata } from "@bearcove/vox-wire";
import { MetadataFlagValues, metadataEntry, metadataU64 } from "@bearcove/vox-wire";
import { ClientMetadata } from "./metadata.ts";

export const RETRY_SUPPORT_METADATA_KEY = "vox-retry-support";
export const OPERATION_ID_METADATA_KEY = "vox-operation-id";
export const RETRY_SUPPORT_VERSION = 1n;

export function appendRetrySupportMetadata(metadata: Metadata): Metadata {
  if (metadataSupportsRetry(metadata)) {
    return [...metadata];
  }
  return [
    ...metadata,
    metadataEntry(
      RETRY_SUPPORT_METADATA_KEY,
      metadataU64(RETRY_SUPPORT_VERSION),
      MetadataFlagValues.NONE,
    ),
  ];
}

export function metadataSupportsRetry(metadata: Metadata): boolean {
  return metadata.some(
    (entry) =>
      entry.key === RETRY_SUPPORT_METADATA_KEY
      && entry.value.tag === "U64"
      && entry.value.value === RETRY_SUPPORT_VERSION,
  );
}

export function metadataOperationId(metadata: Metadata): bigint | undefined {
  const entry = metadata.find((candidate) => candidate.key === OPERATION_ID_METADATA_KEY);
  if (!entry || entry.value.tag !== "U64") {
    return undefined;
  }
  return entry.value.value;
}

export function ensureOperationId(metadata: ClientMetadata, operationId: bigint): void {
  if (metadata.has(OPERATION_ID_METADATA_KEY)) {
    return;
  }
  metadata.setWithFlags(OPERATION_ID_METADATA_KEY, operationId, MetadataFlagValues.NONE);
}
