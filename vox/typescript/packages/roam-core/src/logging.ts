// Logging middleware for roam clients.
//
// Provides request/response logging with timing information.

import type { ClientMiddleware, ClientContext, CallRequest, CallOutcome } from "./middleware.ts";

const START_TIME = Symbol("logging:start-time");

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
      let message = `← ${request.method}: ${duration.toFixed(2)}ms`;

      if (outcome.ok) {
        if (logResults) {
          const resultStr = typeof outcome.value === "string"
            ? `"${outcome.value}"`
            : JSON.stringify(outcome.value);
          message += ` → ${resultStr}`;
        }
      } else {
        // Log errors regardless of logResults setting
        const error = outcome.error;
        if (error instanceof Error) {
          message += ` ✗ ${error.name}: ${error.message}`;
        } else {
          message += ` ✗ ${error}`;
        }
      }

      logger(message);
    },
  };
}
