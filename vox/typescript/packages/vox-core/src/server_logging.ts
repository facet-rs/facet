import { MetadataFlagValues } from "@bearcove/vox-wire";
import type { RequestContext } from "./request_context.ts";
import type { ServerCallOutcome, ServerMiddleware } from "./server_middleware.ts";

const START_TIME = Symbol("server-logging:start-time");

export interface ServerLoggingOptions {
  logMetadata?: boolean;
  logger?: Pick<Console, "debug">;
}

function metadataForLog(context: RequestContext): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value, flags] of context.metadata) {
    if ((flags & MetadataFlagValues.SENSITIVE) !== 0n) {
      out[key] = "[REDACTED]";
    } else if (value instanceof Uint8Array) {
      out[key] = `<${value.length} bytes>`;
    } else {
      out[key] = value;
    }
  }
  return out;
}

export function serverLoggingMiddleware(
  options: ServerLoggingOptions = {},
): ServerMiddleware {
  const logger = options.logger ?? console;
  const logMetadata = options.logMetadata ?? false;

  return {
    pre(context: RequestContext): void {
      context.extensions.set(START_TIME, performance.now());
      const logObj: Record<string, unknown> = {
        service: context.serviceName,
        method: context.method.name,
      };
      if (logMetadata && context.metadata.size > 0) {
        logObj.metadata = metadataForLog(context);
      }
      logger.debug("vox:server:request", logObj);
    },

    post(context: RequestContext, outcome: ServerCallOutcome): void {
      const start = context.extensions.get<number>(START_TIME);
      const durationMs = start === undefined ? undefined : performance.now() - start;
      const logObj: Record<string, unknown> = {
        service: context.serviceName,
        method: context.method.name,
        outcome: outcome.kind,
      };
      if (durationMs !== undefined) {
        logObj.duration_ms = Number(durationMs.toFixed(2));
      }
      if (outcome.kind === "failed") {
        logObj.error = outcome.error instanceof Error
          ? { name: outcome.error.name, message: outcome.error.message }
          : String(outcome.error);
      }
      logger.debug("vox:server:response", logObj);
    },
  };
}
