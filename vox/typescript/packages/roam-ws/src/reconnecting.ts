// Auto-reconnecting WebSocket client for roam.
//
// Provides automatic reconnection with exponential backoff, connection state
// events, and request queuing during reconnection.

import {
  Connection,
  helloExchangeInitiator,
  defaultHello,
  ConnectionError,
  type Negotiated,
} from "@bearcove/roam-core";
import { WsTransport } from "./transport.ts";

/** Connection state. */
export type ConnectionState = "disconnected" | "connecting" | "connected" | "reconnecting";

/** Backoff configuration for reconnection attempts. */
export interface BackoffConfig {
  /** Initial delay in milliseconds. Default: 1000 */
  initial: number;
  /** Maximum delay in milliseconds. Default: 30000 */
  max: number;
  /** Multiplier for exponential backoff. Default: 2 */
  factor: number;
  /** Jitter factor (0-1) to randomize delays. Default: 0.1 */
  jitter: number;
}

/** Configuration for the reconnecting client. */
export interface ReconnectingClientConfig {
  /** WebSocket URL to connect to. */
  url: string;

  /** Reconnection configuration. */
  reconnect?: {
    /** Whether reconnection is enabled. Default: true */
    enabled?: boolean;
    /** Maximum number of reconnection attempts. Default: Infinity */
    maxAttempts?: number;
    /** Backoff configuration. */
    backoff?: Partial<BackoffConfig>;
  };

  /** Called when connection state changes. */
  onStateChange?: (state: ConnectionState) => void;

  /** Called when a reconnection attempt starts. */
  onReconnectAttempt?: (attempt: number, delay: number) => void;

  /** Called when reconnection fails permanently. */
  onReconnectFailed?: (error: Error) => void;

  /** Timeout for pending requests in milliseconds. Default: 30000 */
  requestTimeout?: number;
}

/** Error thrown when client is permanently closed. */
export class ClientClosedError extends Error {
  constructor() {
    super("Client is closed");
    this.name = "ClientClosedError";
  }
}

/** Error thrown when reconnection fails. */
export class ReconnectFailedError extends Error {
  constructor(
    public attempts: number,
    public lastError: Error,
  ) {
    super(`Reconnection failed after ${attempts} attempts: ${lastError.message}`);
    this.name = "ReconnectFailedError";
  }
}

interface PendingRequest {
  methodId: bigint;
  payload: Uint8Array;
  resolve: (value: Uint8Array) => void;
  reject: (error: Error) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}

/**
 * Auto-reconnecting WebSocket client.
 *
 * This client automatically handles connection failures and reconnects
 * with exponential backoff. Requests made during disconnection are queued
 * and sent once the connection is re-established.
 */
export class ReconnectingWsClient {
  private url: string;
  private config: Required<
    Pick<ReconnectingClientConfig, "requestTimeout"> & {
      reconnect: {
        enabled: boolean;
        maxAttempts: number;
        backoff: BackoffConfig;
      };
    }
  >;

  private onStateChange?: (state: ConnectionState) => void;
  private onReconnectAttempt?: (attempt: number, delay: number) => void;
  private onReconnectFailed?: (error: Error) => void;

  private state: ConnectionState = "disconnected";
  private connection: Connection<WsTransport> | null = null;
  private ws: WebSocket | null = null;
  private pendingWs: WebSocket | null = null; // WebSocket being connected
  private closed = false;
  private reconnectAttempts = 0;
  private pendingRequests: PendingRequest[] = [];
  private connectPromise: Promise<void> | null = null;

  constructor(config: ReconnectingClientConfig) {
    this.url = config.url;
    this.onStateChange = config.onStateChange;
    this.onReconnectAttempt = config.onReconnectAttempt;
    this.onReconnectFailed = config.onReconnectFailed;

    const backoff = config.reconnect?.backoff ?? {};
    this.config = {
      requestTimeout: config.requestTimeout ?? 30000,
      reconnect: {
        enabled: config.reconnect?.enabled ?? true,
        maxAttempts: config.reconnect?.maxAttempts ?? Infinity,
        backoff: {
          initial: backoff.initial ?? 1000,
          max: backoff.max ?? 30000,
          factor: backoff.factor ?? 2,
          jitter: backoff.jitter ?? 0.1,
        },
      },
    };
  }

