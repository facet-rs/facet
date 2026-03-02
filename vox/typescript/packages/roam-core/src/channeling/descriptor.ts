// Runtime method and service descriptor types.
//
// These are the TypeScript equivalents of the Rust ServiceDescriptor,
// used by the runtime to drive schema-based encode/decode for channels.

import type { TupleSchema, EnumSchema } from "./schema.ts";

/**
 * Describes a single RPC method at runtime.
 *
 * Used by the runtime to decode args and encode responses without
 * any serialization logic in generated code.
 */
export interface MethodDescriptor {
  /** Method name (for logging/debugging). */
  name: string;
  /** Method ID hash for wire protocol routing. */
  id: bigint;
  /** Tuple schema for all arguments (decoded once before dispatch). */
  args: TupleSchema;
  /**
   * Schema for the full Result<T, RoamError<E>> wire type.
   *
   * Always an enum with two variants:
   * - Ok (index 0): the success value T
   * - Err (index 1): RoamError<E>, itself an enum:
   *   - User (index 0): user-defined error E (null fields if infallible)
   *   - UnknownMethod (index 1): no fields
   *   - InvalidPayload (index 2): no fields
   *   - Cancelled (index 3): no fields
   */
  result: EnumSchema;
}

/** Describes a service at runtime (collection of method descriptors). */
export interface ServiceDescriptor {
  service_name: string;
  methods: MethodDescriptor[];
}

/**
 * Interface for replying to an RPC call from within a dispatcher.
 *
 * The runtime creates a RoamCall for each incoming request. Generated
 * dispatchers call exactly one of these methods to send the response.
 * Encoding is handled by the runtime using the method's result schema.
 */
export interface RoamCall {
  /** Reply with a successful value. Encoded using method.result Ok variant. */
  reply(value: unknown): void;
  /** Reply with a user-defined error. Encoded using method.result Err/User variant. */
  replyErr(error: unknown): void;
  /** Reply with an internal/protocol error (InvalidPayload). */
  replyInternalError(): void;
}
