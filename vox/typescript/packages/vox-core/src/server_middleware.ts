import type { RequestContext } from "./request_context.ts";

export type ServerCallOutcome =
  | { kind: "replied" }
  | { kind: "dropped" }
  | { kind: "failed"; error: unknown };

export interface ServerMiddleware {
  pre?(context: RequestContext): Promise<void> | void;
  post?(context: RequestContext, outcome: ServerCallOutcome): Promise<void> | void;
}

