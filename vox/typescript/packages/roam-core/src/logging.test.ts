// Tests for logging middleware

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { loggingMiddleware } from "./logging.ts";
import { Extensions } from "./middleware.ts";
import type { ClientContext, CallRequest, CallOutcome } from "./middleware.ts";

// Mock localStorage
const mockLocalStorage: Record<string, string> = {};
vi.stubGlobal("localStorage", {
  getItem: (key: string) => mockLocalStorage[key] ?? null,
  setItem: (key: string, value: string) => {
    mockLocalStorage[key] = value;
  },
  removeItem: (key: string) => {
    delete mockLocalStorage[key];
  },
});

describe("loggingMiddleware", () => {
  let consoleLogs: Array<{ message: string; data: unknown }> = [];
  const originalConsoleLog = console.log;

  beforeEach(() => {
    consoleLogs = [];
    // Mock console.log to capture structured logs
    console.log = (message: string, data?: unknown) => {
      consoleLogs.push({ message, data });
    };
    // Enable logging by default for tests
    mockLocalStorage["debug"] = "roam:*";
  });

  afterEach(() => {
    console.log = originalConsoleLog;
    delete mockLocalStorage["debug"];
  });

  it("logs basic request and response", async () => {
    const middleware = loggingMiddleware();
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    // Pre hook
    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(1);
    expect(consoleLogs[0].message).toBe("→ Service.method");
    expect(consoleLogs[0].data).toMatchObject({
      type: "request",
      method: "Service.method",
    });

    // Simulate some time passing
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Post hook
    const outcome: CallOutcome = { ok: true, value: "result" };
    middleware.post?.(ctx, request, outcome);
    expect(consoleLogs).toHaveLength(2);
    expect(consoleLogs[1].message).toMatch(/← Service\.method: ✓/);
    // When logResults is enabled (default), the data is the result value directly
    expect(consoleLogs[1].data).toBe("result");
  });

  it("logs request arguments when enabled", () => {
    const middleware = loggingMiddleware({ logArgs: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.echo",
      args: { message: "hello", count: 42 },
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs[0].data).toMatchObject({
      args: { message: "hello", count: 42 },
    });
  });

  it("does not log arguments when disabled", () => {
    const middleware = loggingMiddleware({ logArgs: false });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.echo",
      args: { message: "hello" },
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs[0].data).not.toHaveProperty("args");
  });

  it("logs results when enabled", async () => {
    const middleware = loggingMiddleware({ logResults: true });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    const outcome: CallOutcome = { ok: true, value: { foo: "bar" } };
    middleware.post?.(ctx, request, outcome);

    // When results are enabled, the logged data is the result value directly (not wrapped in metadata)
    expect(consoleLogs[1].data).toMatchObject({ foo: "bar" });
  });

  it("does not log results when disabled", async () => {
    const middleware = loggingMiddleware({ logResults: false });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    const outcome: CallOutcome = { ok: true, value: { foo: "bar" } };
    middleware.post?.(ctx, request, outcome);

    // When results are disabled, we log the metadata object which has ok but no result
    expect(consoleLogs[1].data).toMatchObject({ ok: true });
    expect(consoleLogs[1].data).not.toHaveProperty("result");
  });

  it("logs metadata when enabled", () => {
    const middleware = loggingMiddleware({ logMetadata: true });
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
    expect(consoleLogs[0].data).toMatchObject({
      metadata: {
        authorization: "Bearer token",
        "trace-id": "123",
      },
    });
  });

  it("logs errors", async () => {
    const middleware = loggingMiddleware();
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

    expect(consoleLogs[1].message).toMatch(/← Service\.method: ✗/);
    expect(consoleLogs[1].data).toMatchObject({
      ok: false,
      error: {
        name: "Error",
        message: "Something went wrong",
      },
    });
  });

  it("skips logging fast requests when minDuration is set", async () => {
    const middleware = loggingMiddleware({ minDuration: 100 });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.fast",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(1);

    // Don't wait - should be very fast
    const outcome: CallOutcome = { ok: true, value: "result" };
    middleware.post?.(ctx, request, outcome);

    // Post log should be skipped because duration < 100ms
    expect(consoleLogs).toHaveLength(1);
  });

  it("logs slow requests when minDuration is set", async () => {
    const middleware = loggingMiddleware({ minDuration: 5 });
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
    expect(consoleLogs).toHaveLength(2);
  });

  it("does not log when debug is not enabled", () => {
    delete mockLocalStorage["debug"];

    const middleware = loggingMiddleware();
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(0);
  });

  it("respects namespace patterns", () => {
    mockLocalStorage["debug"] = "other:*";

    const middleware = loggingMiddleware({ namespace: "roam:rpc" });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(0);

    // Now enable the right namespace
    mockLocalStorage["debug"] = "roam:rpc";
    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(1);
  });

  it("supports wildcard patterns", () => {
    mockLocalStorage["debug"] = "*";

    const middleware = loggingMiddleware({ namespace: "anything:here" });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(1);
  });

  it("supports exclusion patterns", () => {
    mockLocalStorage["debug"] = "*,-roam:rpc";

    const middleware = loggingMiddleware({ namespace: "roam:rpc" });
    const ctx: ClientContext = { extensions: new Extensions() };
    const request: CallRequest = {
      method: "Service.method",
      args: {},
      metadata: new Map(),
    };

    middleware.pre?.(ctx, request);
    expect(consoleLogs).toHaveLength(0);
  });
});
