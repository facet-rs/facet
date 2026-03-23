// Runtime method and service descriptor types.
//
// These are the TypeScript equivalents of the Rust ServiceDescriptor,
// carrying method metadata plus the canonical schema table used at runtime.

import type { ServiceSendSchemas } from "../schema_tracker.ts";

export interface RetryPolicy {
  /** Whether an admitted operation must persist once started. */
  persist: boolean;
  /** Whether re-executing the same logical operation is semantically safe. */
  idem: boolean;
}

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
  /** Static retry policy declared for this method. */
  retry: RetryPolicy;
}

/** Describes a service at runtime (collection of method descriptors). */
export interface ServiceDescriptor {
  service_name: string;
  /** Canonical per-service schema table generated from Rust shapes. */
  send_schemas: ServiceSendSchemas;
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