  /** Get the current connection state. */
  getState(): ConnectionState {
    return this.state;
  }

  /** Get the negotiated parameters (only valid when connected). */
  getNegotiated(): Negotiated | null {
    return this.connection?.negotiated() ?? null;
  }

  /** Check if the client has been permanently closed. */
  isClosed(): boolean {
    return this.closed;
  }

  /**
   * Connect to the server.
   *
   * This is called automatically on first request, but can be called
   * explicitly to establish the connection eagerly.
   */
  async connect(): Promise<void> {
    if (this.closed) {
      throw new ClientClosedError();
    }

    if (this.state === "connected") {
      return;
    }

    // If already connecting, wait for that attempt
    if (this.connectPromise) {
      return this.connectPromise;
    }

    this.connectPromise = this.doConnect();
    try {
      await this.connectPromise;
    } finally {
      this.connectPromise = null;
    }
  }

  private async doConnect(): Promise<void> {
    const isReconnect = this.state === "reconnecting";
    this.setState(isReconnect ? "reconnecting" : "connecting");

    try {
      // Create WebSocket
      const ws = new WebSocket(this.url);
      ws.binaryType = "arraybuffer";
      this.pendingWs = ws;

      // Wait for open
      await new Promise<void>((resolve, reject) => {
        const onOpen = () => {
          ws.removeEventListener("open", onOpen);
          ws.removeEventListener("error", onError);
          resolve();
        };
        const onError = () => {
          ws.removeEventListener("open", onOpen);
          ws.removeEventListener("error", onError);
          reject(new Error(`Failed to connect to ${this.url}`));
        };
        ws.addEventListener("open", onOpen);
        ws.addEventListener("error", onError);
      });

      // Check if closed while waiting
      if (this.closed) {
        ws.close();
        this.pendingWs = null;
        return;
      }

      // Perform Hello exchange
      const transport = new WsTransport(ws);
      const connection = await helloExchangeInitiator(transport, defaultHello());

      this.pendingWs = null;
      this.ws = ws;
      this.connection = connection;
      this.reconnectAttempts = 0;
      this.setState("connected");

      // Set up disconnect handler
      this.setupDisconnectHandler(ws);

      // Flush pending requests
      await this.flushPendingRequests();
    } catch (error) {
      // Connection failed
      if (this.config.reconnect.enabled && !this.closed) {
        await this.scheduleReconnect(error as Error);
      } else {
        this.setState("disconnected");
        throw error;
      }
    }
  }

  private setupDisconnectHandler(ws: WebSocket): void {
    const handleDisconnect = () => {
      if (this.ws !== ws) return; // Stale handler

      this.connection = null;
      this.ws = null;

      if (this.closed) {
        this.setState("disconnected");
        return;
      }

      if (this.config.reconnect.enabled) {
        this.setState("reconnecting");
        this.scheduleReconnect(new Error("Connection lost"));
      } else {
        this.setState("disconnected");
        this.failPendingRequests(new Error("Connection lost"));
      }
    };

    ws.addEventListener("close", handleDisconnect);
    ws.addEventListener("error", handleDisconnect);
  }

  private async scheduleReconnect(lastError: Error): Promise<void> {
    this.reconnectAttempts++;

    if (this.reconnectAttempts > this.config.reconnect.maxAttempts) {
      const error = new ReconnectFailedError(this.reconnectAttempts - 1, lastError);
      this.onReconnectFailed?.(error);
      this.failPendingRequests(error);
      this.setState("disconnected");
      return;
    }

    const delay = this.calculateBackoff();
    this.onReconnectAttempt?.(this.reconnectAttempts, delay);

    await this.sleep(delay);

    if (this.closed) return;

    try {
      await this.doConnect();
    } catch {
      // doConnect handles scheduling the next attempt
    }
  }

