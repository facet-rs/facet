// Runtime method and service descriptor types.
//
// These are the TypeScript equivalents of the Rust ServiceDescriptor,
// carrying method metadata plus the canonical schema table used at runtime.

import type { PhonMethodSchemas } from "../schema_tracker.ts";

// TODO(phon-channels): the per-method channel runtime is being rewritten on the
// phon engine. The canonical per-service schema table is now a phon-shaped map
// of `PhonMethodSchemas` keyed by wire method id; each entry carries the args
// root, the args schema-closure hex, the ok root, and the channel bindings
// (`PhonMethodSchemas.channels`). This is a minimal correct typing for the
// descriptor until that runtime lands.
export type ServiceSendSchemas = Record<string, PhonMethodSchemas>;

/**
 * Describes a single RPC method at runtime.
 *
 * Carries only method metadata. Canonical args/response schemas live in the
 * service-level `send_schemas` table keyed by `id`.
 */
export interface MethodDescriptor {
  /** Method name (for logging/debugging). */
  name: string;
  /** Method ID hash for wire protocol routing. */
  id: bigint;
}

/** Describes a service at runtime (collection of method descriptors). */
export interface ServiceDescriptor {
  service_name: string;
  /** Canonical per-service schema table generated from Rust shapes. */
  send_schemas: ServiceSendSchemas;
  /** The service's phon `Registry`, resolving every args/response/channel type. */
  registry: import("@bearcove/phon-schema").Registry;
  /** Method metadata keyed by wire method ID. */
  methods: Map<bigint, MethodDescriptor>;
}

/**
 * Interface for replying to an RPC call from within a dispatcher.
 *
 * The runtime creates a VoxCall for each incoming request. Generated
 * dispatchers call exactly one of these methods to send the response.
 * Encoding is handled by the runtime using the canonical response schema.
 */
export interface VoxCall {
  /** Reply with a successful value. */
  reply(value: unknown): void;
  /** Reply with a user-defined error. */
  replyErr(error: unknown): void;
  /** Reply with an internal/protocol error (InvalidPayload). */
  replyInternalError(message?: string): void;
}
