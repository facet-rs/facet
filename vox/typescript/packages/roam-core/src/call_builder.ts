// CallBuilder for fluent RPC call construction.
//
// Allows adding metadata before making the call:
//   await client.echo("hello").withMeta("trace-id", "abc123");

import type { ClientMetadataValue } from "./middleware.ts";

/**
 * Executor function type for CallBuilder.
 * Takes metadata and returns the call result.
 */
export type CallExecutor<T> = (metadata: Map<string, ClientMetadataValue>) => Promise<T>;

/**
 * Fluent builder for RPC calls.
 *
 * Implements PromiseLike so it can be awaited directly, while also
 * supporting metadata modification before the call is made.
 *
 * IMPORTANT: The call is executed eagerly when the CallBuilder is created.
 * This is necessary for streaming methods where channels must be active
 * before data can be sent. The builder pattern still works because metadata
 * is captured at construction time.
 *
 * @example
 * ```typescript
 * // Simple call (awaits immediately)
 * const result = await client.echo("hello");
 *
 * // With metadata - note: metadata must be added BEFORE creating the builder
 * // For per-call metadata, use the Caller interface directly:
 * const caller = connection.asCaller().with(authMiddleware);
 * const client = new TestbedClient(caller);
 * await client.echo("hello"); // middleware adds auth
 *
 * // For streaming, the call starts immediately:
 * const call = client.generate(5, tx); // RPC sent now
 * tx.send(data);                        // Can send data
 * await call;                           // Wait for completion
 * ```
 */
export class CallBuilder<T> implements PromiseLike<T> {
  private resultPromise: Promise<T>;

  constructor(executor: CallExecutor<T>, metadata?: Map<string, ClientMetadataValue>) {
    // Execute immediately with the provided metadata (or empty map)
    this.resultPromise = executor(metadata ?? new Map());
  }

  /**
   * Implement PromiseLike for await support.
   */
  // oxlint-disable-next-line unicorn/no-thenable -- intentional: CallBuilder implements PromiseLike
  then<TResult1 = T, TResult2 = never>(
    onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
    onrejected?: ((reason: unknown) => TResult2 | PromiseLike<TResult2>) | null,
  ): Promise<TResult1 | TResult2> {
    return this.resultPromise.then(onfulfilled, onrejected);
  }

  /**
   * Support for catch.
   */
  catch<TResult = never>(
    onrejected?: ((reason: unknown) => TResult | PromiseLike<TResult>) | null,
  ): Promise<T | TResult> {
    return this.resultPromise.catch(onrejected);
  }

  /**
   * Support for finally.
   */
  finally(onfinally?: (() => void) | null): Promise<T> {
    return this.resultPromise.finally(onfinally);
  }
}

/**
 * Create a CallBuilder with metadata.
 *
 * This is the primary way to add per-call metadata:
 * ```typescript
 * await withMeta(
 *   new Map([["auth", "Bearer token"]]),
 *   (meta) => client.echo("hello", meta)
 * );
 * ```
 *
 * For most use cases, prefer using middleware instead:
 * ```typescript
 * const authedCaller = connection.asCaller().with(authMiddleware);
 * const client = new TestbedClient(authedCaller);
 * await client.echo("hello"); // middleware adds auth
 * ```
 */
export function withMeta<T>(
  metadata: Map<string, ClientMetadataValue>,
  executor: CallExecutor<T>,
): CallBuilder<T> {
  return new CallBuilder(executor, metadata);
}