  private calculateBackoff(): number {
    const { initial, max, factor, jitter } = this.config.reconnect.backoff;
    const base = Math.min(initial * Math.pow(factor, this.reconnectAttempts - 1), max);
    const jitterAmount = base * jitter * (Math.random() * 2 - 1);
    return Math.max(0, Math.floor(base + jitterAmount));
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  private setState(state: ConnectionState): void {
    if (this.state !== state) {
      this.state = state;
      this.onStateChange?.(state);
    }
  }

  private async flushPendingRequests(): Promise<void> {
    const pending = [...this.pendingRequests];
    this.pendingRequests = [];

    for (const request of pending) {
      // Don't await - let them run concurrently
      this.doCall(request.methodId, request.payload).then(request.resolve, request.reject);
    }
  }

  private failPendingRequests(error: Error): void {
    const pending = [...this.pendingRequests];
    this.pendingRequests = [];

    for (const request of pending) {
      clearTimeout(request.timeoutId);
      request.reject(error);
    }
  }

  /**
   * Make an RPC call.
   *
   * If not connected, this will attempt to connect first. If disconnected
   * during the call, it will be retried after reconnection (up to the
   * request timeout).
   */
  async call(methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    if (this.closed) {
      throw new ClientClosedError();
    }

    // If connected, try the call directly
    if (this.state === "connected" && this.connection) {
      return this.doCall(methodId, payload);
    }

    // Otherwise, queue the request and ensure we're connecting
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        const idx = this.pendingRequests.findIndex(
          (r) => r.resolve === resolve && r.reject === reject,
        );
        if (idx !== -1) {
          this.pendingRequests.splice(idx, 1);
          reject(new Error("Request timed out waiting for connection"));
        }
      }, this.config.requestTimeout);

      this.pendingRequests.push({
        methodId,
        payload,
        resolve,
        reject,
        timeoutId,
      });

      // Trigger connection if not already connecting
      if (!this.connectPromise) {
        this.connect().catch(() => {
          // Error handling is done via pending request rejection
        });
      }
    });
  }

  private async doCall(methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    if (!this.connection) {
      throw new Error("Not connected");
    }

    try {
      return await this.connection.call(methodId, payload, this.config.requestTimeout);
    } catch (error) {
      // If connection error and reconnect is enabled, queue for retry
      if (error instanceof ConnectionError && this.config.reconnect.enabled && !this.closed) {
        return new Promise((resolve, reject) => {
          const timeoutId = setTimeout(() => {
            const idx = this.pendingRequests.findIndex(
              (r) => r.resolve === resolve && r.reject === reject,
            );
            if (idx !== -1) {
              this.pendingRequests.splice(idx, 1);
              reject(new Error("Request timed out during reconnection"));
            }
          }, this.config.requestTimeout);

          this.pendingRequests.push({
            methodId,
            payload,
            resolve,
            reject,
            timeoutId,
          });
        });
      }
      throw error;
    }
  }

  /**
   * Close the client permanently.
   *
   * This stops all reconnection attempts and fails any pending requests.
   */
  close(): void {
    if (this.closed) return;

    this.closed = true;
    this.failPendingRequests(new ClientClosedError());

    // Close any pending WebSocket (still connecting)
    if (this.pendingWs) {
      this.pendingWs.close();
      this.pendingWs = null;
    }

    // Close established WebSocket
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.connection = null;
    this.setState("disconnected");
  }
}

/**
 * Create a reconnecting WebSocket client.
 *
 * @example
 * ```typescript
 * const client = createReconnectingClient({
 *   url: "ws://localhost:8080/roam",
 *   reconnect: {
 *     enabled: true,
 *     maxAttempts: 10,
 *     backoff: { initial: 1000, max: 30000, factor: 2 },
 *   },
 *   onStateChange: (state) => console.log("Connection:", state),
 * });
 *
 * // Calls automatically wait for connection or fail after timeout
 * const response = await client.call(methodId, payload);
 * ```
 */
export function createReconnectingClient(config: ReconnectingClientConfig): ReconnectingWsClient {
  return new ReconnectingWsClient(config);
}
