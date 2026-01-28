// Client-side middleware types for TypeScript roam clients.
//
// Middleware allows intercepting and modifying requests/responses,
// enabling patterns like auth injection, tracing, logging, and observability.

import type { ClientMetadata } from "./metadata.ts";

// Re-export for backwards compatibility
export type { ClientMetadataValue } from "./metadata.ts";

/**
 * Extensions provide type-safe, symbol-keyed storage for middleware state.
 *
 * Each middleware can define a unique symbol and store/retrieve typed data
 * without conflicts with other middleware.
 *
 * @example
 * ```typescript
 * const AUTH_KEY = Symbol("auth");
 * ctx.extensions.set(AUTH_KEY, { userId: "123" });
 * const auth = ctx.extensions.get(AUTH_KEY); // typed as { userId: string }
 * ```
 */
export class Extensions {
  private data = new Map<symbol, unknown>();

  /**
   * Set a value for a symbol key.
   */
  set<T>(key: symbol, value: T): void {
    this.data.set(key, value);
  }

  /**
   * Get a value by symbol key.
   */
  get<T>(key: symbol): T | undefined {
    return this.data.get(key) as T | undefined;
  }

  /**
   * Check if a key exists.
   */
  has(key: symbol): boolean {
    return this.data.has(key);
  }

  /**
   * Remove a value by key.
   */
  delete(key: symbol): boolean {
    return this.data.delete(key);
  }
}

/**
 * Context passed to middleware hooks.
 *
 * Contains shared state (extensions) that persists across pre/post hooks
 * for a single call.
 */
export interface ClientContext {
  /**
   * Type-safe extensions storage for middleware state.
   */
  extensions: Extensions;
}

/**
 * Represents an outgoing RPC request.
 *
 * Middleware can read and modify the request before it's sent.
 */
export interface CallRequest {
  /**
   * Fully qualified method name (e.g., "Testbed.echo").
   */
  readonly method: string;

  /**
   * Method arguments as a record.
   * Middleware can inspect or modify these before encoding.
   */
  args: Record<string, unknown>;

  /**
   * Request metadata (headers).
   * Middleware can add/modify entries for auth, tracing, etc.
   * Use `set()` for normal metadata, `setSensitive()` for sensitive values.
   */
  metadata: ClientMetadata;
}

/**
 * Represents the outcome of an RPC call.
 */
export type CallOutcome =
  | { ok: true; value: unknown }
  | { ok: false; error: RejectionError }
  | { ok: false; error: Error };

/**
 * Rejection codes for middleware rejections.
 */
export type RejectionCode =
  | "unauthenticated"
  | "permission-denied"
  | "rate-limited"
  | "invalid-request"
  | "internal"
  | string;

/**
 * Rejection returned by middleware to abort a request.
 */
export interface Rejection {
  code: RejectionCode;
  message: string;
}

/**
 * Error thrown when middleware rejects a request.
 */
export class RejectionError extends Error {
  public readonly code: RejectionCode;

  constructor(rejection: Rejection) {
    super(rejection.message);
    this.name = "RejectionError";
    this.code = rejection.code;
  }

  static from(rejection: Rejection): RejectionError {
    return new RejectionError(rejection);
  }
}

/**
 * Client middleware interface.
 *
 * Middleware can intercept requests before they're sent (pre) and
 * observe/modify responses after they're received (post).
 *
 * @example
 * ```typescript
 * const authMiddleware: ClientMiddleware = {
 *   pre(ctx, request) {
 *     // Use setSensitive for auth tokens so they're redacted in logs
 *     request.metadata.setSensitive("authorization", `Bearer ${getToken()}`);
 *   }
 * };
 *
 * const loggingMiddleware: ClientMiddleware = {
 *   pre(ctx, request) {
 *     ctx.extensions.set(START_TIME, Date.now());
 *     console.log(`-> ${request.method}`);
 *   },
 *   post(ctx, request, outcome) {
 *     const start = ctx.extensions.get(START_TIME);
 *     const duration = Date.now() - start;
 *     console.log(`<- ${request.method} (${duration}ms)`);
 *   }
 * };
 * ```
 */
export interface ClientMiddleware {
  /**
   * Called before the request is sent.
   *
   * Can modify the request (add metadata, change args) or reject the call
   * by returning a Rejection.
   *
   * @param ctx - Context with extensions storage
   * @param request - The outgoing request (mutable)
   * @returns void to continue, Rejection to abort
   */
  pre?(ctx: ClientContext, request: CallRequest): Promise<Rejection | void> | Rejection | void;

  /**
   * Called after the response is received.
   *
   * Can observe the outcome for logging, metrics, etc.
   * Cannot modify the response.
   *
   * @param ctx - Context with extensions storage
   * @param request - The original request (for correlation)
   * @param outcome - The call result or error
   */
  post?(ctx: ClientContext, request: CallRequest, outcome: CallOutcome): Promise<void> | void;
}
