import type { Metadata } from "@bearcove/vox-wire";

export const SESSION_RESUME_KEY_METADATA_KEY = "vox-session-key";

export function appendSessionResumeKeyMetadata(metadata: Metadata, key: Uint8Array): Metadata {
  return [
    ...metadata,
    {
      key: SESSION_RESUME_KEY_METADATA_KEY,
      value: { tag: "Bytes", value: key.slice() },
      flags: 0n,
    },
  ];
}

export function metadataSessionResumeKey(
  metadata: Metadata,
): Uint8Array | null {
  for (const entry of metadata) {
    if (entry.key !== SESSION_RESUME_KEY_METADATA_KEY) {
      continue;
    }
    if (entry.value.tag === "Bytes" && entry.value.value.length === 16) {
      return entry.value.value.slice();
    }
  }
  return null;
}
