// Tests for logging middleware

import { describe, it, expect, beforeEach } from "vitest";
import { loggingMiddleware } from "./logging.ts";
import { Extensions } from "./middleware.ts";
import type { ClientContext, CallRequest, CallOutcome } from "./middleware.ts";

describe("loggingMiddleware", () => {
  let logs: string[] = [];
  const mockLogger = (msg: string) => {
    logs.push(msg);
  };

  beforeEach(() => {
    logs = [];
  });

  it("logs basic request and response", async () => {
    const middleware = loggingMiddleware({ logger: mockLogger });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    // Pre hook
    middleware.pre?.(ctx, request);
    expect(logs).toHaveLength(1);
    expect(logs[0]).toBe("→ Service.method");

    // Simulate some time passing
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Post hook
    const outcome: CallOutcome = { ok: true, value: "result" };
    middleware.post?.(ctx, request, outcome);
    expect(logs).toHaveLength(2);
    expect(logs[1]).toMatch(/← Service\.method: \d+\.\d+ms/);
  });

  it("logs request arguments when enabled", () => {
    const middleware = loggingMiddleware({ logger: mockLogger, logArgs: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.echo",
      args: { message: "hello", count: 42 },
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(logs[0]).toBe('→ Service.echo(message="hello", count=42)');
  });

  it("does not log arguments when disabled", () => {
    const middleware = loggingMiddleware({ logger: mockLogger, logArgs: false });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.echo",
      args: { message: "hello" },
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(logs[0]).toBe("→ Service.echo");
  });

  it("logs results when enabled", async () => {
    const middleware = loggingMiddleware({ logger: mockLogger, logResults: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    const outcome: CallOutcome = { ok: true, value: "success" };
    middleware.post?.(ctx, request, outcome);

    expect(logs[1]).toMatch(/← Service\.method: \d+\.\d+ms → "success"/);
  });

  it("logs metadata when enabled", () => {
    const middleware = loggingMiddleware({ logger: mockLogger, logMetadata: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map([
        ["authorization", "Bearer token"],
        ["trace-id", "123"],
      ]),
    };

    middleware.pre?.(ctx, request);
    expect(logs[0]).toContain("[authorization=Bearer token, trace-id=123]");
  });

  it("logs errors", async () => {
    const middleware = loggingMiddleware({ logger: mockLogger });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    const error = new Error("Something went wrong");
    const outcome: CallOutcome = { ok: false, error };
    middleware.post?.(ctx, request, outcome);

    expect(logs[1]).toMatch(/← Service\.method: \d+\.\d+ms ✗ Error: Something went wrong/);
  });

  it("skips logging fast requests when minDuration is set", async () => {
    const middleware = loggingMiddleware({ logger: mockLogger, minDuration: 100 });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.fast",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(logs).toHaveLength(1);

    // Don't wait - should be very fast
    const outcome: CallOutcome = { ok: true, value: "result" };
    middleware.post?.(ctx, request, outcome);

    // Post log should be skipped because duration < 100ms
    expect(logs).toHaveLength(1);
  });

  it("logs slow requests when minDuration is set", async () => {
    const middleware = loggingMiddleware({ logger: mockLogger, minDuration: 5 });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.slow",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);

    // Wait to exceed minDuration
    await new Promise((resolve) => setTimeout(resolve, 10));

    const outcome: CallOutcome = { ok: true, value: "result" };
    middleware.post?.(ctx, request, outcome);

    // Should log both pre and post
    expect(logs).toHaveLength(2);
  });

  it("handles complex argument types", () => {
    const middleware = loggingMiddleware({ logger: mockLogger, logArgs: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.complex",
      args: {
        obj: { nested: "value" },
        arr: [1, 2, 3],
        num: 42,
        bool: true,
      },
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(logs[0]).toContain("Service.complex(");
    expect(logs[0]).toContain('obj={"nested":"value"}');
    expect(logs[0]).toContain("arr=[1,2,3]");
    expect(logs[0]).toContain("num=42");
    expect(logs[0]).toContain("bool=true");
  });

  it("uses console.log by default", () => {
    const originalLog = console.log;
    let logged = false;
    console.log = () => {
      logged = true;
    };

    const middleware = loggingMiddleware();
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(logged).toBe(true);

    console.log = originalLog;
  });
});
