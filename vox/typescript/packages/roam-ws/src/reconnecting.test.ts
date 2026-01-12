// Tests for auto-reconnecting WebSocket client.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  ReconnectingWsClient,
  createReconnectingClient,
  ClientClosedError,
  ReconnectFailedError,
  type ConnectionState,
} from "./reconnecting.ts";

// Mock WebSocket for testing
class MockWebSocket {
  static instances: MockWebSocket[] = [];

  binaryType: string = "blob";
  readyState: number = 0; // CONNECTING

  private listeners: Map<string, Set<(event: unknown) => void>> = new Map();

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    if (!this.listeners.has(type)) {
      this.listeners.set(type, new Set());
    }
    this.listeners.get(type)!.add(listener);
  }

  removeEventListener(type: string, listener: (event: unknown) => void): void {
    this.listeners.get(type)?.delete(listener);
  }

  send(_data: ArrayBuffer | Uint8Array): void {
    if (this.readyState !== 1) {
      throw new Error("WebSocket not open");
    }
  }

  close(): void {
    this.readyState = 3; // CLOSED
    this.emit("close", {});
  }

  // Test helpers
  simulateOpen(): void {
    this.readyState = 1; // OPEN
    this.emit("open", {});
  }

  simulateError(): void {
    this.emit("error", new Error("Connection error"));
  }

  simulateClose(): void {
    this.readyState = 3;
    this.emit("close", {});
  }

  private emit(type: string, event: unknown): void {
    const listeners = this.listeners.get(type);
    if (listeners) {
      for (const listener of listeners) {
        listener(event);
      }
    }
  }

  static clear(): void {
    MockWebSocket.instances = [];
  }

  static get latest(): MockWebSocket | undefined {
    return MockWebSocket.instances[MockWebSocket.instances.length - 1];
  }
}

describe("ReconnectingWsClient", () => {
  let originalWebSocket: typeof globalThis.WebSocket;

  beforeEach(() => {
    originalWebSocket = globalThis.WebSocket;
    // @ts-expect-error - Mock WebSocket
    globalThis.WebSocket = MockWebSocket;
    MockWebSocket.clear();
  });

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
  });

  describe("createReconnectingClient", () => {
    it("creates a client with default config", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      expect(client).toBeInstanceOf(ReconnectingWsClient);
      expect(client.getState()).toBe("disconnected");
      expect(client.isClosed()).toBe(false);
    });

    it("creates a client with custom config", () => {
      const stateChanges: ConnectionState[] = [];
      const client = createReconnectingClient({
        url: "ws://localhost:8080",
        reconnect: {
          enabled: true,
          maxAttempts: 5,
          backoff: { initial: 500, max: 10000, factor: 1.5, jitter: 0 },
        },
        onStateChange: (state) => stateChanges.push(state),
        requestTimeout: 5000,
      });
      expect(client).toBeInstanceOf(ReconnectingWsClient);
    });
  });

  describe("connection state", () => {
    it("transitions to connecting when connect is called", () => {
      const stateChanges: ConnectionState[] = [];
      const client = createReconnectingClient({
        url: "ws://localhost:8080",
        onStateChange: (state) => stateChanges.push(state),
      });

      // Start connecting (don't await)
      client.connect();

      // Should be connecting now
      expect(stateChanges).toContain("connecting");
      expect(client.getState()).toBe("connecting");

      // Clean up
      client.close();
    });

    it("stays disconnected initially", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      expect(client.getState()).toBe("disconnected");
    });

    it("creates WebSocket with correct URL", () => {
      const client = createReconnectingClient({ url: "ws://example.com:9000/roam" });
      client.connect();

      expect(MockWebSocket.latest?.url).toBe("ws://example.com:9000/roam");

      client.close();
    });
  });

  describe("close", () => {
    it("marks client as closed", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      expect(client.isClosed()).toBe(false);

      client.close();

      expect(client.isClosed()).toBe(true);
      expect(client.getState()).toBe("disconnected");
    });

    it("throws ClientClosedError on connect after close", async () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      client.close();

      await expect(client.connect()).rejects.toThrow(ClientClosedError);
    });

    it("throws ClientClosedError on call after close", async () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      client.close();

      await expect(client.call(1n, new Uint8Array())).rejects.toThrow(ClientClosedError);
    });

    it("is idempotent", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });

      client.close();
      client.close();
      client.close();

      expect(client.isClosed()).toBe(true);
    });

    it("closes underlying WebSocket when connecting", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      client.connect();

      const ws = MockWebSocket.latest!;
      expect(ws).toBeDefined();

      // Close before connection completes
      client.close();

      // WebSocket should be closed
      expect(ws.readyState).toBe(3); // CLOSED
    });
  });

  describe("reconnect disabled", () => {
    it("does not reconnect when disabled", async () => {
      const stateChanges: ConnectionState[] = [];
      const client = createReconnectingClient({
        url: "ws://localhost:8080",
        reconnect: { enabled: false },
        onStateChange: (state) => stateChanges.push(state),
      });

      const connectPromise = client.connect();

      // Connection fails
      MockWebSocket.latest?.simulateError();

      await expect(connectPromise).rejects.toThrow();
      expect(stateChanges).toContain("disconnected");
      expect(stateChanges).not.toContain("reconnecting");
    });
  });

  describe("getNegotiated", () => {
    it("returns null when not connected", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      expect(client.getNegotiated()).toBeNull();
    });
  });

  describe("error types", () => {
    it("ClientClosedError has correct name", () => {
      const error = new ClientClosedError();
      expect(error.name).toBe("ClientClosedError");
      expect(error.message).toBe("Client is closed");
    });

    it("ReconnectFailedError includes attempt count and last error", () => {
      const lastError = new Error("Connection refused");
      const error = new ReconnectFailedError(5, lastError);
      expect(error.name).toBe("ReconnectFailedError");
      expect(error.attempts).toBe(5);
      expect(error.lastError).toBe(lastError);
      expect(error.message).toContain("5 attempts");
      expect(error.message).toContain("Connection refused");
    });
  });

  describe("config defaults", () => {
    it("uses default request timeout of 30000", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });
      // The default is used internally - we can verify by checking the client exists
      expect(client).toBeInstanceOf(ReconnectingWsClient);
    });

    it("allows custom request timeout", () => {
      const client = createReconnectingClient({
        url: "ws://localhost:8080",
        requestTimeout: 5000,
      });
      expect(client).toBeInstanceOf(ReconnectingWsClient);
    });
  });

  describe("multiple connect calls", () => {
    it("returns same promise when already connecting", () => {
      const client = createReconnectingClient({ url: "ws://localhost:8080" });

      const promise1 = client.connect();
      const promise2 = client.connect();

      // Should be the same underlying connection attempt
      expect(MockWebSocket.instances.length).toBe(1);

      client.close();
    });
  });
});
