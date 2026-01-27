// Logging middleware for roam clients.
//
// Provides request/response logging with timing information.
// Uses localStorage.debug pattern matching (like npm's debug package).

import type { ClientMiddleware, ClientContext, CallRequest, CallOutcome } from "./middleware.ts";
import { RpcError, RpcErrorCode } from "@bearcove/roam-wire";

const START_TIME = Symbol("logging:start-time");

/**
 * Error decoder function type.
 * Takes the method name and error payload, returns a string representation.
 */
export type ErrorDecoder = (method: string, payload: Uint8Array) => string | null;

export interface LoggingOptions {
  /**
   * Namespace for debug matching. Defaults to "roam:rpc".
   * Logging is enabled when localStorage.debug matches this namespace.
   * Supports patterns like "roam:*" or "*".
   */
  namespace?: string;

  /**
   * Log request arguments. Defaults to true.
   */
  logArgs?: boolean;

  /**
   * Log response values. Defaults to true.
   */
  logResults?: boolean;

  /**
   * Log metadata. Defaults to false (can contain sensitive data).
   */
  logMetadata?: boolean;

  /**
   * Minimum duration (ms) to log. Requests faster than this are skipped.
   * Defaults to 0 (log all requests).
   */
  minDuration?: number;

  /**
   * Custom error decoder for user errors.
   * If provided, will be called to decode error payloads into human-readable strings.
   * Return null to fall back to default "(N bytes)" display.
   */
  errorDecoder?: ErrorDecoder;
}

/**
 * Check if a namespace is enabled based on localStorage.debug pattern.
 * Supports wildcards (*) and exclusions (-prefix).
 */
function isEnabled(namespace: string): boolean {
  if (typeof localStorage === "undefined") return false;

  const debug = localStorage.getItem("debug");
  if (!debug) return false;

  const patterns = debug.split(/[\s,]+/).filter(Boolean);
  let enabled = false;

  for (const pattern of patterns) {
    if (pattern.startsWith("-")) {
      // Exclusion pattern
      const excluded = pattern.slice(1);
      if (matchPattern(namespace, excluded)) {
        enabled = false;
      }
    } else {
      // Inclusion pattern
      if (matchPattern(namespace, pattern)) {
        enabled = true;
      }
    }
  }

  return enabled;
}

/**
 * Match a namespace against a pattern with wildcard support.
 */
function matchPattern(namespace: string, pattern: string): boolean {
  if (pattern === "*") return true;

  // Convert glob pattern to regex
  const regexStr = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // Escape special chars except *
    .replace(/\*/g, ".*"); // Convert * to .*

  const regex = new RegExp(`^${regexStr}$`);
  return regex.test(namespace);
}

/**
 * Create a logging middleware that logs all RPC calls with timing information.
 * Logging is controlled by localStorage.debug (like npm's debug package).
 *
 * To enable logging in the browser console:
 * ```javascript
 * localStorage.debug = 'roam:*'  // Enable all roam logging
 * localStorage.debug = 'roam:rpc'  // Enable only RPC logging
 * localStorage.debug = '*'  // Enable everything
 * ```
 *
 * Logs structured objects to the console for easy inspection:
 * - Request: { type: "request", method, args, metadata? }
 * - Response: { type: "response", method, duration, result?, error? }
 *
 * @example
 * ```typescript
 * const caller = connection.asCaller().with(loggingMiddleware());
 * const client = new TestbedClient(caller);
 * await client.echo("hello");
 * // Console shows expandable objects with full request/response data
 * ```
 */
export function loggingMiddleware(options: LoggingOptions = {}): ClientMiddleware {
  const namespace = options.namespace ?? "roam:rpc";
  const logArgs = options.logArgs ?? true;
  const logResults = options.logResults ?? true;
  const logMetadata = options.logMetadata ?? false;
  const minDuration = options.minDuration ?? 0;
  const errorDecoder = options.errorDecoder;

  return {
    pre(ctx: ClientContext, request: CallRequest): void {
      // Store start time in extensions
      ctx.extensions.set(START_TIME, performance.now());

      // Check if logging is enabled
      if (!isEnabled(namespace)) return;

      // Build structured log object
      const logObj: Record<string, unknown> = {
        type: "request",
        method: request.method,
      };

      if (logArgs && Object.keys(request.args).length > 0) {
        logObj.args = request.args;
      }

      if (logMetadata && request.metadata.size > 0) {
        logObj.metadata = Object.fromEntries(request.metadata.entries());
      }

      console.log(`→ ${request.method}`, logObj);
    },

    post(ctx: ClientContext, request: CallRequest, outcome: CallOutcome): void {
      const startTime = ctx.extensions.get<number>(START_TIME);
      if (startTime === undefined) return;

      const duration = performance.now() - startTime;

      // Skip if below minimum duration
      if (duration < minDuration) return;

      // Check if logging is enabled
      if (!isEnabled(namespace)) return;

      // Build structured log object
      const logObj: Record<string, unknown> = {
        type: "response",
        method: request.method,
        duration: `${duration.toFixed(2)}ms`,
      };

      if (outcome.ok) {
        logObj.ok = true;
        if (logResults && outcome.value !== undefined) {
          logObj.result = outcome.value;
        }
        console.log(`← ${request.method}: ✓ ${duration.toFixed(2)}ms`, logObj);
      } else {
        logObj.ok = false;
        const error = outcome.error;

        if (error instanceof RpcError) {
          const codeStr = rpcErrorCodeToString(error.code);
          logObj.errorCode = codeStr;

          if (error.isUserError() && error.payload) {
            // Try to decode the error payload
            let decoded: string | null = null;
            if (errorDecoder) {
              try {
                decoded = errorDecoder(request.method, error.payload);
              } catch {
                // Decoder failed, fall back to bytes display
              }
            }
            if (decoded) {
              logObj.error = decoded;
            } else {
              logObj.errorPayloadBytes = error.payload.length;
            }
          }
        } else if (error instanceof Error) {
          logObj.error = {
            name: error.name,
            message: error.message,
          };
        } else {
          logObj.error = error;
        }

        console.log(`← ${request.method}: ✗ ${duration.toFixed(2)}ms`, logObj);
      }
    },
  };
}

/** Convert RPC error code to human-readable string */
function rpcErrorCodeToString(code: number): string {
  switch (code) {
    case RpcErrorCode.USER:
      return "user_error";
    case RpcErrorCode.UNKNOWN_METHOD:
      return "unknown_method";
    case RpcErrorCode.INVALID_PAYLOAD:
      return "invalid_payload";
    case RpcErrorCode.CANCELLED:
      return "cancelled";
    default:
      return `error_${code}`;
  }
}
