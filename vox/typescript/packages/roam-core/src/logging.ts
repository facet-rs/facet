// Logging middleware for roam clients.
//
// Provides request/response logging with timing information.

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
   * Custom logger function. Defaults to console.log.
   */
  logger?: (message: string) => void;

  /**
   * Log request arguments. Defaults to true.
   */
  logArgs?: boolean;

  /**
   * Log response values. Defaults to false (can be large).
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
 * Create a logging middleware that logs all RPC calls with timing information.
 *
 * @example
 * ```typescript
 * const caller = connection.asCaller().with(loggingMiddleware());
 * const client = new TestbedClient(caller);
 * await client.echo("hello"); // Logs: -> Testbed.echo(message="hello")
 *                              //       <- Testbed.echo: 15ms
 * ```
 *
 * @example With custom logger
 * ```typescript
 * const caller = connection.asCaller().with(
 *   loggingMiddleware({ logger: (msg) => myLogger.info(msg) })
 * );
 * ```
 *
 * @example Log results too
 * ```typescript
 * const caller = connection.asCaller().with(
 *   loggingMiddleware({ logArgs: true, logResults: true })
 * );
 * ```
 */
export function loggingMiddleware(options: LoggingOptions = {}): ClientMiddleware {
  const logger = options.logger ?? console.log;
  const logArgs = options.logArgs ?? true;
  const logResults = options.logResults ?? false;
  const logMetadata = options.logMetadata ?? false;
  const minDuration = options.minDuration ?? 0;
  const errorDecoder = options.errorDecoder;

  return {
    pre(ctx: ClientContext, request: CallRequest): void {
      // Store start time in extensions
      ctx.extensions.set(START_TIME, performance.now());

      // Build log message
      let message = `→ ${request.method}`;

      if (logArgs && Object.keys(request.args).length > 0) {
        const argsStr = Object.entries(request.args)
          .map(([key, value]) => {
            const valueStr = typeof value === "string"
              ? `"${value}"`
              : JSON.stringify(value);
            return `${key}=${valueStr}`;
          })
          .join(", ");
        message += `(${argsStr})`;
      }

      if (logMetadata && request.metadata.size > 0) {
        const metaStr = Array.from(request.metadata.entries())
          .map(([key, value]) => `${key}=${value}`)
          .join(", ");
        message += ` [${metaStr}]`;
      }

      logger(message);
    },

    post(ctx: ClientContext, request: CallRequest, outcome: CallOutcome): void {
      const startTime = ctx.extensions.get<number>(START_TIME);
      if (startTime === undefined) return;

      const duration = performance.now() - startTime;

      // Skip if below minimum duration
      if (duration < minDuration) return;

      // Build log message
      let message = `← ${request.method}:`;

      if (outcome.ok) {
        message += ` ✓ ${duration.toFixed(2)}ms`;
        if (logResults) {
          const resultStr = typeof outcome.value === "string"
            ? `"${outcome.value}"`
            : JSON.stringify(outcome.value);
          message += ` → ${resultStr}`;
        }
      } else {
        // Log errors regardless of logResults setting
        message += ` ✗ ${duration.toFixed(2)}ms`;
        const error = outcome.error;
        if (error instanceof RpcError) {
          // RPC error - show code and decoded error if possible
          const codeStr = rpcErrorCodeToString(error.code);
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
              message += ` ${codeStr}: ${decoded}`;
            } else {
              message += ` ${codeStr} (${error.payload.length} bytes)`;
            }
          } else {
            message += ` ${codeStr}`;
          }
        } else if (error instanceof Error) {
          message += ` ${error.name}: ${error.message}`;
        } else {
          message += ` ${error}`;
        }
      }

      logger(message);
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
