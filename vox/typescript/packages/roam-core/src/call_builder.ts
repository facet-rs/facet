// CallBuilder for fluent RPC call construction.
//
// Allows adding metadata before starting the request attempt for a call:
//   await client.echo("hello").withMeta("trace-id", "abc123");

import { ClientMetadata } from "./metadata.ts";

/**
 * Executor function type for CallBuilder.
 * Takes metadata and returns the eventual call result.
 */
export type CallExecutor<T> = (metadata: ClientMetadata) => Promise<T>;

/**
 * Fluent builder for RPC calls.
 *
 * Implements PromiseLike so it can be awaited directly, while also
 * supporting metadata modification before the request attempt is started.
 *
 * IMPORTANT: the builder starts work eagerly when the CallBuilder is created.
 * For ordinary unary methods this usually just means the request attempt is
 * issued immediately. For streaming methods this is necessary because channels
 * must be active before data can be sent. The builder pattern still works
 * because metadata is captured at construction time.
 *
 * `CallBuilder` is about one caller-visible call. If retry or session recovery
 * later creates another request attempt for the same logical operation, that is
 * handled below this API surface.
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
 * // For streaming, the initial request attempt starts immediately:
 * const call = client.generate(5, tx); // initial request attempt sent now
 * tx.send(data);                        // Can send data
 * await call;                           // Wait for completion
 * ```
 */
export class CallBuilder<T> implements PromiseLike<T> {
  private resultPromise: Promise<T>;

  constructor(executor: CallExecutor<T>, metadata?: ClientMetadata) {
    // Start the initial request attempt immediately with the provided metadata
    // (or empty metadata).
    this.resultPromise = executor(metadata ?? new ClientMetadata());
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
 * This is the primary way to add per-call metadata before the initial request
 * attempt is started:
 * ```typescript
 * const meta = new ClientMetadata();
 * meta.setSensitive("authorization", "Bearer token");
 * await withMeta(meta, (m) => client.echo("hello", m));
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
  metadata: ClientMetadata,
  executor: CallExecutor<T>,
): CallBuilder<T> {
  return new CallBuilder(executor, metadata);
}
