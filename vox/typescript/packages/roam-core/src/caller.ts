// Caller abstraction for TypeScript roam clients.
//
// Provides a clean interface for making RPC calls that supports
// middleware composition via the with() method.

import type { ChannelIdAllocator, ChannelRegistry, MethodDescriptor } from "./channeling/index.ts";
import type {
  ClientMiddleware,
  ClientContext,
  CallRequest,
  CallOutcome,
} from "./middleware.ts";
import { Extensions, RejectionError } from "./middleware.ts";
import { ClientMetadata } from "./metadata.ts";

/**
 * Internal request representation used by Caller implementations.
 *
 * Contains all information needed to encode and send a request,
 * including a deferred encoding function so middleware can modify args.
 */
export interface CallerRequest {
  /**
   * Fully qualified method name (e.g., "Testbed.echo").
   */
  method: string;

  /**
   * Method arguments as a record (used by middleware for inspection).
   */
  args: Record<string, unknown>;

  /**
   * Method descriptor for encoding args (descriptor.args) and decoding
   * the full Result<T, RoamError<E>> response (descriptor.result).
   * The method ID is descriptor.id.
   */
  descriptor: MethodDescriptor;

  /**
   * Channel IDs for streaming arguments.
   */
  channels?: bigint[];

  /**
   * Request timeout in milliseconds.
   */
  timeoutMs?: number;

  /**
   * Metadata to send with the request.
   */
  metadata?: ClientMetadata;
}

/**
 * Caller interface for making RPC calls.
 *
 * This is the primary abstraction used by generated clients.
 * Implementations handle the actual wire protocol, while middleware
 * can be composed using the with() method.
 */
export interface Caller {
  /**
   * Make an RPC call.
   *
   * @param request - The request to send
   * @returns Decoded response value
   */
  call(request: CallerRequest): Promise<unknown>;

  /**
   * Get the channel ID allocator for streaming.
   */
  getChannelAllocator(): ChannelIdAllocator;

  /**
   * Get the channel registry for streaming.
   */
  getChannelRegistry(): ChannelRegistry;

  /**
   * Wrap this caller with middleware.
   *
   * Returns a new Caller that runs the middleware before/after each call.
   * Middleware is applied in order: first added runs first on pre,
   * and last on post (onion model).
   *
   * @param middleware - Middleware to add
   * @returns New caller with middleware applied
   */
  with(middleware: ClientMiddleware): Caller;
}

/**
 * Caller implementation that applies middleware around another Caller.
 *
 * Handles the pre/post middleware lifecycle:
 * 1. Create context with extensions
 * 2. Build CallRequest from CallerRequest
 * 3. Run pre() hooks (can reject or modify request)
 * 4. Call inner caller
 * 5. Run post() hooks with outcome
 * 6. Return result or throw error
 */
export class MiddlewareCaller implements Caller {
  private inner: Caller;
  private middlewares: ClientMiddleware[];

  constructor(inner: Caller, middlewares: ClientMiddleware[]) {
    this.inner = inner;
    this.middlewares = middlewares;
  }

  async call(request: CallerRequest): Promise<unknown> {
    // Create context for this call
    const ctx: ClientContext = {
      extensions: new Extensions(),
    };

    // Build the CallRequest that middleware can inspect/modify
    const callRequest: CallRequest = {
      method: request.method,
      args: { ...request.args },
      metadata: request.metadata ? request.metadata.clone() : new ClientMetadata(),
    };

    // Run pre hooks
    for (const mw of this.middlewares) {
      if (mw.pre) {
        const rejection = await mw.pre(ctx, callRequest);
        if (rejection) {
          const error = RejectionError.from(rejection);
          // Run post hooks with rejection
          await this.runPostHooks(ctx, callRequest, { ok: false, error });
          throw error;
        }
      }
    }

    // Build the actual request with potentially modified args/metadata
    const finalRequest: CallerRequest = {
      ...request,
      args: callRequest.args,
      metadata: callRequest.metadata,
    };

    let outcome: CallOutcome;

    try {
      const value = await this.inner.call(finalRequest);
      outcome = { ok: true, value };
    } catch (e) {
      const error = e instanceof Error ? e : new Error(String(e));
      outcome = { ok: false, error };
      // Run post hooks with error
      await this.runPostHooks(ctx, callRequest, outcome);
      throw e;
    }

    // Run post hooks with actual outcome
    await this.runPostHooks(ctx, callRequest, outcome);

    return outcome.value;
  }

  private async runPostHooks(
    ctx: ClientContext,
    request: CallRequest,
    outcome: CallOutcome,
  ): Promise<void> {
    // Run post hooks in reverse order (onion model)
    for (let i = this.middlewares.length - 1; i >= 0; i--) {
      const mw = this.middlewares[i];
      if (mw.post) {
        try {
          await mw.post(ctx, request, outcome);
        } catch {
          // Post hooks should not throw, but swallow errors to ensure all run
        }
      }
    }
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.inner.getChannelAllocator();
  }

  getChannelRegistry(): ChannelRegistry {
    return this.inner.getChannelRegistry();
  }

  with(middleware: ClientMiddleware): Caller {
    return new MiddlewareCaller(this.inner, [...this.middlewares, middleware]);
  }
}
